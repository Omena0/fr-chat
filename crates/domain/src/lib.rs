#![forbid(unsafe_code)]

mod capabilities;
mod ids;
mod model;
mod timeline;

pub use capabilities::{Capability, CapabilityMap, CapabilityState};
pub use ids::{AccountRef, EntityKind, EntityRef, OpaqueId, ProviderId, RemoteOrigin};
pub use model::{
    Account, Attachment, ConnectionState, Conversation, ConversationKind, DeliveryState,
    FormattedBody, Member, Message, MessageBody, MessageRelation, ModelError, Role, Server,
    SpaceChildOrder, SyncState, UnreadState, User,
    PushRuleKind, PushCondition, PushAction, PushTweaks, PushRule, PushRuleset,
    Pusher, PusherKind, PushNotification, PushCounts, PushDevice,
    SearchEventKey, SearchOrderBy, SearchEventFilter, SearchEventContext, SearchCriteria,
    SearchCategories, SearchResultGroup, SearchResultGroups, SearchResult,
    SearchEventContextResult, SearchUserProfile, SearchRoomEventsResult, SearchResponse,
};
pub use timeline::{
    EventPosition, MembershipChange, TimelineError, TimelineEvent, TimelinePayload,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountRef {
        AccountRef::new(
            ProviderId::new("matrix").unwrap(),
            OpaqueId::new("account-1").unwrap(),
        )
    }

    fn entity(account: &AccountRef, kind: EntityKind, id: &str) -> EntityRef {
        EntityRef::new(account.clone(), kind, OpaqueId::new(id).unwrap())
    }

    #[test]
    fn remote_references_include_provider_account_and_entity_kind() {
        let first = entity(&account(), EntityKind::Conversation, "42");
        let other_account = AccountRef::new(
            ProviderId::new("matrix").unwrap(),
            OpaqueId::new("account-2").unwrap(),
        );
        let other_provider = AccountRef::new(
            ProviderId::new("example.chat").unwrap(),
            OpaqueId::new("account-1").unwrap(),
        );

        assert_ne!(
            first,
            entity(&other_account, EntityKind::Conversation, "42")
        );
        assert_ne!(
            first,
            entity(&other_provider, EntityKind::Conversation, "42")
        );
        assert_ne!(first, entity(&account(), EntityKind::User, "42"));
    }

    #[test]
    fn conversation_rejects_cross_account_parent() {
        let first_account = account();
        let second_account = AccountRef::new(
            ProviderId::new("matrix").unwrap(),
            OpaqueId::new("account-2").unwrap(),
        );
        let conversation = Conversation {
            id: entity(&first_account, EntityKind::Conversation, "room-a"),
            server_id: None,
            parent_id: Some(entity(
                &second_account,
                EntityKind::Conversation,
                "category-b",
            )),
            kind: ConversationKind::Text,
            name: "general".to_owned(),
            topic: None,
            origin: None,
            capabilities: CapabilityMap::default(),
            position: None,
            unread: UnreadState::default(),
        };

        assert!(conversation.validate().is_err());
    }

    #[test]
    fn capability_defaults_to_unsupported_and_can_represent_unknown_values() {
        let capabilities = CapabilityMap::default();
        assert_eq!(
            capabilities.state(&Capability::SendMessage),
            CapabilityState::Unsupported
        );
        assert_eq!(
            Capability::Other("custom.operation".to_owned()).as_str(),
            "custom.operation"
        );
    }

    #[test]
    fn message_validates_scoped_conversation_sender_and_reply() {
        let scope = account();
        let conversation = entity(&scope, EntityKind::Conversation, "room-a");
        let sender = entity(&scope, EntityKind::User, "user-a");
        let message = Message {
            id: entity(&scope, EntityKind::Message, "$event-a"),
            conversation_id: conversation,
            sender_id: sender,
            sent_at_ms: 1_760_000_000_000,
            edited_at_ms: None,
            body: MessageBody::plain("hello"),
            relation: Some(MessageRelation::Reply {
                message_id: entity(&scope, EntityKind::Message, "$parent"),
            }),
            attachments: Vec::new(),
            delivery: DeliveryState::Sent,
        };

        assert!(message.validate().is_ok());
    }

    #[test]
    fn deserialization_cannot_construct_invalid_provider_ids() {
        let invalid = serde_json::from_str::<ProviderId>("\"UPPER CASE\"");
        assert!(invalid.is_err());
    }

    #[test]
    fn timeline_event_rejects_cross_account_sender() {
        let scope = account();
        let other_scope = AccountRef::new(
            ProviderId::new("matrix").unwrap(),
            OpaqueId::new("account-2").unwrap(),
        );
        let event = TimelineEvent {
            id: entity(&scope, EntityKind::Event, "$event"),
            conversation_id: entity(&scope, EntityKind::Conversation, "room"),
            sender_id: Some(entity(&other_scope, EntityKind::User, "user")),
            server_timestamp_ms: Some(1),
            position: EventPosition {
                provider_sequence: None,
                received_sequence: 1,
            },
            payload: TimelinePayload::Unsupported {
                type_id: "unknown.event".to_owned(),
                summary: None,
            },
        };

        assert!(event.validate().is_err());
    }

    #[test]
    fn timeline_event_validates_supported_system_events() {
        let scope = account();
        let event = TimelineEvent {
            id: entity(&scope, EntityKind::Event, "$event"),
            conversation_id: entity(&scope, EntityKind::Conversation, "room"),
            sender_id: None,
            server_timestamp_ms: Some(1),
            position: EventPosition {
                provider_sequence: Some("cursor-10".to_owned()),
                received_sequence: 1,
            },
            payload: TimelinePayload::System {
                type_id: "room.name.changed".to_owned(),
                summary: "Room renamed".to_owned(),
            },
        };

        assert!(event.validate().is_ok());
    }
}
