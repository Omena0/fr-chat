
use crate::auth::MatrixProvider;
use crate::auth::PROVIDER_ID;
use crate::convert::convert_timeline_event;
use fr_chat_domain::{EntityKind, EntityRef, OpaqueId, Server, Conversation};
use fr_chat_provider_api::{ProviderChange, ProviderContext, ProviderError, ProviderErrorKind, SyncBatch, SyncCursor};

pub async fn sync(
    provider: &MatrixProvider,
    _context: &ProviderContext,
    account: &fr_chat_domain::Account,
    _cursor: Option<SyncCursor>,
    _limit: u16,
) -> Result<SyncBatch, ProviderError> {
    let client = provider.get_or_create_client().await?;

    let sync_settings = matrix_sdk::config::SyncSettings::new()
        .timeout(std::time::Duration::from_secs(30));

    let response = client
        .sync_once(sync_settings)
        .await
        .map_err(|e| ProviderError::new(ProviderErrorKind::Internal, e.to_string()))?;

    let mut changes = Vec::new();
    let mut seen_servers = std::collections::HashSet::new();
    let mut seen_conversations = std::collections::HashSet::new();

    for (room_id, room_update) in &response.rooms.joined {
        // Emit server change based on homeserver
        let server_name = room_id
            .server_name()
            .expect("Room ID should have server name")
            .to_string();
        let server_id = EntityRef::new(
            account.id.clone(),
            EntityKind::Server,
            OpaqueId::new(format!("{}:{}", PROVIDER_ID, server_name)).unwrap(),
        );
        if seen_servers.insert(server_name.clone()) {
            changes.push(ProviderChange::ServerUpserted(Server {
                id: server_id.clone(),
                parent_id: None,
                name: server_name.clone(),
                description: None,
                icon: None,
                origin: None,
                capabilities: fr_chat_domain::CapabilityMap::default(),
                position: None,
            }));
        }

        // Emit conversation change
        if seen_conversations.insert(room_id.to_string()) {
            changes.push(ProviderChange::ConversationUpserted(Conversation {
                id: EntityRef::new(
                    account.id.clone(),
                    EntityKind::Conversation,
                    OpaqueId::new(room_id.to_string()).unwrap(),
                ),
                server_id: Some(server_id.clone()),
                parent_id: None,
                kind: fr_chat_domain::ConversationKind::Text,
                name: format!("Room {}", room_id),
                topic: None,
                origin: None,
                capabilities: fr_chat_domain::CapabilityMap::default(),
                position: None,
                unread: fr_chat_domain::UnreadState::default(),
            }));
        }

        for event in &room_update.timeline.events {
            if let Ok(change) = convert_timeline_event(&account.id, room_id, event) {
                changes.push(change);
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