use async_trait::async_trait;
use fr_chat_domain::{
    Account, AccountRef, Capability, CapabilityMap, Conversation, EntityRef, Message, Server,
    TimelineEvent,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, ProviderErrorKind, SyncBatch, SyncCursor,
};
use serde::{Deserialize, Serialize};

use crate::StoreError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OutboxState {
    Queued,
    InFlight,
    Accepted,
    Failed { retryable: bool },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutboxFailure {
    pub kind: ProviderErrorKind,
    pub safe_message: String,
    pub retry_after_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub request: ActionRequest,
    pub state: OutboxState,
    pub outcome: Option<ActionOutcome>,
    pub failure: Option<OutboxFailure>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SyncCommit {
    pub applied_change_count: u64,
    pub new_events: u64,
    pub updated_conversations: Vec<EntityRef>,
}

#[async_trait]
pub trait AppStore: Send + Sync {
    async fn accounts(&self) -> Result<Vec<Account>, StoreError>;
    async fn account(&self, account: &AccountRef) -> Result<Option<Account>, StoreError>;
    async fn put_account(&self, account: Account) -> Result<(), StoreError>;
    async fn remove_account_and_data(&self, account: &AccountRef) -> Result<(), StoreError>;

    async fn servers(&self, account: &AccountRef) -> Result<Vec<Server>, StoreError>;
    async fn conversations(
        &self,
        account: &AccountRef,
        server: Option<&EntityRef>,
    ) -> Result<Vec<Conversation>, StoreError>;
    async fn timeline(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
        before: Option<&EntityRef>,
        limit: u16,
    ) -> Result<Vec<TimelineEvent>, StoreError>;
    async fn message(
        &self,
        account: &AccountRef,
        id: &EntityRef,
    ) -> Result<Option<Message>, StoreError>;
    async fn capabilities(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
    ) -> Result<CapabilityMap, StoreError>;

    async fn sync_cursor(&self, account: &AccountRef) -> Result<Option<SyncCursor>, StoreError>;

    /// Atomically persists all changes, event deduplication keys, and the next cursor.
    async fn commit_sync_batch(&self, batch: SyncBatch) -> Result<SyncCommit, StoreError>;

    /// Inserts idempotently by (account, operation ID); mismatched reuse is a conflict.
    async fn enqueue_action(&self, request: ActionRequest) -> Result<OutboxEntry, StoreError>;

    /// Atomically claims a queued or retryable operation. Returns false if it is not runnable.
    async fn claim_action(
        &self,
        account: &AccountRef,
        operation_id: &fr_chat_domain::OpaqueId,
    ) -> Result<bool, StoreError>;

    async fn complete_action(
        &self,
        account: &AccountRef,
        operation_id: &fr_chat_domain::OpaqueId,
        outcome: ActionOutcome,
    ) -> Result<OutboxEntry, StoreError>;

    async fn fail_action(
        &self,
        account: &AccountRef,
        operation_id: &fr_chat_domain::OpaqueId,
        failure: OutboxFailure,
        retryable: bool,
    ) -> Result<OutboxEntry, StoreError>;

    async fn action(
        &self,
        account: &AccountRef,
        operation_id: &fr_chat_domain::OpaqueId,
    ) -> Result<Option<OutboxEntry>, StoreError>;

    async fn queued_actions(
        &self,
        account: &AccountRef,
        limit: u16,
    ) -> Result<Vec<OutboxEntry>, StoreError>;

    async fn set_capability(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
        capability: Capability,
        state: fr_chat_domain::CapabilityState,
    ) -> Result<(), StoreError>;
}
