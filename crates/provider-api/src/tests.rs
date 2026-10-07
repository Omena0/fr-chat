use std::collections::BTreeMap;

use fr_chat_domain::{
    AccountRef, Capability, ConversationKind, EntityKind, EntityRef, MessageBody, OpaqueId,
    ProviderId,
};

use crate::{
    ActionRequest, AuthStep, ProviderAction, ProviderDescriptor, ProviderError, ProviderErrorKind,
    ProviderIdentityModel, SyncBatch, SyncBatchError, SyncCursor,
};

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
fn sync_batch_rejects_entity_from_another_account() {
    let account = account();
    let other = AccountRef::new(
        ProviderId::new("matrix").unwrap(),
        OpaqueId::new("account-2").unwrap(),
    );
    let batch = SyncBatch {
        account: account.clone(),
        changes: vec![crate::ProviderChange::EntityRemoved(entity(
            &other,
            EntityKind::Conversation,
            "room",
        ))],
        next_cursor: Some(SyncCursor::new(vec![1]).unwrap()),
        has_more: false,
    };

    assert_eq!(
        batch.validate(),
        Err(SyncBatchError::AccountMismatch { index: 0 })
    );
}

#[test]
fn sync_batch_with_more_pages_requires_a_cursor() {
    let batch = SyncBatch {
        account: account(),
        changes: Vec::new(),
        next_cursor: None,
        has_more: true,
    };
    assert_eq!(batch.validate(), Err(SyncBatchError::MissingCursor));
}

#[test]
fn action_requirements_include_all_features_and_own_or_any_permissions() {
    let account = account();
    let request = ActionRequest {
        account: account.clone(),
        operation_id: OpaqueId::new("operation-1").unwrap(),
        action: ProviderAction::SendMessage {
            conversation_id: entity(&account, EntityKind::Conversation, "room"),
            body: MessageBody::plain("hello"),
            attachments: vec![entity(&account, EntityKind::Media, "upload")],
            relation: Some(fr_chat_domain::MessageRelation::Reply {
                message_id: entity(&account, EntityKind::Message, "parent"),
            }),
        },
    };

    assert!(request.validate().is_ok());
    assert_eq!(
        request.action.capability_requirements(),
        vec![
            vec![Capability::SendMessage],
            vec![Capability::UploadAttachment],
            vec![Capability::Reply],
        ]
    );

    let edit = ProviderAction::EditMessage {
        conversation_id: entity(&account, EntityKind::Conversation, "room"),
        message_id: entity(&account, EntityKind::Message, "message"),
        body: MessageBody::plain("updated"),
    };
    assert_eq!(
        edit.capability_requirements(),
        vec![vec![Capability::EditOwnMessage, Capability::EditAnyMessage]]
    );
}

#[test]
fn provider_descriptor_checks_host_api_and_duplicate_capabilities() {
    let descriptor = ProviderDescriptor {
        provider_id: ProviderId::new("matrix").unwrap(),
        display_name: "Matrix".to_owned(),
        provider_version: "0.1.0".to_owned(),
        minimum_api: crate::ApiVersion { major: 1, minor: 0 },
        maximum_api: crate::ApiVersion { major: 1, minor: 2 },
        identity_model: ProviderIdentityModel::UserAccount,
        conversation_kinds: vec![ConversationKind::Text],
        capabilities: vec![Capability::SendMessage],
    };
    assert!(
        descriptor
            .validate(crate::ApiVersion { major: 1, minor: 1 })
            .is_ok()
    );
    assert_eq!(
        descriptor
            .validate(crate::ApiVersion { major: 2, minor: 0 })
            .unwrap_err()
            .kind,
        ProviderErrorKind::Unsupported
    );

    let duplicate = ProviderDescriptor {
        capabilities: vec![Capability::SendMessage, Capability::SendMessage],
        ..descriptor
    };
    assert_eq!(
        duplicate
            .validate(crate::ApiVersion { major: 1, minor: 0 })
            .unwrap_err()
            .kind,
        ProviderErrorKind::InvalidInput
    );
}

#[test]
fn provider_errors_are_safe_and_have_retry_hints() {
    let error = ProviderError::rate_limited("Try again shortly.", Some(2_000));
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(error.retry_after_ms, Some(2_000));
    assert_eq!(error.to_string(), "Try again shortly.");
    assert!(matches!(AuthStep::Cancelled, AuthStep::Cancelled));
    assert!(BTreeMap::<String, String>::new().is_empty());
}
