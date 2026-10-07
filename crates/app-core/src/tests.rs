use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use fr_chat_domain::{
    Account, AccountRef, Capability, CapabilityMap, CapabilityState, ConnectionState, Conversation,
    ConversationKind, EntityKind, EntityRef, MessageBody, OpaqueId, ProviderId, SyncState,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, AuthPrompt, AuthResponse, AuthStartRequest, AuthStep,
    CredentialHandle, CredentialStore, Provider, ProviderChange, ProviderContext,
    ProviderDescriptor, ProviderError, ProviderErrorKind, SearchHit, SearchRequest, SecretBytes,
    SecureInputHandle, SecureInputVault, SyncBatch, SyncCursor,
};

use crate::{
    AppCore, AppCoreError, AppEvent, AppEventSink, AppStore, OutboxEntry, OutboxFailure,
    OutboxState, ProviderContextFactory, StoreError, SyncCommit,
};

fn account_ref() -> AccountRef {
    AccountRef::new(
        ProviderId::new("matrix").unwrap(),
        OpaqueId::new("account-1").unwrap(),
    )
}

fn entity(account: &AccountRef, kind: EntityKind, id: &str) -> EntityRef {
    EntityRef::new(account.clone(), kind, OpaqueId::new(id).unwrap())
}

fn account_model(account: AccountRef) -> Account {
    Account {
        id: account.clone(),
        user_id: Some(entity(&account, EntityKind::User, "self")),
        display_name: "Test user".to_owned(),
        avatar: None,
        sync: SyncState {
            connection: ConnectionState::Connected,
            last_successful_sync_ms: None,
            pending_operations: 0,
        },
        credential_refs: Default::default(),
    }
}

fn conversation(account: &AccountRef) -> Conversation {
    Conversation {
        id: entity(account, EntityKind::Conversation, "room-1"),
        server_id: None,
        parent_id: None,
        kind: ConversationKind::Text,
        name: "general".to_owned(),
        topic: None,
        origin: None,
        capabilities: CapabilityMap::default(),
        position: None,
        unread: Default::default(),
    }
}

struct TestStore {
    account: Mutex<Option<Account>>,
    conversation: Conversation,
    capabilities: Mutex<CapabilityMap>,
    cursor: Mutex<Option<SyncCursor>>,
    outbox: Mutex<HashMap<OpaqueId, OutboxEntry>>,
    commits: AtomicUsize,
}

