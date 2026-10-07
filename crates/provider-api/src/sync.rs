use fr_chat_domain::{
    Account, AccountRef, Conversation, EntityKind, EntityRef, Member, Role, Server, TimelineEvent,
    User,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SyncBatchError {
    #[error("change {index} belongs to a different account")]
    AccountMismatch { index: usize },
    #[error("change {index} is invalid: {message}")]
    InvalidChange { index: usize, message: String },
    #[error("sync batch has more pages but no next cursor")]
    MissingCursor,
    #[error("sync cursor exceeds the maximum supported size")]
    CursorTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncCursor(Vec<u8>);

impl SyncCursor {
    pub const MAX_BYTES: usize = 64 * 1024;

    pub fn new(value: Vec<u8>) -> Result<Self, SyncBatchError> {
        if value.len() > Self::MAX_BYTES {
            return Err(SyncBatchError::CursorTooLarge);
        }
        Ok(Self(value))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum ProviderChange {
    AccountUpdated(Account),
    ServerUpserted(Server),
    ConversationUpserted(Conversation),
    UserUpserted(User),
    RoleUpserted(Role),
    MemberUpserted(Member),
    TimelineEvent(TimelineEvent),
    EntityRemoved(EntityRef),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncBatch {
    pub account: AccountRef,
    pub changes: Vec<ProviderChange>,
    pub next_cursor: Option<SyncCursor>,
    pub has_more: bool,
}

impl SyncBatch {
    pub fn validate(&self) -> Result<(), SyncBatchError> {
        if self.has_more && self.next_cursor.is_none() {
            return Err(SyncBatchError::MissingCursor);
        }
        for (index, change) in self.changes.iter().enumerate() {
            let result = validate_change(change, &self.account);
            if let Err(error) = result {
                return Err(match error {
                    ChangeValidationError::AccountMismatch => {
                        SyncBatchError::AccountMismatch { index }
                    }
                    ChangeValidationError::Invalid(message) => {
                        SyncBatchError::InvalidChange { index, message }
                    }
                });
            }
        }
        Ok(())
    }
}

enum ChangeValidationError {
    AccountMismatch,
    Invalid(String),
}

fn validate_change(
    change: &ProviderChange,
    account: &AccountRef,
) -> Result<(), ChangeValidationError> {
    let validate_entity = |entity: &EntityRef, expected: EntityKind| {
        if &entity.account != account {
            return Err(ChangeValidationError::AccountMismatch);
        }
        if !entity.has_kind(&expected) {
            return Err(ChangeValidationError::Invalid(format!(
                "expected {expected:?} entity"
            )));
        }
        Ok(())
    };

    let model_result = match change {
        ProviderChange::AccountUpdated(value) => {
            if &value.id != account {
                return Err(ChangeValidationError::AccountMismatch);
            }
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::ServerUpserted(value) => {
            validate_entity(&value.id, EntityKind::Server)?;
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::ConversationUpserted(value) => {
            validate_entity(&value.id, EntityKind::Conversation)?;
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::UserUpserted(value) => {
            validate_entity(&value.id, EntityKind::User)?;
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::RoleUpserted(value) => {
            validate_entity(&value.id, EntityKind::Role)?;
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::MemberUpserted(value) => {
            validate_entity(&value.user_id, EntityKind::User)?;
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::TimelineEvent(value) => {
            validate_entity(&value.id, EntityKind::Event)?;
            value.validate().map_err(|error| error.to_string())
        }
        ProviderChange::EntityRemoved(value) => {
            if &value.account != account {
                return Err(ChangeValidationError::AccountMismatch);
            }
            return Ok(());
        }
    };
    model_result.map_err(ChangeValidationError::Invalid)
}
