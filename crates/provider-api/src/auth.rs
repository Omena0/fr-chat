use std::{fmt, sync::Arc};

use async_trait::async_trait;
use fr_chat_domain::{Account, AccountRef, OpaqueId, ProviderId};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::ProviderError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecureInputHandle {
    provider_id: ProviderId,
    id: OpaqueId,
}

impl SecureInputHandle {
    pub fn new(provider_id: ProviderId, id: OpaqueId) -> Self {
        Self { provider_id, id }
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CredentialHandle(OpaqueId);

impl CredentialHandle {
    pub fn new(id: OpaqueId) -> Self {
        Self(id)
    }
}

pub struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    pub fn new(value: Vec<u8>) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretBytes([REDACTED])")
    }
}

#[async_trait]
pub trait SecureInputVault: Send + Sync {
    async fn take(&self, handle: &SecureInputHandle) -> Result<SecretBytes, ProviderError>;
}

#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn store(
        &self,
        account: &AccountRef,
        name: &str,
        secret: SecretBytes,
    ) -> Result<CredentialHandle, ProviderError>;

    async fn load(
        &self,
        account: &AccountRef,
        handle: &CredentialHandle,
    ) -> Result<SecretBytes, ProviderError>;

    async fn delete(
        &self,
        account: &AccountRef,
        handle: &CredentialHandle,
    ) -> Result<(), ProviderError>;
}

#[derive(Clone)]
pub struct ProviderContext {
    pub secure_inputs: Arc<dyn SecureInputVault>,
    pub credentials: Arc<dyn CredentialStore>,
}

impl ProviderContext {
    pub fn scoped_to_provider(&self, provider_id: ProviderId) -> Self {
        Self {
            secure_inputs: Arc::new(ScopedSecureInputVault {
                inner: self.secure_inputs.clone(),
                provider_id: provider_id.clone(),
            }),
            credentials: Arc::new(ScopedCredentialStore {
                inner: self.credentials.clone(),
                scope: CredentialScope::Provider(provider_id),
            }),
        }
    }

    pub fn scoped_to_account(&self, account: AccountRef) -> Self {
        Self {
            secure_inputs: Arc::new(ScopedSecureInputVault {
                inner: self.secure_inputs.clone(),
                provider_id: account.provider_id.clone(),
            }),
            credentials: Arc::new(ScopedCredentialStore {
                inner: self.credentials.clone(),
                scope: CredentialScope::Account(account),
            }),
        }
    }
}

struct ScopedSecureInputVault {
    inner: Arc<dyn SecureInputVault>,
    provider_id: ProviderId,
}

#[async_trait]
impl SecureInputVault for ScopedSecureInputVault {
    async fn take(&self, handle: &SecureInputHandle) -> Result<SecretBytes, ProviderError> {
        if handle.provider_id() != &self.provider_id {
            return Err(ProviderError::new(
                crate::ProviderErrorKind::PermissionDenied,
                "Secure input is outside this provider scope.",
            ));
        }
        self.inner.take(handle).await
    }
}

#[derive(Clone)]
enum CredentialScope {
    Provider(ProviderId),
    Account(AccountRef),
}

struct ScopedCredentialStore {
    inner: Arc<dyn CredentialStore>,
    scope: CredentialScope,
}

impl ScopedCredentialStore {
    fn check_scope(&self, account: &AccountRef) -> Result<(), ProviderError> {
        let allowed = match &self.scope {
            CredentialScope::Provider(provider_id) => &account.provider_id == provider_id,
            CredentialScope::Account(expected) => account == expected,
        };
        if allowed {
            Ok(())
        } else {
            Err(ProviderError::new(
                crate::ProviderErrorKind::PermissionDenied,
                "Credential access is outside this provider/account scope.",
            ))
        }
    }
}

#[async_trait]
impl CredentialStore for ScopedCredentialStore {
    async fn store(
        &self,
        account: &AccountRef,
        name: &str,
        secret: SecretBytes,
    ) -> Result<CredentialHandle, ProviderError> {
        self.check_scope(account)?;
        self.inner.store(account, name, secret).await
    }

    async fn load(
        &self,
        account: &AccountRef,
        handle: &CredentialHandle,
    ) -> Result<SecretBytes, ProviderError> {
        self.check_scope(account)?;
        self.inner.load(account, handle).await
    }

    async fn delete(
        &self,
        account: &AccountRef,
        handle: &CredentialHandle,
    ) -> Result<(), ProviderError> {
        self.check_scope(account)?;
        self.inner.delete(account, handle).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthFieldKind {
    PublicText,
    Secret,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthField {
    pub id: String,
    pub label: String,
    pub kind: AuthFieldKind,
    pub required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum AuthStepKind {
    OpenExternalUrl {
        url: String,
        callback_uri: String,
    },
    EnterUserCode {
        verification_url: String,
        user_code: String,
        expires_at_ms: Option<i64>,
    },
    CollectFields {
        fields: Vec<AuthField>,
    },
    WaitForCallback,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthPrompt {
    pub flow_id: OpaqueId,
    pub title: String,
    pub instructions: Option<String>,
    pub step: AuthStepKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "data")]
pub enum AuthorizationValue {
    PublicText(String),
    SecureInput(SecureInputHandle),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthResponse {
    pub flow_id: OpaqueId,
    pub values: std::collections::BTreeMap<String, AuthorizationValue>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthStartRequest {
    pub values: std::collections::BTreeMap<String, AuthorizationValue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "data")]
pub enum AuthStep {
    Continue(AuthPrompt),
    Authorized(Account),
    Cancelled,
}
