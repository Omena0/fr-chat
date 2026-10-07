use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use fr_chat_domain::{
    Account, AccountRef, Capability, CapabilityState, Conversation, EntityKind, EntityRef,
    ProviderId, Server, TimelineEvent,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, AuthPrompt, AuthResponse, AuthStartRequest, AuthStep, Provider,
    ProviderContext, ProviderDescriptor, SearchHit, SearchRequest, SyncCursor,
};

use crate::{
    AppCoreError, AppStore, OutboxEntry, OutboxFailure, OutboxState, StoreError, SyncCommit,
    error::{action_error_is_retryable, validate_entity_scope},
};

const HOST_API: fr_chat_provider_api::ApiVersion =
    fr_chat_provider_api::ApiVersion { major: 1, minor: 0 };
const DEFAULT_SYNC_LIMIT: u16 = 500;
const MAX_QUERY_LIMIT: u16 = 100;

pub trait ProviderContextFactory: Send + Sync {
    fn for_provider(&self, provider: &ProviderId) -> ProviderContext;
    fn for_account(&self, account: &AccountRef) -> ProviderContext;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppEventType {
    AccountChanged,
    SyncCommitted {
        change_count: u64,
        event_count: u64,
        has_more: bool,
    },
    OutboxChanged {
        operation_id: fr_chat_domain::OpaqueId,
        state: OutboxState,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppEvent {
    pub account: AccountRef,
    pub event: AppEventType,
}

pub trait AppEventSink: Send + Sync {
    /// Implementations should enqueue quickly; durable changes can be reloaded from AppStore.
    fn publish(&self, event: AppEvent);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncResult {
    pub commit: SyncCommit,
    pub next_cursor: Option<SyncCursor>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchScope {
    All,
    Conversation(EntityRef),
}

pub struct AppCore {
    store: Arc<dyn AppStore>,
    contexts: Arc<dyn ProviderContextFactory>,
    events: Arc<dyn AppEventSink>,
    providers: RwLock<HashMap<ProviderId, Arc<dyn Provider>>>,
}

impl AppCore {
    pub fn new(
        store: Arc<dyn AppStore>,
        contexts: Arc<dyn ProviderContextFactory>,
        events: Arc<dyn AppEventSink>,
    ) -> Self {
        Self {
            store,
            contexts,
            events,
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_provider(&self, provider: Arc<dyn Provider>) -> Result<(), AppCoreError> {
        let descriptor = provider.descriptor();
        descriptor
            .validate(HOST_API)
            .map_err(|_| AppCoreError::IncompatibleProvider)?;
        let mut providers = self
            .providers
            .write()
            .map_err(|_| AppCoreError::IncompatibleProvider)?;
        if providers.contains_key(&descriptor.provider_id) {
            return Err(AppCoreError::DuplicateProvider);
        }
        providers.insert(descriptor.provider_id.clone(), provider);
        Ok(())
    }

    pub fn providers(&self) -> Result<Vec<ProviderDescriptor>, AppCoreError> {
        let providers = self
            .providers
            .read()
            .map_err(|_| AppCoreError::IncompatibleProvider)?;
        Ok(providers
            .values()
            .map(|provider| provider.descriptor())
            .collect())
    }

    pub async fn accounts(&self) -> Result<Vec<Account>, AppCoreError> {
        Ok(self.store.accounts().await?)
    }

    pub async fn servers(&self, account: &AccountRef) -> Result<Vec<Server>, AppCoreError> {
        self.require_account(account).await?;
        Ok(self.store.servers(account).await?)
    }

    pub async fn conversations(
        &self,
        account: &AccountRef,
        server: Option<&EntityRef>,
    ) -> Result<Vec<Conversation>, AppCoreError> {
        self.require_account(account).await?;
        if let Some(server) = server {
            if !server.has_kind(&EntityKind::Server) || &server.account != account {
                return Err(AppCoreError::AccountProviderMismatch);
            }
        }
        Ok(self.store.conversations(account, server).await?)
    }

    pub async fn timeline(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
        before: Option<&EntityRef>,
        limit: u16,
    ) -> Result<Vec<TimelineEvent>, AppCoreError> {
        self.require_conversation(account, conversation).await?;
        if !(1..=MAX_QUERY_LIMIT).contains(&limit) {
            return Err(AppCoreError::InvalidProviderData(
                "timeline limit is outside the supported range".to_owned(),
            ));
        }
        if let Some(before) = before {
            if !before.has_kind(&EntityKind::Event) || &before.account != account {
                return Err(AppCoreError::AccountProviderMismatch);
            }
        }
        Ok(self
            .store
            .timeline(account, conversation, before, limit)
            .await?)
    }

    pub async fn begin_authorization(
        &self,
        provider_id: &ProviderId,
        request: AuthStartRequest,
    ) -> Result<AuthPrompt, AppCoreError> {
        let provider = self.provider(provider_id)?;
        Ok(provider
            .begin_authorization(
                &self
                    .contexts
                    .for_provider(provider_id)
                    .scoped_to_provider(provider_id.clone()),
                request,
            )
            .await?)
    }

    pub async fn continue_authorization(
        &self,
        provider_id: &ProviderId,
        response: AuthResponse,
    ) -> Result<AuthStep, AppCoreError> {
        let provider = self.provider(provider_id)?;
        let step = provider
            .continue_authorization(
                &self
                    .contexts
                    .for_provider(provider_id)
                    .scoped_to_provider(provider_id.clone()),
                response,
            )
            .await?;
        if let AuthStep::Authorized(account) = &step {
            account
                .validate()
                .map_err(|error| AppCoreError::InvalidProviderData(error.to_string()))?;
            if &account.id.provider_id != provider_id {
                return Err(AppCoreError::ProviderIdentityMismatch);
            }
            self.store.put_account(account.clone()).await?;
            self.events.publish(AppEvent {
                account: account.id.clone(),
                event: AppEventType::AccountChanged,
            });
        }
        Ok(step)
    }

    pub async fn revoke_account(&self, account: &AccountRef) -> Result<(), AppCoreError> {
        self.require_account(account).await?;
        let provider = self.provider(&account.provider_id)?;
        let remote_revoke = provider
            .revoke_account(
                &self
                    .contexts
                    .for_account(account)
                    .scoped_to_account(account.clone()),
                account,
            )
            .await;
        self.store.remove_account_and_data(account).await?;
        self.events.publish(AppEvent {
            account: account.clone(),
            event: AppEventType::AccountChanged,
        });
        remote_revoke.map_err(AppCoreError::RemoteRevocationPending)
    }

    pub async fn sync_account(
        &self,
        account_ref: &AccountRef,
        limit: Option<u16>,
    ) -> Result<SyncResult, AppCoreError> {
        let account = self.require_account(account_ref).await?;
        let provider = self.provider(&account_ref.provider_id)?;
        let cursor = self.store.sync_cursor(account_ref).await?;
        let batch = provider
            .sync(
                &self
                    .contexts
                    .for_account(account_ref)
                    .scoped_to_account(account_ref.clone()),
                &account,
                cursor,
                limit
                    .unwrap_or(DEFAULT_SYNC_LIMIT)
                    .clamp(1, DEFAULT_SYNC_LIMIT),
            )
            .await?;
        if &batch.account != account_ref {
            return Err(AppCoreError::InvalidProviderData(
                "provider returned a sync batch for another account".to_owned(),
            ));
        }
        batch.validate()?;
        let has_more = batch.has_more;
        let next_cursor = batch.next_cursor.clone();
        let commit = self.store.commit_sync_batch(batch).await?;
        self.events.publish(AppEvent {
            account: account_ref.clone(),
            event: AppEventType::SyncCommitted {
                change_count: commit.applied_change_count,
                event_count: commit.new_events,
                has_more,
            },
        });
        Ok(SyncResult {
            commit,
            next_cursor,
            has_more,
        })
    }

    pub async fn execute_action(
        &self,
        request: ActionRequest,
    ) -> Result<OutboxEntry, AppCoreError> {
        request
            .validate()
            .map_err(|error| AppCoreError::InvalidProviderData(error.to_string()))?;
        let account = self.require_account(&request.account).await?;
        let conversation = request.action.conversation_id();
        self.require_conversation(&request.account, conversation)
            .await?;
        if let Some(existing) = self
            .store
            .action(&request.account, &request.operation_id)
            .await?
        {
            if existing.request != request {
                return Err(AppCoreError::Store(StoreError::OperationConflict));
            }
            if matches!(
                existing.state,
                OutboxState::Accepted | OutboxState::InFlight
            ) {
                return Ok(existing);
            }
        }
        self.authorize_action(&account, &request).await?;

        let queued = self.store.enqueue_action(request.clone()).await?;
        if matches!(queued.state, OutboxState::Accepted | OutboxState::InFlight) {
            return Ok(queued);
        }
        if !self
            .store
            .claim_action(&request.account, &request.operation_id)
            .await?
        {
            return self
                .store
                .action(&request.account, &request.operation_id)
                .await?
                .ok_or(AppCoreError::NotFound);
        }

        let provider = self.provider(&request.account.provider_id)?;
        let result = provider
            .execute(
                &self
                    .contexts
                    .for_account(&request.account)
                    .scoped_to_account(request.account.clone()),
                request.clone(),
            )
            .await;
        match result {
            Ok(outcome) => {
                if let ActionOutcome::Accepted {
                    event_id: Some(event_id),
                } = &outcome
                {
                    if validate_entity_scope(event_id, &request.account).is_err()
                        || !event_id.has_kind(&EntityKind::Event)
                    {
                        let entry = self
                            .store
                            .complete_action(
                                &request.account,
                                &request.operation_id,
                                ActionOutcome::Accepted { event_id: None },
                            )
                            .await?;
                        self.publish_outbox(&request.account, &entry);
                        return Err(AppCoreError::InvalidProviderData(
                            "provider accepted the action but returned an invalid event reference"
                                .to_owned(),
                        ));
                    }
                }
                let entry = self
                    .store
                    .complete_action(&request.account, &request.operation_id, outcome)
                    .await?;
                self.publish_outbox(&request.account, &entry);
                Ok(entry)
            }
            Err(error) => {
                let retryable = action_error_is_retryable(&error);
                let failure = OutboxFailure {
                    kind: error.kind,
                    safe_message: error.safe_message.clone(),
                    retry_after_ms: error.retry_after_ms,
                };
                let entry = self
                    .store
                    .fail_action(&request.account, &request.operation_id, failure, retryable)
                    .await?;
                self.publish_outbox(&request.account, &entry);
                Err(AppCoreError::Provider(error))
            }
        }
    }

    pub async fn retry_queued_actions(
        &self,
        account: &AccountRef,
        limit: u16,
    ) -> Result<Vec<Result<OutboxEntry, AppCoreError>>, AppCoreError> {
        self.require_account(account).await?;
        let queued = self
            .store
            .queued_actions(account, limit.clamp(1, 100))
            .await?;
        let mut results = Vec::with_capacity(queued.len());
        for entry in queued {
            results.push(self.execute_action(entry.request).await);
        }
        Ok(results)
    }

    pub async fn search(
        &self,
        account: &AccountRef,
        scope: SearchScope,
        query: String,
        limit: u16,
    ) -> Result<Vec<SearchHit>, AppCoreError> {
        self.require_account(account).await?;
        if query.trim().is_empty() || !(1..=MAX_QUERY_LIMIT).contains(&limit) {
            return Err(AppCoreError::InvalidProviderData(
                "search query or limit is invalid".to_owned(),
            ));
        }
        let conversation = match scope {
            SearchScope::All => None,
            SearchScope::Conversation(id) => {
                self.require_conversation(account, &id).await?;
                Some(id)
            }
        };
        let capability_scope = conversation
            .as_ref()
            .ok_or(AppCoreError::CapabilityUnavailable(Capability::Search));
        if let Ok(scope) = capability_scope {
            self.require_capability(account, scope, &Capability::Search)
                .await?;
        } else {
            let descriptor = self.provider(&account.provider_id)?.descriptor();
            if !descriptor.capabilities.contains(&Capability::Search) {
                return Err(AppCoreError::CapabilityUnavailable(Capability::Search));
            }
        }
        let provider = self.provider(&account.provider_id)?;
        let hits = provider
            .search(
                &self
                    .contexts
                    .for_account(account)
                    .scoped_to_account(account.clone()),
                SearchRequest {
                    account: account.clone(),
                    query,
                    conversation,
                    limit,
                },
            )
            .await?;
        for hit in &hits {
            validate_hit(account, hit)?;
        }
        Ok(hits)
    }

    async fn authorize_action(
        &self,
        account: &Account,
        request: &ActionRequest,
    ) -> Result<(), AppCoreError> {
        let conversation = request.action.conversation_id();
        let capabilities = self.store.capabilities(&account.id, conversation).await?;
        let mut requirements = request.action.capability_requirements();
        match &request.action {
            fr_chat_provider_api::ProviderAction::EditMessage { message_id, .. }
            | fr_chat_provider_api::ProviderAction::RedactMessage { message_id, .. } => {
                let message = self
                    .store
                    .message(&account.id, message_id)
                    .await?
                    .ok_or(AppCoreError::NotFound)?;
                if message.conversation_id != *conversation {
                    return Err(AppCoreError::NotFound);
                }
                let is_own = account.user_id.as_ref() == Some(&message.sender_id);
                let (own, any) = match &request.action {
                    fr_chat_provider_api::ProviderAction::EditMessage { .. } => {
                        (Capability::EditOwnMessage, Capability::EditAnyMessage)
                    }
                    _ => (Capability::RedactOwnMessage, Capability::RedactAnyMessage),
                };
                requirements = vec![if is_own { vec![own, any] } else { vec![any] }];
            }
            _ => {}
        }
        for alternatives in requirements {
            let states: Vec<_> = alternatives
                .iter()
                .map(|capability| (capability, capabilities.state(capability)))
                .collect();
            if states
                .iter()
                .any(|(_, state)| *state == CapabilityState::Allowed)
            {
                continue;
            }
            let (capability, state) = states
                .iter()
                .find(|(_, state)| *state == CapabilityState::TemporarilyUnavailable)
                .or_else(|| {
                    states
                        .iter()
                        .find(|(_, state)| *state == CapabilityState::Denied)
                })
                .or_else(|| states.first())
                .ok_or(AppCoreError::CapabilityUnavailable(Capability::Other(
                    "unknown".to_owned(),
                )))?;
            return Err(match state {
                CapabilityState::Denied => AppCoreError::CapabilityDenied((*capability).clone()),
                CapabilityState::TemporarilyUnavailable => {
                    AppCoreError::CapabilityTemporarilyUnavailable((*capability).clone())
                }
                CapabilityState::Unsupported => {
                    AppCoreError::CapabilityUnavailable((*capability).clone())
                }
                CapabilityState::Allowed => unreachable!(),
            });
        }
        Ok(())
    }

    async fn require_capability(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
        capability: &Capability,
    ) -> Result<(), AppCoreError> {
        let map = self.store.capabilities(account, conversation).await?;
        match map.state(capability) {
            CapabilityState::Allowed => Ok(()),
            CapabilityState::Denied => Err(AppCoreError::CapabilityDenied(capability.clone())),
            CapabilityState::Unsupported => {
                Err(AppCoreError::CapabilityUnavailable(capability.clone()))
            }
            CapabilityState::TemporarilyUnavailable => Err(
                AppCoreError::CapabilityTemporarilyUnavailable(capability.clone()),
            ),
        }
    }

    async fn require_account(&self, account: &AccountRef) -> Result<Account, AppCoreError> {
        self.provider(&account.provider_id)?;
        self.store
            .account(account)
            .await?
            .ok_or(AppCoreError::NotFound)
    }

    async fn require_conversation(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
    ) -> Result<(), AppCoreError> {
        if &conversation.account != account || !conversation.has_kind(&EntityKind::Conversation) {
            return Err(AppCoreError::AccountProviderMismatch);
        }
        Ok(())
    }

    fn provider(&self, provider_id: &ProviderId) -> Result<Arc<dyn Provider>, AppCoreError> {
        self.providers
            .read()
            .map_err(|_| AppCoreError::IncompatibleProvider)?
            .get(provider_id)
            .cloned()
            .ok_or_else(|| AppCoreError::UnknownProvider(provider_id.clone()))
    }

    fn publish_outbox(&self, account: &AccountRef, entry: &OutboxEntry) {
        self.events.publish(AppEvent {
            account: account.clone(),
            event: AppEventType::OutboxChanged {
                operation_id: entry.request.operation_id.clone(),
                state: entry.state.clone(),
            },
        });
    }
}

fn validate_hit(account: &AccountRef, hit: &SearchHit) -> Result<(), AppCoreError> {
    for (entity, kind) in [
        (&hit.conversation_id, EntityKind::Conversation),
        (&hit.event_id, EntityKind::Event),
    ] {
        validate_entity_scope(entity, account)?;
        if !entity.has_kind(&kind) {
            return Err(AppCoreError::InvalidProviderData(
                "provider returned a search result with an invalid entity kind".to_owned(),
            ));
        }
    }
    if let Some(sender) = &hit.sender_id {
        validate_entity_scope(sender, account)?;
        if !sender.has_kind(&EntityKind::User) {
            return Err(AppCoreError::InvalidProviderData(
                "provider returned a search result with an invalid sender".to_owned(),
            ));
        }
    }
    Ok(())
}
