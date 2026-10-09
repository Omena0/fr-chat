use tokio::sync::Mutex;
use url::Url;

use fr_chat_domain::{
    Account, AccountRef, Capability, ConversationKind, EntityKind,
    EntityRef, OpaqueId, ProviderId,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, ApiVersion, AuthPrompt, AuthResponse, AuthStartRequest, AuthStep,
    AuthorizationValue, Provider, ProviderContext, ProviderDescriptor, ProviderError, ProviderErrorKind, ProviderIdentityModel,
    SearchHit, SearchRequest, SyncBatch, SyncCursor,
};
use matrix_sdk::{
    Client,
    authentication::oauth::{
        ClientRegistrationData, OAuthError,
        registration::{ApplicationType, ClientMetadata, Localized, OAuthGrantType},
    },
    encryption::EncryptionSettings,
    ruma::serde::Raw,
    utils::local_server::QueryString,
};

pub const PROVIDER_ID: &str = "matrix";
const REDIRECT_URI: &str = "http://localhost:8080/callback";

pub struct MatrixProvider {
    client: Mutex<Option<Client>>,
    homeserver: String,
    store_path: String,
}

impl MatrixProvider {
    pub fn new(homeserver: String, store_path: String) -> Self {
        Self {
            client: Mutex::new(None),
            homeserver,
            store_path,
        }
    }

    pub async fn get_or_create_client(&self) -> Result<Client, ProviderError> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }

        let client = Client::builder()
            .homeserver_url(&self.homeserver)
            .sqlite_store(&self.store_path, None)
            .with_encryption_settings(EncryptionSettings {
                auto_enable_cross_signing: true,
                auto_enable_backups: true,
                backup_download_strategy: matrix_sdk::encryption::BackupDownloadStrategy::AfterDecryptionFailure,
            })
            .build()
            .await
            .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

        *guard = Some(client.clone());
        Ok(client)
    }

    pub fn homeserver(&self) -> &str {
        &self.homeserver
    }

    fn create_client_metadata(&self, redirect_uri: Url) -> ClientRegistrationData {
        let client_uri = Localized::new(
            Url::parse("https://github.com/omena0/fr-chat").expect("Couldn't parse client URI"),
            None,
        );

        let metadata = ClientMetadata {
            client_name: Some(Localized::new("fr-chat".to_owned(), [])),
            policy_uri: Some(client_uri.clone()),
            tos_uri: Some(client_uri.clone()),
            ..ClientMetadata::new(
                ApplicationType::Native,
                vec![OAuthGrantType::AuthorizationCode {
                    redirect_uris: vec![redirect_uri],
                }],
                client_uri,
            )
        };

        ClientRegistrationData::new(Raw::new(&metadata).expect("Couldn't serialize client metadata"))
    }

    fn create_account(&self, user_id: &matrix_sdk::ruma::UserId) -> Result<Account, ProviderError> {
        let provider_id = ProviderId::new(PROVIDER_ID).expect("Invalid provider ID");
        let account_id = OpaqueId::new(user_id.localpart()).expect("valid opaque id");
        let account = Account {
            id: AccountRef {
                provider_id: provider_id.clone(),
                account_id: account_id.clone(),
            },
            user_id: Some(EntityRef::new(
                AccountRef {
                    provider_id: provider_id.clone(),
                    account_id: OpaqueId::new(user_id.to_string()).expect("valid opaque id"),
                },
                EntityKind::User,
                OpaqueId::new(user_id.to_string()).expect("valid opaque id"),
            )),
            display_name: self.get_display_name(user_id),
            avatar: None,
            sync: fr_chat_domain::SyncState {
                connection: fr_chat_domain::ConnectionState::Connected,
                last_successful_sync_ms: None,
                pending_operations: 0,
            },
            credential_refs: std::collections::BTreeMap::new(),
        };
        Ok(account)
    }

    fn get_display_name(&self, user_id: &matrix_sdk::ruma::UserId) -> String {
        // This is called during authorization, client is already set up
        user_id.to_string()
    }
}

