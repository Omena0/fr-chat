use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    Offline,
    AuthorizationRequired,
    PermissionDenied,
    RateLimited,
    Unsupported,
    Conflict,
    InvalidInput,
    RemoteRejected,
    CredentialUnavailable,
    Internal,
}

#[derive(Clone, Debug, Eq, Error, PartialEq, Serialize, Deserialize)]
#[error("{safe_message}")]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub safe_message: String,
    pub retry_after_ms: Option<u64>,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, safe_message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: safe_message.into(),
            retry_after_ms: None,
        }
    }

    pub fn rate_limited(safe_message: impl Into<String>, retry_after_ms: Option<u64>) -> Self {
        Self {
            kind: ProviderErrorKind::RateLimited,
            safe_message: safe_message.into(),
            retry_after_ms,
        }
    }
}
