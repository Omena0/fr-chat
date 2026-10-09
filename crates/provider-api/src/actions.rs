use fr_chat_domain::{
    AccountRef, Capability, EntityKind, EntityRef, MessageBody, MessageRelation, OpaqueId,
    ProviderId, SpaceChildOrder,
    PushRule, PushRuleKind, Pusher, PusherKind,
    SearchCriteria,
    VoipCallType, VoipSessionDescription, VoipIceCandidate, VoipHangupReason, VoipRejectReason,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum ProviderAction {
    SendMessage {
        conversation_id: EntityRef,
        body: MessageBody,
        attachments: Vec<EntityRef>,
        relation: Option<MessageRelation>,
    },
    EditMessage {
        conversation_id: EntityRef,
        message_id: EntityRef,
        body: MessageBody,
    },
    RedactMessage {
        conversation_id: EntityRef,
        message_id: EntityRef,
        reason: Option<String>,
    },
    AddReaction {
        conversation_id: EntityRef,
        message_id: EntityRef,
        key: String,
    },
    RemoveReaction {
        conversation_id: EntityRef,
        message_id: EntityRef,
        key: String,
    },
    MarkRead {
        conversation_id: EntityRef,
        event_id: EntityRef,
    },
    SetTyping {
        conversation_id: EntityRef,
        typing: bool,
    },
    JoinConversation {
        conversation_id: EntityRef,
    },
    LeaveConversation {
        conversation_id: EntityRef,
    },
    UploadMedia {
        conversation_id: EntityRef,
        filename: String,
        content_type: String,
        data: Vec<u8>,
    },
    CreateSpace {
        name: String,
        topic: Option<String>,
        avatar: Option<EntityRef>,
        parent_space_id: Option<EntityRef>,
        initial_children: Vec<EntityRef>,
    },
    AddSpaceChild {
        space_id: EntityRef,
        child_id: EntityRef,
        order: Option<SpaceChildOrder>,
        suggested: bool,
    },
    RemoveSpaceChild {
        space_id: EntityRef,
        child_id: EntityRef,
    },
    GetPushRules,
    SetPushRule {
        rule_kind: PushRuleKind,
        rule_id: String,
        rule: PushRule,
    },
    DeletePushRule {
        rule_kind: PushRuleKind,
        rule_id: String,
    },
    AddPusher {
        pusher: Pusher,
    },
    RemovePusher {
        pushkey: String,
        kind: PusherKind,
    },
    SetPusher {
        pushkey: String,
        kind: PusherKind,
        pusher: Pusher,
    },
    SearchEvents {
        criteria: SearchCriteria,
        next_batch: Option<String>,
    },
    VoipInvite {
        conversation_id: EntityRef,
        call_id: String,
        call_type: VoipCallType,
        offer: VoipSessionDescription,
        lifetime: Option<u64>,
    },
    VoipCandidates {
        conversation_id: EntityRef,
        call_id: String,
        candidates: Vec<VoipIceCandidate>,
    },
    VoipAnswer {
        conversation_id: EntityRef,
        call_id: String,
        answer: VoipSessionDescription,
    },
    VoipHangup {
        conversation_id: EntityRef,
        call_id: String,
        reason: Option<VoipHangupReason>,
    },
    VoipReject {
        conversation_id: EntityRef,
        call_id: String,
        reason: Option<VoipRejectReason>,
    },
    VoipSelectAnswer {
        conversation_id: EntityRef,
        call_id: String,
    },
    VoipNegotiate {
        conversation_id: EntityRef,
        call_id: String,
        offer: VoipSessionDescription,
    },
}

impl ProviderAction {
    pub fn capability_requirements(&self) -> Vec<Vec<Capability>> {
        match self {
            Self::SendMessage {
                attachments,
                relation,
                ..
            } => {
                let mut requirements = vec![vec![Capability::SendMessage]];
                if !attachments.is_empty() {
                    requirements.push(vec![Capability::UploadAttachment]);
                }
                if let Some(relation) = relation {
                    requirements.push(vec![match relation {
                        MessageRelation::Reply { .. } => Capability::Reply,
                        MessageRelation::Thread { .. } => Capability::Thread,
                    }]);
                }
                requirements
            }
            Self::EditMessage { .. } => {
                vec![vec![Capability::EditOwnMessage, Capability::EditAnyMessage]]
            }
            Self::RedactMessage { .. } => {
                vec![vec![
                    Capability::RedactOwnMessage,
                    Capability::RedactAnyMessage,
                ]]
            }
            Self::AddReaction { .. } | Self::RemoveReaction { .. } => {
                vec![vec![Capability::React]]
            }
            Self::MarkRead { .. } => vec![vec![Capability::MarkRead]],
            Self::SetTyping { .. } => vec![vec![Capability::Typing]],
            Self::JoinConversation { .. } => vec![vec![Capability::JoinConversation]],
            Self::LeaveConversation { .. } => vec![vec![Capability::LeaveConversation]],
            Self::UploadMedia { .. } => vec![vec![Capability::UploadAttachment]],
            Self::CreateSpace { .. } => vec![vec![Capability::CreateSpace]],
            Self::AddSpaceChild { .. } => vec![vec![Capability::ManageSpace]],
            Self::RemoveSpaceChild { .. } => vec![vec![Capability::ManageSpace]],
            Self::GetPushRules { .. } => vec![vec![Capability::PushRules]],
            Self::SetPushRule { .. } => vec![vec![Capability::PushRules]],
            Self::DeletePushRule { .. } => vec![vec![Capability::PushRules]],
            Self::AddPusher { .. } => vec![vec![Capability::PushNotifications]],
            Self::RemovePusher { .. } => vec![vec![Capability::PushNotifications]],
            Self::SetPusher { .. } => vec![vec![Capability::PushNotifications]],
            Self::SearchEvents { .. } => vec![vec![Capability::Search]],
            Self::VoipInvite { .. } => vec![vec![Capability::VoipCalls, Capability::VoipSignaling]],
            Self::VoipCandidates { .. } => vec![vec![Capability::VoipSignaling]],
            Self::VoipAnswer { .. } => vec![vec![Capability::VoipCalls, Capability::VoipSignaling]],
            Self::VoipHangup { .. } => vec![vec![Capability::VoipCalls, Capability::VoipSignaling]],
            Self::VoipReject { .. } => vec![vec![Capability::VoipCalls, Capability::VoipSignaling]],
            Self::VoipSelectAnswer { .. } => vec![vec![Capability::VoipSignaling]],
            Self::VoipNegotiate { .. } => vec![vec![Capability::VoipCalls, Capability::VoipSignaling]],
        }
    }

    pub fn conversation_id(&self) -> &EntityRef {
        match self {
            Self::SendMessage {
                conversation_id, ..
            }
            | Self::EditMessage {
                conversation_id, ..
            }
            | Self::RedactMessage {
                conversation_id, ..
            }
            | Self::AddReaction {
                conversation_id, ..
            }
            | Self::RemoveReaction {
                conversation_id, ..
            }
            | Self::MarkRead {
                conversation_id, ..
            }
            | Self::SetTyping {
                conversation_id, ..
            }
            | Self::JoinConversation { conversation_id }
            | Self::LeaveConversation { conversation_id }
            | Self::UploadMedia { conversation_id, .. }
            | Self::CreateSpace { parent_space_id: Some(conversation_id), .. }
            | Self::AddSpaceChild { space_id: conversation_id, .. }
            | Self::RemoveSpaceChild { space_id: conversation_id, .. } => conversation_id,
            Self::VoipInvite { conversation_id, .. }
            | Self::VoipCandidates { conversation_id, .. }
            | Self::VoipAnswer { conversation_id, .. }
            | Self::VoipHangup { conversation_id, .. }
            | Self::VoipReject { conversation_id, .. }
            | Self::VoipSelectAnswer { conversation_id, .. }
            | Self::VoipNegotiate { conversation_id, .. } => conversation_id,
            Self::CreateSpace { parent_space_id: None, .. } => {
                // For root spaces, we need a dummy conversation_id - validation will catch this
                // This is a limitation of the current API design
                static DUMMY: std::sync::OnceLock<EntityRef> = std::sync::OnceLock::new();
                DUMMY.get_or_init(|| EntityRef::new(
                    AccountRef {
                        provider_id: ProviderId::new("").unwrap(),
                        account_id: OpaqueId::new("").unwrap(),
                    },
                    EntityKind::Conversation,
                    OpaqueId::new("").unwrap(),
                ))
            }
            // Push actions don't have a specific conversation_id - they're account-level
            Self::GetPushRules
            | Self::SetPushRule { .. }
            | Self::DeletePushRule { .. }
            | Self::AddPusher { .. }
            | Self::RemovePusher { .. }
            | Self::SetPusher { .. }
            | Self::SearchEvents { .. } => {
                static DUMMY: std::sync::OnceLock<EntityRef> = std::sync::OnceLock::new();
                DUMMY.get_or_init(|| EntityRef::new(
                    AccountRef {
                        provider_id: ProviderId::new("").unwrap(),
                        account_id: OpaqueId::new("").unwrap(),
                    },
                    EntityKind::Conversation,
                    OpaqueId::new("").unwrap(),
                ))
            }
        }
    }

    pub fn validate(&self, account: &AccountRef) -> Result<(), ActionValidationError> {
        let conversation = self.conversation_id();
        if &conversation.account != account {
            return Err(ActionValidationError::AccountMismatch);
        }
        if !conversation.has_kind(&EntityKind::Conversation) {
            return Err(ActionValidationError::WrongEntityKind("conversation_id"));
        }
        for entity in related_entities(self) {
            if &entity.account != account {
                return Err(ActionValidationError::AccountMismatch);
            }
        }
        match self {
            Self::SendMessage {
                body,
                attachments,
                relation,
                ..
            } => {
                if body.formatted.is_some() && body.plain.is_empty() {
                    return Err(ActionValidationError::InvalidBody);
                }
                for attachment in attachments {
                    if !attachment.has_kind(&EntityKind::Media) {
                        return Err(ActionValidationError::WrongEntityKind("attachment"));
                    }
                }
                if let Some(relation) = relation {
                    let related = match relation {
                        MessageRelation::Reply { message_id }
                        | MessageRelation::Thread {
                            root_message_id: message_id,
                        } => message_id,
                    };
                    if !related.has_kind(&EntityKind::Message) {
                        return Err(ActionValidationError::WrongEntityKind("relation"));
                    }
                }
            }
            Self::EditMessage {
                message_id, body, ..
            } => {
                validate_message_id(message_id)?;
                if body.formatted.is_some() && body.plain.is_empty() {
                    return Err(ActionValidationError::InvalidBody);
                }
            }
            Self::RedactMessage { message_id, .. }
            | Self::AddReaction { message_id, .. }
            | Self::RemoveReaction { message_id, .. } => validate_message_id(message_id)?,
            Self::MarkRead { event_id, .. } => {
                if !event_id.has_kind(&EntityKind::Event) {
                    return Err(ActionValidationError::WrongEntityKind("event_id"));
                }
            }
            Self::SetTyping { .. }
            | Self::JoinConversation { .. }
            | Self::LeaveConversation { .. }
            | Self::UploadMedia { .. }
            | Self::CreateSpace { .. }
            | Self::AddSpaceChild { .. }
            | Self::RemoveSpaceChild { .. }
            | Self::GetPushRules { .. }
            | Self::SetPushRule { .. }
            | Self::DeletePushRule { .. }
            | Self::AddPusher { .. }
            | Self::RemovePusher { .. }
            | Self::SetPusher { .. }
            | Self::SearchEvents { .. }
            | Self::VoipInvite { .. }
            | Self::VoipCandidates { .. }
            | Self::VoipAnswer { .. }
            | Self::VoipHangup { .. }
            | Self::VoipReject { .. }
            | Self::VoipSelectAnswer { .. }
            | Self::VoipNegotiate { .. } => {}
        }
        match self {
            Self::AddReaction { key, .. } | Self::RemoveReaction { key, .. }
                if key.trim().is_empty() =>
            {
                return Err(ActionValidationError::EmptyReaction);
            }
            Self::CreateSpace { name, .. } if name.trim().is_empty() => {
                return Err(ActionValidationError::InvalidBody);
            }
            Self::AddSpaceChild { order: Some(order), .. } if order.as_str().trim().is_empty() => {
                return Err(ActionValidationError::WrongEntityKind("order"));
            }
            Self::SetPushRule { rule, .. } => {
                if rule.rule_id.trim().is_empty() {
                    return Err(ActionValidationError::WrongEntityKind("rule_id"));
                }
            }
            Self::AddPusher { pusher } | Self::SetPusher { pusher, .. } => {
                if pusher.pushkey.trim().is_empty() {
                    return Err(ActionValidationError::WrongEntityKind("pushkey"));
                }
            }
            Self::VoipInvite { call_id, .. }
            | Self::VoipCandidates { call_id, .. }
            | Self::VoipAnswer { call_id, .. }
            | Self::VoipHangup { call_id, .. }
            | Self::VoipReject { call_id, .. }
            | Self::VoipSelectAnswer { call_id, .. }
            | Self::VoipNegotiate { call_id, .. } if call_id.trim().is_empty() => {
                return Err(ActionValidationError::WrongEntityKind("call_id"));
            }
            _ => {}
        }
        Ok(())
    }
}

fn related_entities(action: &ProviderAction) -> Vec<&EntityRef> {
    match action {
        ProviderAction::SendMessage {
            attachments,
            relation,
            ..
        } => {
            let mut entities: Vec<_> = attachments.iter().collect();
            if let Some(relation) = relation {
                entities.push(match relation {
                    MessageRelation::Reply { message_id }
                    | MessageRelation::Thread {
                        root_message_id: message_id,
                    } => message_id,
                });
            }
            entities
        }
        ProviderAction::EditMessage { message_id, .. }
        | ProviderAction::RedactMessage { message_id, .. }
        | ProviderAction::AddReaction { message_id, .. }
        | ProviderAction::RemoveReaction { message_id, .. } => vec![message_id],
        ProviderAction::MarkRead { event_id, .. } => vec![event_id],
        ProviderAction::CreateSpace { avatar, parent_space_id, initial_children, .. } => {
            let mut entities = Vec::new();
            if let Some(avatar) = avatar {
                entities.push(avatar);
            }
            if let Some(parent) = parent_space_id {
                entities.push(parent);
            }
            entities.extend(initial_children.iter());
            entities
        }
        ProviderAction::AddSpaceChild { child_id, .. }
        | ProviderAction::RemoveSpaceChild { child_id, .. } => vec![child_id],
        ProviderAction::GetPushRules { .. }
        | ProviderAction::SetPushRule { .. }
        | ProviderAction::DeletePushRule { .. }
        | ProviderAction::AddPusher { .. }
        | ProviderAction::RemovePusher { .. }
        | ProviderAction::SetPusher { .. }
        | ProviderAction::SetTyping { .. }
        | ProviderAction::JoinConversation { .. }
        | ProviderAction::LeaveConversation { .. }
        | ProviderAction::UploadMedia { .. }
        | ProviderAction::SearchEvents { .. }
        | ProviderAction::VoipInvite { .. }
        | ProviderAction::VoipCandidates { .. }
        | ProviderAction::VoipAnswer { .. }
        | ProviderAction::VoipHangup { .. }
        | ProviderAction::VoipReject { .. }
        | ProviderAction::VoipSelectAnswer { .. }
        | ProviderAction::VoipNegotiate { .. } => Vec::new(),
    }
}

fn validate_message_id(entity: &EntityRef) -> Result<(), ActionValidationError> {
    if !entity.has_kind(&EntityKind::Message) {
        return Err(ActionValidationError::WrongEntityKind("message_id"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ActionValidationError {
    #[error("action reference belongs to another account")]
    AccountMismatch,
    #[error("action reference has the wrong entity kind: {0}")]
    WrongEntityKind(&'static str),
    #[error("formatted message must include plain text")]
    InvalidBody,
    #[error("reaction key must not be empty")]
    EmptyReaction,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub account: AccountRef,
    pub operation_id: OpaqueId,
    pub action: ProviderAction,
}

impl ActionRequest {
    pub fn validate(&self) -> Result<(), ActionValidationError> {
        self.action.validate(&self.account)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ActionOutcome {
    Accepted { event_id: Option<EntityRef> },
    NoChange,
}
