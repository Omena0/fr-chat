#![forbid(unsafe_code)]

mod actions;
mod auth;
mod error;
mod provider;
mod sync;

pub use actions::{ActionOutcome, ActionRequest, ActionValidationError, ProviderAction};
pub use auth::{
    AuthField, AuthFieldKind, AuthPrompt, AuthResponse, AuthStartRequest, AuthStep, AuthStepKind,
    AuthorizationValue,
    CredentialHandle, CredentialStore, ProviderContext, SecretBytes, SecureInputHandle,
    SecureInputVault,
};
pub use error::{ProviderError, ProviderErrorKind};
pub use provider::{
    ApiVersion, Provider, ProviderDescriptor, ProviderIdentityModel, SearchHit, SearchRequest,
};
pub use sync::{ProviderChange, SyncBatch, SyncBatchError, SyncCursor};

#[cfg(test)]
mod tests;
