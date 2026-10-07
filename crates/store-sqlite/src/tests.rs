use std::path::Path;

use fr_chat_app_core::{AppStore, OutboxState, StoreError};
use fr_chat_domain::{
    Account, AccountRef, CapabilityMap, ConnectionState, Conversation, ConversationKind,
    DeliveryState, EntityKind, EntityRef, Message, MessageBody, OpaqueId, ProviderId, SyncState,
    TimelineEvent,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, ProviderChange, SecretBytes, SyncBatch, SyncCursor,
};
use tempfile::tempdir;

use crate::SqliteAppStore;

fn account() -> AccountRef {
    AccountRef::new(
        ProviderId::new("matrix").unwrap(),
        OpaqueId::new("account-1").unwrap(),
    )
}

fn entity(account: &AccountRef, kind: EntityKind, id: &str) -> EntityRef {
    EntityRef::new(account.clone(), kind, OpaqueId::new(id).unwrap())
}

fn account_record(account: &AccountRef) -> Account {
    Account {
        id: account.clone(),
        user_id: Some(entity(account, EntityKind::User, "self")),
        display_name: "Local user".to_owned(),
        avatar: None,
        sync: SyncState {
            connection: ConnectionState::Connected,
            last_successful_sync_ms: Some(1),
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
        position: Some(0),
        unread: Default::default(),
    }
}

fn event(
    account: &AccountRef,
    id: &str,
    sequence: u64,
    payload: fr_chat_domain::TimelinePayload,
) -> TimelineEvent {
    TimelineEvent {
        id: entity(account, EntityKind::Event, id),
        conversation_id: entity(account, EntityKind::Conversation, "room-1"),
        sender_id: Some(entity(account, EntityKind::User, "self")),
        server_timestamp_ms: Some(1_760_000_000_000 + sequence as i64),
        position: fr_chat_domain::EventPosition {
            provider_sequence: Some(sequence.to_string()),
            received_sequence: sequence,
        },
        payload,
    }
}

fn message(account: &AccountRef) -> Message {
    Message {
        id: entity(account, EntityKind::Message, "$message"),
        conversation_id: entity(account, EntityKind::Conversation, "room-1"),
        sender_id: entity(account, EntityKind::User, "self"),
        sent_at_ms: 1_760_000_000_000,
        edited_at_ms: None,
        body: MessageBody::plain("before edit"),
        relation: None,
        attachments: Vec::new(),
        delivery: DeliveryState::Sent,
    }
}

fn batch(account: &AccountRef, changes: Vec<ProviderChange>, cursor: u8) -> SyncBatch {
    SyncBatch {
        account: account.clone(),
        changes,
        next_cursor: Some(SyncCursor::new(vec![cursor]).unwrap()),
        has_more: false,
    }
}

fn open(path: impl AsRef<Path>) -> SqliteAppStore {
    SqliteAppStore::open_encrypted(path, test_key()).unwrap()
}

fn test_key() -> SecretBytes {
    SecretBytes::new(vec![0x5a; 32])
}

#[tokio::test]
async fn account_and_sync_cursor_survive_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("fr-chat.sqlite3");
    let account = account();
    {
        let store = open(&path);
        store.put_account(account_record(&account)).await.unwrap();
        store
            .commit_sync_batch(batch(
                &account,
                vec![ProviderChange::ConversationUpserted(conversation(&account))],
                4,
            ))
            .await
            .unwrap();
    }

    let store = open(&path);
    assert_eq!(
        store.account(&account).await.unwrap(),
        Some(account_record(&account))
    );
    assert_eq!(
        store.sync_cursor(&account).await.unwrap(),
        Some(SyncCursor::new(vec![4]).unwrap())
    );
    assert_eq!(
        store.conversations(&account, None).await.unwrap(),
        vec![conversation(&account)]
    );
}

