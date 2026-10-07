use async_trait::async_trait;
use fr_chat_domain::{Account, AccountRef, Capability, ConversationKind, EntityRef};
use serde::{Deserialize, Serialize};

use crate::{
    ActionOutcome, ActionRequest, AuthPrompt, AuthResponse, AuthStartRequest, AuthStep,
    ProviderContext, ProviderError, SyncBatch, SyncCursor,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ApiVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderIdentityModel {
    UserAccount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub provider_id: fr_chat_domain::ProviderId,
    pub display_name: String,
    pub provider_version: String,
    pub minimum_api: ApiVersion,
    pub maximum_api: ApiVersion,
    pub identity_model: ProviderIdentityModel,
    pub conversation_kinds: Vec<ConversationKind>,
    pub capabilities: Vec<Capability>,
}

impl ProviderDescriptor {
    pub fn validate(&self, host_api: ApiVersion) -> Result<(), ProviderError> {
        if self.display_name.trim().is_empty() || self.provider_version.trim().is_empty() {
            return Err(ProviderError::new(
                crate::ProviderErrorKind::InvalidInput,
                "Provider metadata is incomplete.",
            ));
        }
        if self.minimum_api > self.maximum_api
            || self.minimum_api.major != self.maximum_api.major
            || host_api.major != self.minimum_api.major
            || host_api < self.minimum_api
            || host_api > self.maximum_api
        {
            return Err(ProviderError::new(
                crate::ProviderErrorKind::Unsupported,
                "This provider is not compatible with this app version.",
            ));
        }
        if self
            .capabilities
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.capabilities.len()
        {
            return Err(ProviderError::new(
                crate::ProviderErrorKind::InvalidInput,
                "Provider capabilities contain duplicates.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchRequest {
    pub account: AccountRef,
    pub query: String,
    pub conversation: Option<EntityRef>,
    pub limit: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub conversation_id: EntityRef,
    pub event_id: EntityRef,
    pub sender_id: Option<EntityRef>,
    pub timestamp_ms: Option<i64>,
    pub snippet: String,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    async fn begin_authorization(
        &self,
        context: &ProviderContext,
        request: AuthStartRequest,
    ) -> Result<AuthPrompt, ProviderError>;

    async fn continue_authorization(
        &self,
        context: &ProviderContext,
        response: AuthResponse,
    ) -> Result<AuthStep, ProviderError>;

    async fn revoke_account(
        &self,
        context: &ProviderContext,
        account: &AccountRef,
    ) -> Result<(), ProviderError>;

    async fn sync(
        &self,
        context: &ProviderContext,
        account: &Account,
        cursor: Option<SyncCursor>,
        limit: u16,
    ) -> Result<SyncBatch, ProviderError>;

    async fn execute(
        &self,
        context: &ProviderContext,
        request: ActionRequest,
    ) -> Result<ActionOutcome, ProviderError>;

    async fn search(
        &self,
        context: &ProviderContext,
        request: SearchRequest,
    ) -> Result<Vec<SearchHit>, ProviderError>;
}
