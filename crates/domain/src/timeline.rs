use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EntityKind, EntityRef, Message, ModelError};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TimelineError {
    #[error("{field} has the wrong entity kind")]
    WrongKind { field: &'static str },
    #[error("{field} belongs to a different account")]
    AccountMismatch { field: &'static str },
    #[error("nested message is invalid: {0}")]
    InvalidMessage(#[from] ModelError),
    #[error("event type must not be empty")]
    EmptyEventType,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventPosition {
    pub provider_sequence: Option<String>,
    pub received_sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipChange {
    Joined,
    Left,
    Invited,
    Kicked,
    Banned,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum TimelinePayload {
    MessageCreated {
        message: Box<Message>,
    },
    MessageEdited {
        message_id: EntityRef,
        body: crate::MessageBody,
        edited_at_ms: i64,
    },
    MessageRedacted {
        message_id: EntityRef,
        reason: Option<String>,
    },
    ReactionAdded {
        message_id: EntityRef,
        key: String,
    },
    ReactionRemoved {
        message_id: EntityRef,
        key: String,
    },
    MembershipChanged {
        user_id: EntityRef,
        change: MembershipChange,
    },
    TypingChanged {
        user_ids: Vec<EntityRef>,
    },
    ReadMarkerChanged {
        user_id: EntityRef,
        event_id: EntityRef,
    },
    System {
        type_id: String,
        summary: String,
    },
    Unsupported {
        type_id: String,
        summary: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: EntityRef,
    pub conversation_id: EntityRef,
    pub sender_id: Option<EntityRef>,
    pub server_timestamp_ms: Option<i64>,
    pub position: EventPosition,
    pub payload: TimelinePayload,
}

impl TimelineEvent {
    pub fn validate(&self) -> Result<(), TimelineError> {
        require_kind(&self.id, EntityKind::Event, "id")?;
        require_kind(
            &self.conversation_id,
            EntityKind::Conversation,
            "conversation_id",
        )?;
        require_same_account(&self.id, &self.conversation_id, "conversation_id")?;
        if let Some(sender) = &self.sender_id {
            require_kind(sender, EntityKind::User, "sender_id")?;
            require_same_account(&self.id, sender, "sender_id")?;
        }

        match &self.payload {
            TimelinePayload::MessageCreated { message } => {
                message.validate()?;
                require_same_account(&self.id, &message.id, "message.id")?;
                require_same_account(
                    &self.conversation_id,
                    &message.conversation_id,
                    "message.conversation_id",
                )?;
                if let Some(sender) = &self.sender_id {
                    if sender != &message.sender_id {
                        return Err(TimelineError::AccountMismatch {
                            field: "message.sender_id",
                        });
                    }
                }
            }
            TimelinePayload::MessageEdited { message_id, .. }
            | TimelinePayload::MessageRedacted { message_id, .. }
            | TimelinePayload::ReactionAdded { message_id, .. }
            | TimelinePayload::ReactionRemoved { message_id, .. } => {
                require_kind(message_id, EntityKind::Message, "message_id")?;
                require_same_account(&self.conversation_id, message_id, "message_id")?;
            }
            TimelinePayload::MembershipChanged { user_id, .. } => {
                require_kind(user_id, EntityKind::User, "user_id")?;
                require_same_account(&self.conversation_id, user_id, "user_id")?;
            }
            TimelinePayload::TypingChanged { user_ids } => {
                for user in user_ids {
                    require_kind(user, EntityKind::User, "typing.user_id")?;
                    require_same_account(&self.conversation_id, user, "typing.user_id")?;
                }
            }
            TimelinePayload::ReadMarkerChanged { user_id, event_id } => {
                require_kind(user_id, EntityKind::User, "read_marker.user_id")?;
                require_kind(event_id, EntityKind::Event, "read_marker.event_id")?;
                require_same_account(&self.conversation_id, user_id, "read_marker.user_id")?;
                require_same_account(&self.conversation_id, event_id, "read_marker.event_id")?;
            }
            TimelinePayload::System { type_id, .. }
            | TimelinePayload::Unsupported { type_id, .. } => {
                if type_id.trim().is_empty() {
                    return Err(TimelineError::EmptyEventType);
                }
            }
        }
        Ok(())
    }
}

fn require_kind(
    entity: &EntityRef,
    expected: EntityKind,
    field: &'static str,
) -> Result<(), TimelineError> {
    if !entity.has_kind(&expected) {
        return Err(TimelineError::WrongKind { field });
    }
    Ok(())
}

fn require_same_account(
    left: &EntityRef,
    right: &EntityRef,
    field: &'static str,
) -> Result<(), TimelineError> {
    if left.account != right.account {
        return Err(TimelineError::AccountMismatch { field });
    }
    Ok(())
}
