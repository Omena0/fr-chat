use fr_chat_domain::{Capability, EntityRef, ProviderId};
use fr_chat_provider_api::{ProviderError, ProviderErrorKind, SyncBatchError};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum StoreError {
    #[error("requested record was not found")]
    NotFound,
    #[error("operation ID was reused with different content")]
    OperationConflict,
    #[error("stored data is invalid")]
    InvalidData,
    #[error("storage is temporarily unavailable")]
    Unavailable,
    #[error("storage migration failed")]
    Migration,
    #[error("storage operation failed")]
    Internal,
}

#[derive(Debug, Error)]
pub enum AppCoreError {
    #[error("provider is not registered: {0}")]
    UnknownProvider(ProviderId),
    #[error("provider ID does not match its registration")]
    ProviderIdentityMismatch,
    #[error("provider API is incompatible with this app")]
    IncompatibleProvider,
    #[error("provider is already registered")]
    DuplicateProvider,
    #[error("account does not belong to the selected provider")]
    AccountProviderMismatch,
    #[error("account or conversation was not found")]
    NotFound,
    #[error("action capability is unavailable: {0:?}")]
    CapabilityUnavailable(Capability),
    #[error("action capability is denied: {0:?}")]
    CapabilityDenied(Capability),
    #[error("action capability is temporarily unavailable: {0:?}")]
    CapabilityTemporarilyUnavailable(Capability),
    #[error("provider returned invalid data: {0}")]
    InvalidProviderData(String),
    #[error("provider operation failed: {0}")]
    Provider(#[from] ProviderError),
    #[error("local account data was removed, but remote revocation needs attention: {0}")]
    RemoteRevocationPending(ProviderError),
    #[error("storage operation failed: {0}")]
    Store(#[from] StoreError),
}

impl From<SyncBatchError> for AppCoreError {
    fn from(value: SyncBatchError) -> Self {
        Self::InvalidProviderData(value.to_string())
    }
}

pub(crate) fn action_error_is_retryable(error: &ProviderError) -> bool {
    matches!(
        error.kind,
        ProviderErrorKind::Offline
            | ProviderErrorKind::RateLimited
            | ProviderErrorKind::CredentialUnavailable
            | ProviderErrorKind::Internal
    )
}

pub(crate) fn validate_entity_scope(
    entity: &EntityRef,
    account: &fr_chat_domain::AccountRef,
) -> Result<(), AppCoreError> {
    if &entity.account != account {
        return Err(AppCoreError::InvalidProviderData(
            "provider returned an entity from a different account".to_owned(),
        ));
    }
    Ok(())
}