impl TestStore {
    fn new(account: AccountRef, capabilities: CapabilityMap) -> Self {
        Self {
            account: Mutex::new(Some(account_model(account.clone()))),
            conversation: conversation(&account),
            capabilities: Mutex::new(capabilities),
            cursor: Mutex::new(None),
            outbox: Mutex::new(HashMap::new()),
            commits: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl AppStore for TestStore {
    async fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        Ok(self.account.lock().unwrap().clone().into_iter().collect())
    }

    async fn account(&self, account: &AccountRef) -> Result<Option<Account>, StoreError> {
        Ok(self
            .account
            .lock()
            .unwrap()
            .clone()
            .filter(|record| &record.id == account))
    }

    async fn put_account(&self, account: Account) -> Result<(), StoreError> {
        *self.account.lock().unwrap() = Some(account);
        Ok(())
    }

    async fn remove_account_and_data(&self, account: &AccountRef) -> Result<(), StoreError> {
        let mut stored = self.account.lock().unwrap();
        if stored.as_ref().is_none_or(|record| &record.id != account) {
            return Err(StoreError::NotFound);
        }
        *stored = None;
        Ok(())
    }

    async fn servers(
        &self,
        _account: &AccountRef,
    ) -> Result<Vec<fr_chat_domain::Server>, StoreError> {
        Ok(Vec::new())
    }

    async fn conversations(
        &self,
        _account: &AccountRef,
        _server: Option<&EntityRef>,
    ) -> Result<Vec<Conversation>, StoreError> {
        Ok(vec![self.conversation.clone()])
    }

    async fn timeline(
        &self,
        _account: &AccountRef,
        _conversation: &EntityRef,
        _before: Option<&EntityRef>,
        _limit: u16,
    ) -> Result<Vec<fr_chat_domain::TimelineEvent>, StoreError> {
        Ok(Vec::new())
    }

    async fn message(
        &self,
        _account: &AccountRef,
        _id: &EntityRef,
    ) -> Result<Option<fr_chat_domain::Message>, StoreError> {
        Ok(None)
    }

    async fn capabilities(
        &self,
        _account: &AccountRef,
        _conversation: &EntityRef,
    ) -> Result<CapabilityMap, StoreError> {
        Ok(self.capabilities.lock().unwrap().clone())
    }

    async fn sync_cursor(&self, _account: &AccountRef) -> Result<Option<SyncCursor>, StoreError> {
        Ok(self.cursor.lock().unwrap().clone())
    }

    async fn commit_sync_batch(&self, batch: SyncBatch) -> Result<SyncCommit, StoreError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        *self.cursor.lock().unwrap() = batch.next_cursor;
        Ok(SyncCommit {
            applied_change_count: batch.changes.len() as u64,
            new_events: 0,
            updated_conversations: Vec::new(),
        })
    }

    async fn enqueue_action(&self, request: ActionRequest) -> Result<OutboxEntry, StoreError> {
        let mut outbox = self.outbox.lock().unwrap();
        if let Some(existing) = outbox.get(&request.operation_id) {
            if existing.request != request {
                return Err(StoreError::OperationConflict);
            }
            return Ok(existing.clone());
        }
        let entry = OutboxEntry {
            request: request.clone(),
            state: OutboxState::Queued,
            outcome: None,
            failure: None,
        };
        outbox.insert(request.operation_id.clone(), entry.clone());
        Ok(entry)
    }

    async fn claim_action(
        &self,
        _account: &AccountRef,
        operation_id: &OpaqueId,
    ) -> Result<bool, StoreError> {
        let mut outbox = self.outbox.lock().unwrap();
        let entry = outbox.get_mut(operation_id).ok_or(StoreError::NotFound)?;
        match &entry.state {
            OutboxState::Queued | OutboxState::Failed { retryable: true } => {
                entry.state = OutboxState::InFlight;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn complete_action(
        &self,
        _account: &AccountRef,
        operation_id: &OpaqueId,
        outcome: ActionOutcome,
    ) -> Result<OutboxEntry, StoreError> {
        let mut outbox = self.outbox.lock().unwrap();
        let entry = outbox.get_mut(operation_id).ok_or(StoreError::NotFound)?;
        entry.state = OutboxState::Accepted;
        entry.outcome = Some(outcome);
        entry.failure = None;
        Ok(entry.clone())
    }

    async fn fail_action(
        &self,
        _account: &AccountRef,
        operation_id: &OpaqueId,
        failure: OutboxFailure,
        retryable: bool,
    ) -> Result<OutboxEntry, StoreError> {
        let mut outbox = self.outbox.lock().unwrap();
        let entry = outbox.get_mut(operation_id).ok_or(StoreError::NotFound)?;
        entry.state = OutboxState::Failed { retryable };
        entry.failure = Some(failure);
        Ok(entry.clone())
    }

    async fn action(
        &self,
        _account: &AccountRef,
        operation_id: &OpaqueId,
    ) -> Result<Option<OutboxEntry>, StoreError> {
        Ok(self.outbox.lock().unwrap().get(operation_id).cloned())
    }

    async fn queued_actions(
        &self,
        _account: &AccountRef,
        limit: u16,
    ) -> Result<Vec<OutboxEntry>, StoreError> {
        Ok(self
            .outbox
            .lock()
            .unwrap()
            .values()
            .filter(|entry| {
                matches!(
                    &entry.state,
                    OutboxState::Queued | OutboxState::Failed { retryable: true }
                )
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn set_capability(
        &self,
        _account: &AccountRef,
        _conversation: &EntityRef,
        capability: Capability,
        state: CapabilityState,
    ) -> Result<(), StoreError> {
        self.capabilities.lock().unwrap().set(capability, state);
        Ok(())
    }
}

struct TestVault;

#[async_trait]
impl SecureInputVault for TestVault {
    async fn take(&self, _handle: &SecureInputHandle) -> Result<SecretBytes, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialUnavailable,
            "Secure input is unavailable.",
        ))
    }
}

#[async_trait]
impl CredentialStore for TestVault {
    async fn store(
        &self,
        _account: &AccountRef,
        _name: &str,
        _secret: SecretBytes,
    ) -> Result<CredentialHandle, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialUnavailable,
            "Credential storage is unavailable.",
        ))
    }

    async fn load(
        &self,
        _account: &AccountRef,
        _handle: &CredentialHandle,
    ) -> Result<SecretBytes, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialUnavailable,
            "Credential storage is unavailable.",
        ))
    }

    async fn delete(
        &self,
        _account: &AccountRef,
        _handle: &CredentialHandle,
    ) -> Result<(), ProviderError> {
        Ok(())
    }
}

struct TestContexts;

impl ProviderContextFactory for TestContexts {
    fn for_provider(&self, _provider: &ProviderId) -> ProviderContext {
        ProviderContext {
            secure_inputs: Arc::new(TestVault),
            credentials: Arc::new(TestVault),
        }
    }