#[tokio::test]
async fn sync_commit_is_idempotent_and_projects_edits_and_redactions() {
    let directory = tempdir().unwrap();
    let store = open(directory.path().join("fr-chat.sqlite3"));
    let account = account();
    store.put_account(account_record(&account)).await.unwrap();
    let created = event(
        &account,
        "$event-created",
        1,
        fr_chat_domain::TimelinePayload::MessageCreated {
            message: Box::new(message(&account)),
        },
    );
    let edited = event(
        &account,
        "$event-edited",
        2,
        fr_chat_domain::TimelinePayload::MessageEdited {
            message_id: entity(&account, EntityKind::Message, "$message"),
            body: MessageBody::plain("after edit"),
            edited_at_ms: 1_760_000_000_002,
        },
    );
    let redacted = event(
        &account,
        "$event-redacted",
        3,
        fr_chat_domain::TimelinePayload::MessageRedacted {
            message_id: entity(&account, EntityKind::Message, "$message"),
            reason: None,
        },
    );
    let first_batch = batch(
        &account,
        vec![
            ProviderChange::ConversationUpserted(conversation(&account)),
            ProviderChange::TimelineEvent(created.clone()),
            ProviderChange::TimelineEvent(edited.clone()),
        ],
        1,
    );

    let first = store.commit_sync_batch(first_batch.clone()).await.unwrap();
    assert_eq!(first.new_events, 2);
    assert_eq!(
        store
            .message(&account, &message(&account).id)
            .await
            .unwrap()
            .unwrap()
            .body
            .plain,
        "after edit"
    );
    let before_edit = store
        .timeline(&account, &conversation(&account).id, Some(&edited.id), 10)
        .await
        .unwrap();
    assert_eq!(before_edit, vec![created]);

    let replay = store.commit_sync_batch(first_batch).await.unwrap();
    assert_eq!(replay.new_events, 0);
    assert_eq!(
        store
            .timeline(&account, &conversation(&account).id, None, 10)
            .await
            .unwrap()
            .len(),
        2
    );

    let redaction = store
        .commit_sync_batch(batch(
            &account,
            vec![ProviderChange::TimelineEvent(redacted)],
            2,
        ))
        .await
        .unwrap();
    assert_eq!(redaction.new_events, 1);
    let stored_message = store
        .message(&account, &message(&account).id)
        .await
        .unwrap()
        .unwrap();
    assert!(stored_message.body.plain.is_empty());
    let redacted_timeline = store
        .timeline(&account, &conversation(&account).id, None, 10)
        .await
        .unwrap();
    for event in redacted_timeline {
        match event.payload {
            fr_chat_domain::TimelinePayload::MessageCreated { message } => {
                assert!(message.body.plain.is_empty());
                assert!(message.attachments.is_empty());
            }
            fr_chat_domain::TimelinePayload::MessageEdited { body, .. } => {
                assert!(body.plain.is_empty());
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn outbox_claim_is_atomic_and_operation_ids_are_idempotent() {
    let directory = tempdir().unwrap();
    let store = open(directory.path().join("fr-chat.sqlite3"));
    let account = account();
    store.put_account(account_record(&account)).await.unwrap();
    let request = ActionRequest {
        account: account.clone(),
        operation_id: OpaqueId::new("operation-1").unwrap(),
        action: fr_chat_provider_api::ProviderAction::SendMessage {
            conversation_id: entity(&account, EntityKind::Conversation, "room-1"),
            body: MessageBody::plain("hello"),
            attachments: Vec::new(),
            relation: None,
        },
    };

    let queued = store.enqueue_action(request.clone()).await.unwrap();
    assert_eq!(queued.state, OutboxState::Queued);
    assert_eq!(store.enqueue_action(request.clone()).await.unwrap(), queued);
    assert!(
        store
            .claim_action(&account, &request.operation_id)
            .await
            .unwrap()
    );
    assert!(
        !store
            .claim_action(&account, &request.operation_id)
            .await
            .unwrap()
    );
    let accepted = store
        .complete_action(
            &account,
            &request.operation_id,
            ActionOutcome::Accepted { event_id: None },
        )
        .await
        .unwrap();
    assert_eq!(accepted.state, OutboxState::Accepted);
    assert!(
        !store
            .claim_action(&account, &request.operation_id)
            .await
            .unwrap()
    );

    let different_request = ActionRequest {
        action: fr_chat_provider_api::ProviderAction::SendMessage {
            conversation_id: entity(&account, EntityKind::Conversation, "room-1"),
            body: MessageBody::plain("different payload"),
            attachments: Vec::new(),
            relation: None,
        },
        ..request.clone()
    };
    assert_eq!(
        store.enqueue_action(different_request).await.unwrap_err(),
        StoreError::OperationConflict
    );
}

#[tokio::test]
async fn removing_account_deletes_its_sync_and_outbox_data() {
    let directory = tempdir().unwrap();
    let store = open(directory.path().join("fr-chat.sqlite3"));
    let account = account();
    store.put_account(account_record(&account)).await.unwrap();
    store
        .commit_sync_batch(batch(
            &account,
            vec![ProviderChange::ConversationUpserted(conversation(&account))],
            1,
        ))
        .await
        .unwrap();
    store
        .enqueue_action(ActionRequest {
            account: account.clone(),
            operation_id: OpaqueId::new("operation-1").unwrap(),
            action: fr_chat_provider_api::ProviderAction::SendMessage {
                conversation_id: entity(&account, EntityKind::Conversation, "room-1"),
                body: MessageBody::plain("hello"),
                attachments: Vec::new(),
                relation: None,
            },
        })
        .await
        .unwrap();

    store.remove_account_and_data(&account).await.unwrap();
    assert!(store.account(&account).await.unwrap().is_none());
    assert!(
        store
            .conversations(&account, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(store.sync_cursor(&account).await.unwrap().is_none());
    assert!(store.queued_actions(&account, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn database_rejects_invalid_or_wrong_encryption_keys() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("encrypted.sqlite3");
    assert!(matches!(
        SqliteAppStore::open_encrypted(&path, SecretBytes::new(vec![1; 16])),
        Err(StoreError::InvalidData)
    ));
    let store = open(&path);
    store.put_account(account_record(&account())).await.unwrap();
    assert!(matches!(
        SqliteAppStore::open_encrypted(&path, SecretBytes::new(vec![0xa5; 32])),
        Err(StoreError::Internal | StoreError::Migration)
    ));
}