#[async_trait::async_trait]
impl Provider for MatrixProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: ProviderId::new(PROVIDER_ID).expect("Invalid provider ID"),
            display_name: "Matrix".to_owned(),
            provider_version: "0.18.0".to_owned(),
            minimum_api: ApiVersion { major: 0, minor: 1 },
            maximum_api: ApiVersion { major: 0, minor: 1 },
            identity_model: ProviderIdentityModel::UserAccount,
            conversation_kinds: vec![
                ConversationKind::DirectMessage,
                ConversationKind::GroupDirectMessage,
                ConversationKind::Text,
                ConversationKind::Category,
            ],
            capabilities: vec![
                Capability::ListConversations,
                Capability::ReadHistory,
                Capability::Search,
                Capability::SendMessage,
                Capability::EditOwnMessage,
                Capability::RedactOwnMessage,
                Capability::React,
                Capability::JoinConversation,
                Capability::LeaveConversation,
                Capability::Reply,
                Capability::Thread,
                Capability::UploadAttachment,
                Capability::MarkRead,
                Capability::Typing,
                Capability::Encrypt,
                Capability::Decrypt,
                Capability::VerifyIdentity,
                Capability::CreateSpace,
                Capability::ManageSpace,
            ],
        }
    }

    async fn begin_authorization(
        &self,
        _context: &ProviderContext,
        _request: AuthStartRequest,
    ) -> Result<AuthPrompt, ProviderError> {
        let client = self.get_or_create_client().await?;
        let oauth = client.oauth();

        let redirect_uri = Url::parse(REDIRECT_URI)
            .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

        let client_metadata = self.create_client_metadata(redirect_uri.clone());

        let auth_data = oauth
            .login(redirect_uri.clone(), None, Some(client_metadata), None)
            .build()
            .await
            .map_err(|e| match e {
                OAuthError::ClientRegistration(e) if e.to_string().contains("not supported") => {
                    ProviderError::new(ProviderErrorKind::Unsupported, "OAuth not supported by homeserver".to_owned())
                }
                _ => ProviderError::new(ProviderErrorKind::Internal, e.to_string()),
            })?;

        let flow_id = OpaqueId::new(format!("matrix-auth-{}", rand::random::<u64>())).expect("valid opaque id");
        Ok(AuthPrompt {
            flow_id,
            title: "Sign in to Matrix".to_owned(),
            instructions: Some("Complete sign-in in your browser. You will be redirected back to the app.".to_owned()),
            step: fr_chat_provider_api::AuthStepKind::OpenExternalUrl {
                url: auth_data.url.to_string(),
                callback_uri: REDIRECT_URI.to_owned(),
            },
        })
    }

    async fn continue_authorization(
        &self,
        _context: &ProviderContext,
        response: AuthResponse,
    ) -> Result<AuthStep, ProviderError> {
        let client = self.get_or_create_client().await?;
        let oauth = client.oauth();

        let query_string = match response.values.get("query_string") {
            Some(AuthorizationValue::PublicText(qs)) => qs.clone(),
            _ => return Err(ProviderError::new(ProviderErrorKind::InvalidInput, "Missing query string from redirect".to_owned())),
        };

        oauth
            .finish_login(QueryString(query_string).into())
            .await
            .map_err(|e| ProviderError::new(ProviderErrorKind::AuthorizationRequired, e.to_string()))?;

        let user_id = client
            .user_id()
            .ok_or_else(|| ProviderError::new(ProviderErrorKind::Internal, "No user ID after login".to_owned()))?;

        let account = self.create_account(user_id)?;
        Ok(AuthStep::Authorized(account))
    }

    async fn revoke_account(
        &self,
        _context: &ProviderContext,
        _account: &AccountRef,
    ) -> Result<(), ProviderError> {
        let client = self.get_or_create_client().await?;
        client
            .logout()
            .await
            .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
        Ok(())
    }

    async fn sync(
        &self,
        _context: &ProviderContext,
        account: &Account,
        _cursor: Option<SyncCursor>,
        _limit: u16,
    ) -> Result<SyncBatch, ProviderError> {
        // Imported from sync.rs
        crate::sync::sync(self, _context, account, _cursor, _limit).await
    }

    async fn execute(
        &self,
        _context: &ProviderContext,
        request: ActionRequest,
    ) -> Result<ActionOutcome, ProviderError> {
        // Imported from execute.rs
        crate::execute::execute(self, _context, request).await
    }

    async fn search(
        &self,
        _context: &ProviderContext,
        _request: SearchRequest,
    ) -> Result<Vec<SearchHit>, ProviderError> {
        Err(ProviderError::new(ProviderErrorKind::Unsupported, "Search not implemented".to_owned()))
    }
}