    fn for_account(&self, account: &AccountRef) -> ProviderContext {
        self.for_provider(&account.provider_id)
    }
}

struct TestEvents;

impl AppEventSink for TestEvents {
    fn publish(&self, _event: AppEvent) {}
}

struct TestProvider {
    batch: Mutex<Option<SyncBatch>>,
    action_event: Mutex<Option<EntityRef>>,
    action_calls: AtomicUsize,
    revoke_fails: AtomicBool,
}

impl TestProvider {
    fn new(batch: Option<SyncBatch>) -> Self {
        Self {
            batch: Mutex::new(batch),
            action_event: Mutex::new(None),
            action_calls: AtomicUsize::new(0),
            revoke_fails: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl Provider for TestProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: ProviderId::new("matrix").unwrap(),
            display_name: "Matrix test provider".to_owned(),
            provider_version: "test".to_owned(),
            minimum_api: fr_chat_provider_api::ApiVersion { major: 1, minor: 0 },
            maximum_api: fr_chat_provider_api::ApiVersion { major: 1, minor: 0 },
            identity_model: fr_chat_provider_api::ProviderIdentityModel::UserAccount,
            conversation_kinds: vec![ConversationKind::Text],
            capabilities: vec![Capability::SendMessage, Capability::Search],
        }
    }

    async fn begin_authorization(
        &self,
        _context: &ProviderContext,
        _request: AuthStartRequest,
    ) -> Result<AuthPrompt, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::Unsupported,
            "Authorization is not used by this test.",
        ))
    }

    async fn continue_authorization(
        &self,
        _context: &ProviderContext,
        _response: AuthResponse,
    ) -> Result<AuthStep, ProviderError> {
        Ok(AuthStep::Cancelled)
    }

    async fn revoke_account(
        &self,
        _context: &ProviderContext,
        _account: &AccountRef,
    ) -> Result<(), ProviderError> {
        if self.revoke_fails.load(Ordering::SeqCst) {
            Err(ProviderError::new(
                ProviderErrorKind::Offline,
                "Remote revocation could not be confirmed.",
            ))
        } else {
            Ok(())
        }
    }

    async fn sync(
        &self,
        _context: &ProviderContext,
        _account: &Account,
        _cursor: Option<SyncCursor>,
        _limit: u16,
    ) -> Result<SyncBatch, ProviderError> {
        self.batch.lock().unwrap().clone().ok_or_else(|| {
            ProviderError::new(ProviderErrorKind::Offline, "No sync test batch is set.")
        })
    }

    async fn execute(
        &self,
        _context: &ProviderContext,
        _request: ActionRequest,
    ) -> Result<ActionOutcome, ProviderError> {
        self.action_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ActionOutcome::Accepted {
            event_id: self.action_event.lock().unwrap().clone(),
        })
    }

    async fn search(
        &self,
        _context: &ProviderContext,
        _request: SearchRequest,
    ) -> Result<Vec<SearchHit>, ProviderError> {
        Ok(Vec::new())
    }
}

fn core(store: Arc<TestStore>, provider: Arc<TestProvider>) -> AppCore {
    let app = AppCore::new(store, Arc::new(TestContexts), Arc::new(TestEvents));
    app.register_provider(provider).unwrap();
    app
}

