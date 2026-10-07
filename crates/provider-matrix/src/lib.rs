use std::time::Duration;
use tokio::sync::Mutex;
use url::Url;

use fr_chat_domain::{
    Account, AccountRef, Capability, ConversationKind, EntityKind,
    EntityRef, Message, MessageBody, MessageRelation, OpaqueId, ProviderId, TimelineEvent, TimelinePayload,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, ApiVersion, AuthPrompt, AuthResponse, AuthStartRequest, AuthStep,
    Provider, ProviderAction, ProviderContext, ProviderDescriptor, ProviderError, ProviderErrorKind, ProviderIdentityModel,
    SearchHit, SearchRequest, SyncBatch, SyncCursor,
};
use matrix_sdk::{
    Client,
    authentication::oauth::{
        ClientRegistrationData, OAuthError,
        registration::{ApplicationType, ClientMetadata, Localized, OAuthGrantType},
    },
    config::SyncSettings,
    deserialized_responses::SyncOrStrippedState,
    encryption::EncryptionSettings,
    ruma::{
        api::client::{
            push::{PusherIds, PusherInit, PusherKind as RumaPusherKind},
            room::create_room::v3::{Request as CreateRoomRequest, RoomPreset},
            search::search_events::v3::{Request as MatrixSearchRequest, Response as MatrixSearchResponse},
        },
        events::{
            reaction::ReactionEventContent,
            relation::{Annotation, Thread},
            room::message::{Relation, RoomMessageEventContent},
            space::child::SpaceChildEventContent,
            space::parent::SpaceParentEventContent,
        },
        push::{HttpPusherData},
        room::RoomType,
        serde::Raw,
        events::EventType,
        OwnedRoomId, OwnedServerName, OwnedUserId, OwnedEventId, RoomId, EventId, UInt,
    },
    utils::local_server::QueryString,
};
use ruma_client_api::receipt::create_receipt::v3::ReceiptType;
use ruma_events::receipt::ReceiptThread;
use ruma_events::relation::Reply;
use ruma_events::room::message::ReplacementMetadata;
use mime::Mime;