#[tokio::test]
async fn cross_account_sync_batch_is_rejected_before_store_commit() {
    let account = account_ref();
    let other = AccountRef::new(
        ProviderId::new("matrix").unwrap(),
        OpaqueId::new("account-2").unwrap(),
    );
    let store = Arc::new(TestStore::new(account.clone(), CapabilityMap::default()));
    let provider = Arc::new(TestProvider::new(Some(SyncBatch {
        account: other.clone(),
        changes: vec![ProviderChange::EntityRemoved(entity(
            &other,
            EntityKind::Conversation,
            "wrong-room",
        ))],
        next_cursor: Some(SyncCursor::new(vec![9]).unwrap()),
        has_more: false,
    })));

    assert!(matches!(
        core(store.clone(), provider)
            .sync_account(&account, None)
            .await,
        Err(AppCoreError::InvalidProviderData(_))
    ));
    assert_eq!(store.commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn denied_capability_prevents_provider_action_and_outbox_enqueue() {
    let account = account_ref();
    let store = Arc::new(TestStore::new(account.clone(), CapabilityMap::default()));
    let provider = Arc::new(TestProvider::new(None));
    let app = core(store.clone(), provider.clone());
    let request = ActionRequest {
        account: account.clone(),
        operation_id: OpaqueId::new("send-1").unwrap(),
        action: fr_chat_provider_api::ProviderAction::SendMessage {
            conversation_id: entity(&account, EntityKind::Conversation, "room-1"),
            body: MessageBody::plain("hello"),
            attachments: Vec::new(),
            relation: None,
        },
    };

    assert!(matches!(
        app.execute_action(request).await,
        Err(AppCoreError::CapabilityUnavailable(Capability::SendMessage))
    ));
    assert_eq!(provider.action_calls.load(Ordering::SeqCst), 0);
    assert!(store.outbox.lock().unwrap().is_empty());
}

#[tokio::test]
async fn permitted_send_is_completed_once_for_repeated_operation_id() {
    let account = account_ref();
    let mut capabilities = CapabilityMap::default();
    capabilities.set(Capability::SendMessage, CapabilityState::Allowed);
    let store = Arc::new(TestStore::new(account.clone(), capabilities));
    let provider = Arc::new(TestProvider::new(None));
    let app = core(store.clone(), provider.clone());
    let request = ActionRequest {
        account: account.clone(),
        operation_id: OpaqueId::new("send-1").unwrap(),
        action: fr_chat_provider_api::ProviderAction::SendMessage {
            conversation_id: entity(&account, EntityKind::Conversation, "room-1"),
            body: MessageBody::plain("hello"),
            attachments: Vec::new(),
            relation: None,
        },
    };

    let first = app.execute_action(request.clone()).await.unwrap();
    store
        .set_capability(
            &account,
            &entity(&account, EntityKind::Conversation, "room-1"),
            Capability::SendMessage,
            CapabilityState::Denied,
        )
        .await
        .unwrap();
    let second = app.execute_action(request).await.unwrap();
    assert_eq!(first.state, OutboxState::Accepted);
    assert_eq!(second.state, OutboxState::Accepted);
    assert_eq!(provider.action_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn remote_revoke_failure_still_removes_local_account_data() {
    let account = account_ref();
    let store = Arc::new(TestStore::new(account.clone(), CapabilityMap::default()));
    let provider = Arc::new(TestProvider::new(None));
    provider.revoke_fails.store(true, Ordering::SeqCst);
    let app = core(store.clone(), provider);

    assert!(matches!(
        app.revoke_account(&account).await,
        Err(AppCoreError::RemoteRevocationPending(_))
    ));
    assert!(store.account(&account).await.unwrap().is_none());
}

#[tokio::test]
async fn malformed_accepted_event_is_recorded_as_accepted_not_retried() {
    let account = account_ref();
    let other = AccountRef::new(
        ProviderId::new("matrix").unwrap(),
        OpaqueId::new("account-2").unwrap(),
    );
    let mut capabilities = CapabilityMap::default();
    capabilities.set(Capability::SendMessage, CapabilityState::Allowed);
    let store = Arc::new(TestStore::new(account.clone(), capabilities));
    let provider = Arc::new(TestProvider::new(None));
    *provider.action_event.lock().unwrap() = Some(entity(&other, EntityKind::Event, "$wrong"));
    let app = core(store.clone(), provider.clone());
    let request = ActionRequest {
        account: account.clone(),
        operation_id: OpaqueId::new("send-bad-ack").unwrap(),
        action: fr_chat_provider_api::ProviderAction::SendMessage {
            conversation_id: entity(&account, EntityKind::Conversation, "room-1"),
            body: MessageBody::plain("hello"),
            attachments: Vec::new(),
            relation: None,
        },
    };

    assert!(matches!(
        app.execute_action(request.clone()).await,
        Err(AppCoreError::InvalidProviderData(_))
    ));
    let entry = store
        .action(&account, &request.operation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.state, OutboxState::Accepted);
    assert_eq!(provider.action_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn account_scoped_context_rejects_other_account_credentials() {
    let account = account_ref();
    let other = AccountRef::new(
        ProviderId::new("matrix").unwrap(),
        OpaqueId::new("account-2").unwrap(),
    );
    let context = TestContexts
        .for_account(&account)
        .scoped_to_account(account);

    let error = context
        .credentials
        .delete(
            &other,
            &CredentialHandle::new(OpaqueId::new("secret-ref").unwrap()),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::PermissionDenied);
}

#[tokio::test]
async fn provider_scoped_context_rejects_another_providers_secure_input() {
    let provider = ProviderId::new("matrix").unwrap();
    let other_provider = ProviderId::new("other").unwrap();
    let context = TestContexts
        .for_provider(&provider)
        .scoped_to_provider(provider);
    let handle = SecureInputHandle::new(other_provider, OpaqueId::new("input-1").unwrap());

    let error = context.secure_inputs.take(&handle).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::PermissionDenied);
}