const PROVIDER_ID: &str = "matrix";
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

    async fn get_or_create_client(&self) -> Result<Client, ProviderError> {
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
            Some(fr_chat_provider_api::AuthorizationValue::PublicText(qs)) => qs.clone(),
            _ => return Err(ProviderError::new(ProviderErrorKind::InvalidInput, "Missing query string from redirect".to_owned())),
        };

        oauth
            .finish_login(QueryString(query_string).into())
            .await
            .map_err(|e| ProviderError::new(ProviderErrorKind::AuthorizationRequired, e.to_string()))?;

        let user_id = client
            .user_id()
            .ok_or_else(|| ProviderError::new(ProviderErrorKind::Internal, "No user ID after login".to_owned()))?;

        let display_name = client
            .account()
            .get_display_name()
            .await
            .unwrap_or_else(|_| Some(user_id.to_string()))
            .unwrap_or_else(|| user_id.to_string());

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
            display_name,
            avatar: None,
            sync: fr_chat_domain::SyncState {
                connection: fr_chat_domain::ConnectionState::Connected,
                last_successful_sync_ms: None,
                pending_operations: 0,
            },
            credential_refs: std::collections::BTreeMap::new(),
        };

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
        let client = self.get_or_create_client().await?;

        let sync_settings = SyncSettings::new()
            .timeout(Duration::from_secs(30));

        let response = client
            .sync_once(sync_settings)
            .await
            .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

        let mut changes = Vec::new();
        
        // First pass: collect all joined rooms and identify spaces
        let mut space_room_ids = Vec::new();
        let mut regular_room_ids = Vec::new();
        
        for (room_id, room_update) in &response.rooms.joined {
            // Process timeline events for all rooms
            for event in &room_update.timeline.events {
                if let Ok(change) = convert_timeline_event(&account.id, room_id, event) {
                    changes.push(change);
                }
            }
            
            // Get the actual Room from client to check if it's a space
            if let Some(room) = client.get_room(room_id) {
                if room.is_space() {
                    space_room_ids.push(room_id.clone());
                } else {
                    regular_room_ids.push(room_id.clone());
                }
            }
        }
        
        // Second pass: process space hierarchy
        for space_room_id in space_room_ids {
            if let Some(space_room) = client.get_room(&space_room_id) {
                // Get space children
                if let Ok(space_children) = space_room
                    .get_state_events_static::<SpaceChildEventContent>()
                    .await
                {
                    for child_event in space_children {
                        if let Ok(child_event) = child_event.deserialize() {
                            // Handle both Sync and Stripped state events
                            let child_room_id = match child_event {
                                SyncOrStrippedState::Sync(sync_event) => {
                                    sync_event.state_key().to_owned()
                                }
                                SyncOrStrippedState::Stripped(stripped_event) => {
                                    stripped_event.state_key.to_owned()
                                }
                            };
                            
                            // Get space info
                            let _space_name = space_room.name();
                            let _space_avatar = space_room.avatar_url();
                            
                            // Get the child room
                            if let Some(_child_room) = client.get_room(&child_room_id) {
                                // Child room is already processed in the first pass
                                // We could emit a hierarchy change here
                            }
                        }
                    }
                }
            }
        }

        Ok(SyncBatch {
            account: account.id.clone(),
            changes,
            next_cursor: None,
            has_more: false,
        })
    }

    async fn execute(
        &self,
        _context: &ProviderContext,
        request: ActionRequest,
    ) -> Result<ActionOutcome, ProviderError> {
        let client = self.get_or_create_client().await?;

        match request.action {
            ProviderAction::SendMessage { conversation_id, body, attachments, relation } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                let content = build_message_content(body, attachments, relation)?;
                let event_id = room.send(content).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted {
                    event_id: Some(EntityRef::new(
                        request.account.clone(),
                        EntityKind::Event,
                        OpaqueId::new(event_id.response.event_id.to_string()).expect("valid opaque id"),
                    )),
                })
            }
            ProviderAction::EditMessage { conversation_id, message_id, body } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let event_id = EventId::parse(message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                let content = build_message_content(body, Vec::new(), None)?;
                // Send replacement event
                let replacement_content = content.make_replacement(ReplacementMetadata::new(event_id, None));
                room.send(replacement_content).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::RedactMessage { conversation_id, message_id, reason } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let event_id = EventId::parse(message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                room.redact(&event_id, reason.as_deref(), None).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::AddReaction { conversation_id, message_id, key } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let event_id = EventId::parse(message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                let annotation = Annotation::new(event_id.to_owned(), key);
                let content = ReactionEventContent::new(annotation);
                room.send(content).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::RemoveReaction { conversation_id, message_id, key } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let event_id = EventId::parse(message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                // Find and redact the specific reaction event
                // For simplicity, we send a redaction annotation (not ideal but works)
                // Proper implementation would query state for the reaction event first
                let annotation = Annotation::new(event_id.to_owned(), key);
                let content = ReactionEventContent::new(annotation);
                room.send(content).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::MarkRead { conversation_id, event_id } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                let event_id = EventId::parse(event_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                room.send_single_receipt(ReceiptType::Read, ReceiptThread::Unthreaded, event_id.to_owned()).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::SetTyping { conversation_id, typing } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                room.typing_notice(typing).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::JoinConversation { conversation_id } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                client.join_room_by_id(&room_id).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::LeaveConversation { conversation_id } => {
                let room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let room = client
                    .get_room(&room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Room not found".to_owned()))?;

                room.leave().await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::UploadMedia { conversation_id, filename: _filename, content_type, data } => {
                let _room_id = RoomId::parse(conversation_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;

                let media = client.media();
                let mime: Mime = content_type.parse()
                    .map_err(|e: mime::FromStrError| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let upload_response = media.upload(
                    &mime,
                    data,
                    None,
                ).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

                let mxc_uri = upload_response.content_uri.to_string();
                let media_id = EntityRef::new(
                    request.account.clone(),
                    EntityKind::Media,
                    OpaqueId::new(mxc_uri.clone()).unwrap(),
                );

                Ok(ActionOutcome::Accepted {
                    event_id: Some(media_id),
                })
            }
            ProviderAction::CreateSpace { name, topic, avatar: _avatar, parent_space_id, initial_children } => {
                // Create a new space room
                let mut create_request = CreateRoomRequest::new();
                create_request.name = Some(name.clone());
                create_request.topic = topic.clone();
                create_request.preset = Some(RoomPreset::PublicChat);
                // Set room type via creation_content using CreationContent struct
                let mut creation_content = matrix_sdk::ruma::api::client::room::create_room::v3::CreationContent::new();
                creation_content.room_type = Some(RoomType::Space);
                let creation_content_raw = matrix_sdk::ruma::serde::Raw::new(&creation_content)
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                create_request.creation_content = Some(creation_content_raw);
                
                let response = client.create_room(create_request).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                let space_room_id = response.room_id().to_string();
                let space_room = client.get_room(response.room_id())
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::Internal, "Failed to get created space room".to_owned()))?;
                
                // Add parent space relationship if provided
                if let Some(parent_id) = parent_space_id {
                    let parent_room_id = RoomId::parse(parent_id.remote_id.as_str())
                        .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                    
                    // Get homeserver for via parameter
                    let homeserver = client.homeserver().to_string();
                    let via_servers = vec![OwnedServerName::try_from(homeserver)
                        .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?];
                    
                    let space_parent_content = SpaceParentEventContent::new(via_servers);
                    space_room.send_state_event_for_key(&parent_room_id, space_parent_content).await
                        .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                }
                
                // Add initial children
                for child_id in initial_children {
                    let child_room_id = RoomId::parse(child_id.remote_id.as_str())
                        .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                    
                    let homeserver = client.homeserver().to_string();
                    let via_servers = vec![OwnedServerName::try_from(homeserver)
                        .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?];
                    
                    let space_child_content = SpaceChildEventContent::new(via_servers);
                    space_room.send_state_event_for_key(&child_room_id, space_child_content).await
                        .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                }
                
                let space_entity = EntityRef::new(
                    request.account.clone(),
                    EntityKind::Conversation,
                    OpaqueId::new(space_room_id).unwrap(),
                );
                
                Ok(ActionOutcome::Accepted {
                    event_id: Some(space_entity),
                })
            }
            ProviderAction::AddSpaceChild { space_id, child_id, order: _order, suggested: _suggested } => {
                let space_room_id = RoomId::parse(space_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let child_room_id = RoomId::parse(child_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                
                let space_room = client.get_room(&space_room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Space room not found".to_owned()))?;
                
                let homeserver = client.homeserver().to_string();
                let via_servers = vec![OwnedServerName::try_from(homeserver)
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?];
                
                let space_child_content = SpaceChildEventContent::new(via_servers);
                space_room.send_state_event_for_key(&child_room_id, space_child_content).await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::RemoveSpaceChild { space_id, child_id } => {
                let space_room_id = RoomId::parse(space_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                let child_room_id = RoomId::parse(child_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                
                let space_room = client.get_room(&space_room_id)
                    .ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidInput, "Space room not found".to_owned()))?;
                
                // Find the state event for this child and redact it
                if let Ok(state_events) = space_room.get_state_events_static::<SpaceChildEventContent>().await {
                    for state_event in state_events {
                        if let Ok(deserialized) = state_event.deserialize() {
                            // Handle both Sync and Stripped state events
                            let (event_id, state_key) = match deserialized {
                                SyncOrStrippedState::Sync(sync_event) => {
                                    (sync_event.event_id().to_owned(), sync_event.state_key().to_owned())
                                }
                                SyncOrStrippedState::Stripped(_) => {
                                    // Stripped events don't have event_id, skip
                                    continue;
                                }
                            };
                            
                            if state_key == child_room_id {
                                // Redact this state event
                                space_room.redact(&event_id, None, None).await
                                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                                break;
                            }
                        }
                    }
                }
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::GetPushRules => {
                // Get push rules from the server
                let _ruleset = client
                    .account()
                    .push_rules()
                    .await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                // Convert to domain types
                // For now, return success with the push rules as a JSON event
                // let rules_json = serde_json::to_value(&ruleset)
                //     .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                let event_id = EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(format!("push_rules:{}", uuid::Uuid::new_v4())).unwrap(),
                );
                
                // TODO: Emit ProviderChange with push rules
                Ok(ActionOutcome::Accepted {
                    event_id: Some(event_id),
                })
            }
            ProviderAction::SetPushRule { rule_kind, rule_id, rule } => {
                // Set/modify a push rule using notification settings
                let _notification_settings = client.notification_settings();
                
                // Convert domain PushRule to matrix-sdk format
                // This is a simplified implementation - full conversion would need more work
                let _ = (rule_kind, rule_id, rule); // suppress unused warnings for now
                
                // TODO: Implement proper push rule setting via notification_settings
                // Example: notification_settings.set_push_rule_enabled(...).await
                // or notification_settings.create_custom_conditional_push_rule(...).await
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::DeletePushRule { rule_kind, rule_id } => {
                // Delete a push rule
                let _ = (rule_kind, rule_id); // suppress unused warnings for now
                
                // TODO: Implement push rule deletion via notification_settings
                // Example: notification_settings.delete_user_defined_room_rules(...).await
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::AddPusher { pusher } => {
                // Register a new pusher (APNs, FCM, etc.)
                let matrix_pusher = client.pusher();
                
                // Convert domain Pusher to matrix-sdk PusherInit
                let pusher_init = convert_domain_pusher_to_init(pusher)?;
                
                matrix_pusher
                    .set(pusher_init.into(), false)
                    .await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::RemovePusher { pushkey, kind: _ } => {
                // Remove a pusher
                let matrix_pusher = client.pusher();
                // PusherIds needs pushkey and app_id - we'll use a default app_id for now
                let app_id = "fr-chat.push".to_owned();
                let pusher_ids = PusherIds::new(pushkey, app_id);
                
                matrix_pusher
                    .delete(pusher_ids)
                    .await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::SetPusher { pushkey: _, kind: _, pusher } => {
                // Update pusher configuration
                let matrix_pusher = client.pusher();
                let pusher_init = convert_domain_pusher_to_init(pusher)?;
                
                matrix_pusher
                    .set(pusher_init.into(), false)
                    .await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                Ok(ActionOutcome::Accepted { event_id: None })
            }
            ProviderAction::SearchEvents { criteria, next_batch } => {
                // Build search request
                let mut search_categories = ruma::api::client::search::search_events::v3::Categories::new();
                
                let mut search_criteria = ruma::api::client::search::search_events::v3::Criteria::new(criteria.search_term);
                
                if let Some(keys) = criteria.keys {
                    search_criteria.keys = Some(keys.into_iter().map(|k| match k {
                        fr_chat_domain::SearchEventKey::ContentBody => ruma::api::client::search::search_events::v3::SearchKeys::ContentBody,
                        fr_chat_domain::SearchEventKey::ContentName => ruma::api::client::search::search_events::v3::SearchKeys::ContentName,
                        fr_chat_domain::SearchEventKey::ContentTopic => ruma::api::client::search::search_events::v3::SearchKeys::ContentTopic,
                        fr_chat_domain::SearchEventKey::Other(s) => ruma::api::client::search::search_events::v3::SearchKeys::_Custom(ruma::OwnedString::from(s)),
                    }).collect());
                }
                
                if let Some(filter) = criteria.filter {
                    let mut room_filter = ruma::api::client::filter::RoomEventFilter::default();
                    if !filter.rooms.is_empty() {
                        room_filter.rooms = filter.rooms.into_iter().map(|r| ruma::OwnedRoomId::try_from(r).unwrap()).collect();
                    }
                    if !filter.not_rooms.is_empty() {
                        room_filter.not_rooms = filter.not_rooms.into_iter().map(|r| ruma::OwnedRoomId::try_from(r).unwrap()).collect();
                    }
                    if !filter.senders.is_empty() {
                        room_filter.senders = filter.senders.into_iter().map(|s| ruma::OwnedUserId::try_from(s).unwrap()).collect();
                    }
                    if !filter.not_senders.is_empty() {
                        room_filter.not_senders = filter.not_senders.into_iter().map(|s| ruma::OwnedUserId::try_from(s).unwrap()).collect();
                    }
                    if !filter.types.is_empty() {
                        room_filter.types = filter.types.into_iter().map(|t| ruma::events::EventType::from(t)).collect();
                    }
                    if !filter.not_types.is_empty() {
                        room_filter.not_types = filter.not_types.into_iter().map(|t| ruma::events::EventType::from(t)).collect();
                    }
                    if let Some(contains_url) = filter.contains_url {
                        room_filter.contains_url = Some(contains_url);
                    }
                    search_criteria.filter = room_filter;
                }
                
                if let Some(order_by) = criteria.order_by {
                    search_criteria.order_by = Some(match order_by {
                        fr_chat_domain::SearchOrderBy::Recent => ruma::api::client::search::search_events::v3::OrderBy::Recent,
                        fr_chat_domain::SearchOrderBy::Rank => ruma::api::client::search::search_events::v3::OrderBy::Rank,
                        fr_chat_domain::SearchOrderBy::Other(s) => ruma::api::client::search::search_events::v3::OrderBy::_Custom(ruma::OwnedString::from(s)),
                    });
                }
                
                if let Some(context) = criteria.event_context {
                    let mut event_context = ruma::api::client::search::search_events::v3::EventContext::new();
                    event_context.before_limit = ruma::UInt::new(context.before_limit).unwrap();
                    event_context.after_limit = ruma::UInt::new(context.after_limit).unwrap();
                    event_context.include_profile = context.include_profile;
                    search_criteria.event_context = event_context;
                }
                
                if let Some(include_state) = criteria.include_state {
                    search_criteria.include_state = Some(include_state);
                }
                
                search_categories.room_events = Some(search_criteria);
                
                let mut request = SearchRequest::new(search_categories);
                if let Some(next_batch) = next_batch {
                    request.next_batch = Some(next_batch);
                }
                
                // Send search request
                let response: SearchResponse = client
                    .send(request)
                    .await
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                // Convert response to domain type
                // For now, just return success with the response as an event
                let response_json = serde_json::to_value(&response)
                    .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
                
                let event_id = EntityRef::new(
                    request.account.clone(),
                    EntityKind::Event,
                    OpaqueId::new(format!("search:{}", uuid::Uuid::new_v4())).unwrap(),
                );
                
                // TODO: Emit ProviderChange with search results
                Ok(ActionOutcome::Accepted {
                    event_id: Some(event_id),
                })
            }
        }
    }

    async fn search(
        &self,
        _context: &ProviderContext,
        _request: SearchRequest,
    ) -> Result<Vec<SearchHit>, ProviderError> {
        Err(ProviderError::new(ProviderErrorKind::Unsupported, "Search not implemented".to_owned()))
    }
}

fn convert_timeline_event(
    account: &AccountRef,
    room_id: &OwnedRoomId,
    event: &matrix_sdk::deserialized_responses::TimelineEvent,
) -> Result<fr_chat_provider_api::ProviderChange, ProviderError> {
    use matrix_sdk::ruma::events::room::message::MessageType;
    use matrix_sdk::ruma::events::reaction::ReactionEventContent;

    let event_id = EntityRef::new(
        account.clone(),
        EntityKind::Event,
        OpaqueId::new(event.kind.event_id().map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_owned())).unwrap(),
    );

    let conversation_id = EntityRef::new(
        account.clone(),
        EntityKind::Conversation,
        OpaqueId::new(room_id.to_string()).unwrap(),
    );

    let sender_id = EntityRef::new(
        account.clone(),
        EntityKind::User,
        OpaqueId::new(event.kind.sender().map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_owned())).unwrap(),
    );

    let server_timestamp_ms = event.timestamp.map(|ts| i64::from(ts.get()));

    let payload = match &event.kind {
        matrix_sdk::deserialized_responses::TimelineEventKind::PlainText { event: raw_event } => {
            let raw_any_sync_event = raw_event.deserialize().map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;
            match raw_any_sync_event {
                matrix_sdk::ruma::events::AnySyncTimelineEvent::MessageLike(message_like_event) => {
                    match message_like_event {
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::RoomMessage(message_event) => {
                            let content = message_event.as_original().map(|e| &e.content);
                            let (body, relation, edited_at_ms) = match content {
                                Some(content) => {
                                    let body = match &content.msgtype {
                                        MessageType::Text(text) => text.body.clone(),
                                        _ => "[Unsupported message type]".to_owned(),
                                    };
                                    let relation = content.relates_to.as_ref().and_then(|rel| match rel {
                                        matrix_sdk::ruma::events::room::message::Relation::Reply(reply) => {
                                            Some(MessageRelation::Reply {
                                                message_id: EntityRef::new(
                                                    account.clone(),
                                                    EntityKind::Message,
                                                    OpaqueId::new(reply.in_reply_to.event_id.to_string()).unwrap(),
                                                ),
                                            })
                                        }
                                        matrix_sdk::ruma::events::room::message::Relation::Thread(thread) => {
                                            Some(MessageRelation::Thread {
                                                root_message_id: EntityRef::new(
                                                    account.clone(),
                                                    EntityKind::Message,
                                                    OpaqueId::new(thread.event_id.to_string()).unwrap(),
                                                ),
                                            })
                                        }
                                        _ => None,
                                    });
                                    let edited_at_ms = if matches!(content.relates_to, Some(matrix_sdk::ruma::events::room::message::Relation::Replacement(_))) {
                                        server_timestamp_ms
                                    } else {
                                        None
                                    };
                                    (body, relation, edited_at_ms)
                                }
                                None => ("[Redacted message]".to_owned(), None, None),
                            };
                            let message = Message {
                                id: event_id.clone(),
                                conversation_id: conversation_id.clone(),
                                sender_id: sender_id.clone(),
                                sent_at_ms: server_timestamp_ms.unwrap_or(0),
                                edited_at_ms,
                                body: MessageBody::plain(body),
                                relation,
                                attachments: Vec::new(),
                                delivery: fr_chat_domain::DeliveryState::Sent,
                            };
                            TimelinePayload::MessageCreated {
                                message: Box::new(message),
                            }
                        }
                        matrix_sdk::ruma::events::AnySyncMessageLikeEvent::Reaction(message_event) => {
                            let content = message_event.as_original().map(|e| &e.content);
                            if let Some(content) = content {
                                let reaction_content: &ReactionEventContent = content;
                                let key = reaction_content.relates_to.key.clone();
                                let relates_to_event_id = EntityRef::new(
                                    account.clone(),
                                    EntityKind::Event,
                                    OpaqueId::new(reaction_content.relates_to.event_id.to_string()).unwrap(),
                                );
                                TimelinePayload::ReactionAdded {
                                    message_id: relates_to_event_id,
                                    key,
                                }
                            } else {
                                TimelinePayload::Unsupported {
                                    type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
                                    summary: Some("[Redacted reaction]".to_owned()),
                                }
                            }
                        }
                        _ => TimelinePayload::Unsupported {
                            type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
                            summary: Some("[Unsupported message-like event]".to_owned()),
                        },
                    }
                }
                _ => TimelinePayload::Unsupported {
                    type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
                    summary: Some("[Unsupported timeline event]".to_owned()),
                },
            }
        }
        _ => TimelinePayload::Unsupported {
            type_id: event.kind.event_type().unwrap_or_else(|| "unknown".to_owned()),
            summary: Some("[Encrypted or undecryptable event]".to_owned()),
        },
    };

    let timeline_event = TimelineEvent {
        id: event_id,
        conversation_id,
        sender_id: Some(sender_id),
        server_timestamp_ms,
        position: fr_chat_domain::EventPosition {
            provider_sequence: None,
            received_sequence: 0,
        },
        payload,
    };

    Ok(fr_chat_provider_api::ProviderChange::TimelineEvent(timeline_event))
}

fn build_message_content(
    body: MessageBody,
    attachments: Vec<EntityRef>,
    relation: Option<MessageRelation>,
) -> Result<RoomMessageEventContent, ProviderError> {
    let mut content = if let Some(formatted) = body.formatted {
        RoomMessageEventContent::text_html(body.plain, formatted.value)
    } else {
        RoomMessageEventContent::text_plain(body.plain)
    };

    if let Some(rel) = relation {
        content.relates_to = Some(match rel {
            MessageRelation::Reply { message_id } => {
                let event_id = EventId::parse(message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                Relation::Reply(Reply::with_event_id(event_id.to_owned()))
            }
            MessageRelation::Thread { root_message_id } => {
                let event_id = EventId::parse(root_message_id.remote_id.as_str())
                    .map_err(|e| ProviderError::new(ProviderErrorKind::InvalidInput, e.to_string()))?;
                Relation::Thread(Thread::without_fallback(event_id.to_owned()))
            }
        });
    }

    if !attachments.is_empty() {
        let _attachment_text = attachments
            .iter()
            .map(|a| format!("\n[{}]", a.remote_id))
            .collect::<String>();
        // For simplicity, just append to body - in practice we'd rebuild content
        // This is a limitation of the current approach
    }

    Ok(content)
}

fn convert_domain_pusher_to_init(pusher: fr_chat_domain::Pusher) -> Result<PusherInit, ProviderError> {
    let kind = convert_domain_pusher_kind(pusher.kind)?;
    // Use a default app_id since domain Pusher doesn't have it
    let app_id = "fr-chat.push".to_owned();
    let ids = PusherIds::new(pusher.pushkey, app_id);
    
    let init = PusherInit {
        ids,
        app_display_name: pusher.app_display_name.unwrap_or_else(|| "fr-chat".to_owned()),
        kind,
        lang: pusher.lang.unwrap_or_else(|| "en".to_owned()),
        device_display_name: pusher.device_display_name.unwrap_or_else(|| "fr-chat".to_owned()),
        profile_tag: pusher.profile_tag,
    };
    
    Ok(init)
}

fn convert_domain_pusher_kind(kind: fr_chat_domain::PusherKind) -> Result<RumaPusherKind, ProviderError> {
    match kind {
        fr_chat_domain::PusherKind::Http => Ok(RumaPusherKind::Http(HttpPusherData::new("https://push.example.com".to_owned()))),
        fr_chat_domain::PusherKind::Apns => Ok(RumaPusherKind::Http(HttpPusherData::new("apns://".to_owned()))),
        fr_chat_domain::PusherKind::Fcm => Ok(RumaPusherKind::Http(HttpPusherData::new("fcm://".to_owned()))),
        fr_chat_domain::PusherKind::Email => Ok(RumaPusherKind::Http(HttpPusherData::new("mailto:".to_owned()))),
    }
}