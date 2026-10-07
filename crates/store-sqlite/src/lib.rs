#![forbid(unsafe_code)]

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use fr_chat_app_core::{AppStore, OutboxEntry, OutboxFailure, OutboxState, StoreError, SyncCommit};
use fr_chat_domain::{
    Account, AccountRef, Capability, CapabilityMap, CapabilityState, Conversation, EntityKind,
    EntityRef, Message, MessageBody, OpaqueId, Server, TimelineEvent, TimelinePayload,
};
use fr_chat_provider_api::{
    ActionOutcome, ActionRequest, ProviderChange, SecretBytes, SyncBatch, SyncCursor,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use zeroize::{Zeroize, Zeroizing};

const SCHEMA_VERSION: i64 = 1;

#[derive(Clone)]
pub struct SqliteAppStore {
    connection: Arc<Mutex<Connection>>,
    path: Arc<PathBuf>,
}

impl SqliteAppStore {
    pub fn open_encrypted(path: impl AsRef<Path>, key: SecretBytes) -> Result<Self, StoreError> {
        if key.expose().len() != 32 {
            return Err(StoreError::InvalidData);
        }
        let path = path.as_ref().to_path_buf();
        if path != Path::new(":memory:") {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).map_err(|_| StoreError::Unavailable)?;
            }
        }
        let mut connection = Connection::open(&path).map_err(map_sqlite_error)?;
        let mut hex_key = Zeroizing::new(String::with_capacity(64));
        for byte in key.expose() {
            use std::fmt::Write;
            write!(hex_key, "{byte:02x}").map_err(|_| StoreError::Internal)?;
        }
        let key_pragma = Zeroizing::new(format!("PRAGMA key = \"x'{}'\";", hex_key.as_str()));
        connection
            .execute_batch(key_pragma.as_str())
            .map_err(map_sqlite_error)?;
        hex_key.zeroize();
        let cipher_version: Option<String> = connection
            .pragma_query_value(None, "cipher_version", |row| row.get(0))
            .optional()
            .map_err(map_sqlite_error)?;
        if cipher_version.is_none() {
            return Err(StoreError::Migration);
        }
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(map_sqlite_error)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(map_sqlite_error)?;
        if path != Path::new(":memory:") {
            connection
                .pragma_update(None, "journal_mode", "WAL")
                .map_err(map_sqlite_error)?;
        }
        migrate(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Arc::new(path),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    async fn run<T, F>(&self, operation: F) -> Result<T, StoreError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, StoreError> + Send + 'static,
    {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || {
            let mut connection = connection.lock().map_err(|_| StoreError::Unavailable)?;
            operation(&mut connection)
        })
        .await
        .map_err(|_| StoreError::Unavailable)?
    }
}

fn migrate(connection: &mut Connection) -> Result<(), StoreError> {
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(map_sqlite_error)?;
    if version > SCHEMA_VERSION {
        return Err(StoreError::Migration);
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_sqlite_error)?;
    transaction
        .execute_batch(
            "CREATE TABLE accounts (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id)
            );
            CREATE TABLE servers (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, remote_id)
            );
            CREATE TABLE conversations (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                server_id TEXT,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, remote_id)
            );
            CREATE INDEX conversations_by_server
                ON conversations(provider_id, account_id, server_id);
            CREATE TABLE entities (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                remote_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, entity_type, remote_id)
            );
            CREATE TABLE timeline_events (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                received_sequence INTEGER NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, event_id)
            );
            CREATE INDEX timeline_by_conversation
                ON timeline_events(provider_id, account_id, conversation_id, received_sequence);
            CREATE TABLE messages (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, message_id)
            );
            CREATE INDEX messages_by_conversation
                ON messages(provider_id, account_id, conversation_id);
            CREATE TABLE sync_cursors (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id)
            );
            CREATE TABLE capabilities (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                capability TEXT NOT NULL,
                state TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, conversation_id, capability)
            );
            CREATE TABLE outbox (
                provider_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                status TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (provider_id, account_id, operation_id)
            );
            CREATE INDEX outbox_by_status
                ON outbox(provider_id, account_id, status, operation_id);
            PRAGMA user_version = 1;",
        )
        .map_err(map_sqlite_error)?;
    transaction.commit().map_err(map_sqlite_error)
}

fn map_sqlite_error(error: rusqlite::Error) -> StoreError {
    match error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            StoreError::Unavailable
        }
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::DatabaseLocked =>
        {
            StoreError::Unavailable
        }
        rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound,
        _ => StoreError::Internal,
    }
}

fn encode<T: Serialize>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|_| StoreError::InvalidData)
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, StoreError> {
    serde_json::from_str(value).map_err(|_| StoreError::InvalidData)
}

fn type_key(kind: &EntityKind) -> Result<String, StoreError> {
    encode(kind)
}

fn upsert_entity(
    connection: &Connection,
    table: &str,
    account: &AccountRef,
    remote_id: &str,
    payload: &str,
) -> Result<(), StoreError> {
    let sql = match table {
        "servers" => {
            "INSERT INTO servers(provider_id, account_id, remote_id, payload)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(provider_id, account_id, remote_id)
             DO UPDATE SET payload = excluded.payload"
        }
        "conversations" => {
            "INSERT INTO conversations(provider_id, account_id, remote_id, server_id, payload)
             VALUES (?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(provider_id, account_id, remote_id)
             DO UPDATE SET payload = excluded.payload, server_id = excluded.server_id"
        }
        _ => {
            "INSERT INTO entities(provider_id, account_id, entity_type, remote_id, payload)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(provider_id, account_id, entity_type, remote_id)
             DO UPDATE SET payload = excluded.payload"
        }
    };
    match table {
        "servers" => connection.execute(
            sql,
            params![
                account.provider_id.as_str(),
                account.account_id.as_str(),
                remote_id,
                payload
            ],
        ),
        "conversations" => {
            let conversation: Conversation = decode(payload)?;
            connection
                .execute(
                    sql,
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        remote_id,
                        payload,
                    ],
                )
                .map_err(map_sqlite_error)?;
            if let Some(server) = conversation.server_id {
                connection
                    .execute(
                        "UPDATE conversations SET server_id = ?1
                     WHERE provider_id = ?2 AND account_id = ?3 AND remote_id = ?4",
                        params![
                            server.remote_id.as_str(),
                            account.provider_id.as_str(),
                            account.account_id.as_str(),
                            remote_id,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            Ok(1)
        }
        _ => {
            let kind = match table {
                "users" => "user",
                "roles" => "role",
                "members" => "member",
                _ => return Err(StoreError::InvalidData),
            };
            connection.execute(
                sql,
                params![
                    account.provider_id.as_str(),
                    account.account_id.as_str(),
                    kind,
                    remote_id,
                    payload
                ],
            )
        }
    }
    .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_event(transaction: &Transaction<'_>, event: &TimelineEvent) -> Result<bool, StoreError> {
    let account = &event.id.account;
    let already_present: bool = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM timeline_events
                WHERE provider_id = ?1 AND account_id = ?2 AND event_id = ?3
             )",
            params![
                account.provider_id.as_str(),
                account.account_id.as_str(),
                event.id.remote_id.as_str(),
            ],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)?;
    if already_present {
        return Ok(false);
    }
    let previous_sequence: Option<i64> = transaction
        .query_row(
            "SELECT MAX(received_sequence) FROM timeline_events
             WHERE provider_id = ?1 AND account_id = ?2 AND conversation_id = ?3",
            params![
                account.provider_id.as_str(),
                account.account_id.as_str(),
                event.conversation_id.remote_id.as_str(),
            ],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)?;
    let sequence = previous_sequence
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(StoreError::InvalidData)?;
    let inserted = transaction
        .execute(
            "INSERT OR IGNORE INTO timeline_events
             (provider_id, account_id, conversation_id, event_id, received_sequence, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                account.provider_id.as_str(),
                account.account_id.as_str(),
                event.conversation_id.remote_id.as_str(),
                event.id.remote_id.as_str(),
                sequence,
                encode(event)?,
            ],
        )
        .map_err(map_sqlite_error)?;
    if inserted == 0 {
        return Ok(false);
    }

    match &event.payload {
        TimelinePayload::MessageCreated { message } => {
            transaction
                .execute(
                    "INSERT INTO messages(provider_id, account_id, message_id, conversation_id, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(provider_id, account_id, message_id)
                     DO UPDATE SET conversation_id = excluded.conversation_id, payload = excluded.payload",
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        message.id.remote_id.as_str(),
                        event.conversation_id.remote_id.as_str(),
                        encode(message.as_ref())?,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
        TimelinePayload::MessageEdited {
            message_id,
            body,
            edited_at_ms,
        } => {
            update_message(transaction, account, message_id, |message| {
                message.body = body.clone();
                message.edited_at_ms = Some(*edited_at_ms);
            })?;
        }
        TimelinePayload::MessageRedacted { message_id, .. } => {
            update_message(transaction, account, message_id, |message| {
                message.body = MessageBody::plain("");
                message.attachments.clear();
            })?;
            redact_message_events(transaction, account, &event.conversation_id, message_id)?;
        }
        _ => {}
    }
    Ok(true)
}

fn redact_message_events(
    transaction: &Transaction<'_>,
    account: &AccountRef,
    conversation: &EntityRef,
    message_id: &EntityRef,
) -> Result<(), StoreError> {
    let events = {
        let mut statement = transaction
            .prepare(
                "SELECT event_id, payload FROM timeline_events
                 WHERE provider_id = ?1 AND account_id = ?2 AND conversation_id = ?3",
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(
                params![
                    account.provider_id.as_str(),
                    account.account_id.as_str(),
                    conversation.remote_id.as_str(),
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(map_sqlite_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite_error)?
    };

    for (event_id, payload) in events {
        let mut event: TimelineEvent = decode(&payload)?;
        let changed = match &mut event.payload {
            TimelinePayload::MessageCreated { message } if message.id == *message_id => {
                message.body = MessageBody::plain("");
                message.attachments.clear();
                true
            }
            TimelinePayload::MessageEdited {
                message_id: edited_id,
                body,
                ..
            } if edited_id == message_id => {
                *body = MessageBody::plain("");
                true
            }
            _ => false,
        };
        if changed {
            transaction
                .execute(
                    "UPDATE timeline_events SET payload = ?1
                     WHERE provider_id = ?2 AND account_id = ?3 AND event_id = ?4",
                    params![
                        encode(&event)?,
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        event_id,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

fn update_message(
    transaction: &Transaction<'_>,
    account: &AccountRef,
    id: &EntityRef,
    update: impl FnOnce(&mut Message),
) -> Result<(), StoreError> {
    let payload: Option<String> = transaction
        .query_row(
            "SELECT payload FROM messages
             WHERE provider_id = ?1 AND account_id = ?2 AND message_id = ?3",
            params![
                account.provider_id.as_str(),
                account.account_id.as_str(),
                id.remote_id.as_str(),
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(payload) = payload else {
        return Ok(());
    };
    let mut message: Message = decode(&payload)?;
    update(&mut message);
    transaction
        .execute(
            "UPDATE messages SET payload = ?1
             WHERE provider_id = ?2 AND account_id = ?3 AND message_id = ?4",
            params![
                encode(&message)?,
                account.provider_id.as_str(),
                account.account_id.as_str(),
                id.remote_id.as_str(),
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn decode_outbox(payload: &str) -> Result<OutboxEntry, StoreError> {
    decode(payload)
}

fn outbox_status(state: &OutboxState) -> &'static str {
    match state {
        OutboxState::Queued => "queued",
        OutboxState::InFlight => "in_flight",
        OutboxState::Accepted => "accepted",
        OutboxState::Failed { retryable: true } => "failed_retryable",
        OutboxState::Failed { retryable: false } => "failed_permanent",
    }
}

fn read_outbox(
    connection: &Connection,
    account: &AccountRef,
    operation_id: &OpaqueId,
) -> Result<Option<OutboxEntry>, StoreError> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM outbox
             WHERE provider_id = ?1 AND account_id = ?2 AND operation_id = ?3",
            params![
                account.provider_id.as_str(),
                account.account_id.as_str(),
                operation_id.as_str(),
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sqlite_error)?;
    payload.as_deref().map(decode_outbox).transpose()
}

#[async_trait]
impl AppStore for SqliteAppStore {
    async fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        self.run(|connection| {
            let mut statement = connection
                .prepare("SELECT payload FROM accounts ORDER BY provider_id, account_id")
                .map_err(map_sqlite_error)?;
            let payloads = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_sqlite_error)?;
            payloads
                .map(|payload| decode(&payload.map_err(map_sqlite_error)?))
                .collect()
        })
        .await
    }

    async fn account(&self, account: &AccountRef) -> Result<Option<Account>, StoreError> {
        let account = account.clone();
        self.run(move |connection| {
            let payload: Option<String> = connection
                .query_row(
                    "SELECT payload FROM accounts WHERE provider_id = ?1 AND account_id = ?2",
                    params![account.provider_id.as_str(), account.account_id.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            payload.as_deref().map(decode).transpose()
        })
        .await
    }

    async fn put_account(&self, account: Account) -> Result<(), StoreError> {
        self.run(move |connection| {
            connection
                .execute(
                    "INSERT INTO accounts(provider_id, account_id, payload) VALUES (?1, ?2, ?3)
                     ON CONFLICT(provider_id, account_id) DO UPDATE SET payload = excluded.payload",
                    params![
                        account.id.provider_id.as_str(),
                        account.id.account_id.as_str(),
                        encode(&account)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            Ok(())
        })
        .await
    }

    async fn remove_account_and_data(&self, account: &AccountRef) -> Result<(), StoreError> {
        let account = account.clone();
        self.run(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_sqlite_error)?;
            let deleted = transaction
                .execute(
                    "DELETE FROM accounts WHERE provider_id = ?1 AND account_id = ?2",
                    params![account.provider_id.as_str(), account.account_id.as_str()],
                )
                .map_err(map_sqlite_error)?;
            if deleted == 0 {
                return Err(StoreError::NotFound);
            }
            for table in [
                "servers",
                "conversations",
                "entities",
                "timeline_events",
                "messages",
                "sync_cursors",
                "capabilities",
                "outbox",
            ] {
                transaction
                    .execute(
                        &format!("DELETE FROM {table} WHERE provider_id = ?1 AND account_id = ?2"),
                        params![account.provider_id.as_str(), account.account_id.as_str()],
                    )
                    .map_err(map_sqlite_error)?;
            }
            transaction.commit().map_err(map_sqlite_error)
        })
        .await
    }

    async fn servers(&self, account: &AccountRef) -> Result<Vec<Server>, StoreError> {
        let account = account.clone();
        self.run(move |connection| {
            let mut statement = connection
                .prepare("SELECT payload FROM servers WHERE provider_id = ?1 AND account_id = ?2 ORDER BY remote_id")
                .map_err(map_sqlite_error)?;
            let payloads = statement
                .query_map(
                    params![account.provider_id.as_str(), account.account_id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(map_sqlite_error)?;
            payloads
                .map(|payload| decode(&payload.map_err(map_sqlite_error)?))
                .collect()
        })
        .await
    }

    async fn conversations(
        &self,
        account: &AccountRef,
        server: Option<&EntityRef>,
    ) -> Result<Vec<Conversation>, StoreError> {
        let account = account.clone();
        let server_id = server.map(|server| server.remote_id.to_string());
        self.run(move |connection| {
            let mut statement = connection
                .prepare(
                    "SELECT payload FROM conversations
                     WHERE provider_id = ?1 AND account_id = ?2
                       AND (?3 IS NULL OR server_id = ?3)
                     ORDER BY COALESCE(server_id, ''), remote_id",
                )
                .map_err(map_sqlite_error)?;
            let payloads = statement
                .query_map(
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        server_id,
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(map_sqlite_error)?;
            payloads
                .map(|payload| decode(&payload.map_err(map_sqlite_error)?))
                .collect()
        })
        .await
    }

    async fn timeline(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
        before: Option<&EntityRef>,
        limit: u16,
    ) -> Result<Vec<TimelineEvent>, StoreError> {
        let account = account.clone();
        let conversation_id = conversation.remote_id.to_string();
        let before = before.cloned();
        self.run(move |connection| {
            let before_sequence = if let Some(before) = before {
                connection
                    .query_row(
                        "SELECT received_sequence FROM timeline_events
                                                 WHERE provider_id = ?1 AND account_id = ?2
                                                     AND conversation_id = ?3 AND event_id = ?4",
                        params![
                            account.provider_id.as_str(),
                            account.account_id.as_str(),
                            conversation_id,
                            before.remote_id.as_str(),
                        ],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(map_sqlite_error)?
                    .ok_or(StoreError::NotFound)?
            } else {
                i64::MAX
            };
            let mut statement = connection
                .prepare(
                    "SELECT payload FROM timeline_events
                     WHERE provider_id = ?1 AND account_id = ?2 AND conversation_id = ?3
                       AND received_sequence < ?4
                     ORDER BY received_sequence DESC, event_id DESC LIMIT ?5",
                )
                .map_err(map_sqlite_error)?;
            let payloads = statement
                .query_map(
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        conversation_id,
                        before_sequence,
                        i64::from(limit),
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(map_sqlite_error)?;
            let mut events: Vec<TimelineEvent> = payloads
                .map(|payload| decode(&payload.map_err(map_sqlite_error)?))
                .collect::<Result<_, StoreError>>()?;
            events.reverse();
            Ok(events)
        })
        .await
    }

    async fn message(
        &self,
        account: &AccountRef,
        id: &EntityRef,
    ) -> Result<Option<Message>, StoreError> {
        let account = account.clone();
        let id = id.remote_id.to_string();
        self.run(move |connection| {
            let payload: Option<String> = connection
                .query_row(
                    "SELECT payload FROM messages WHERE provider_id = ?1 AND account_id = ?2 AND message_id = ?3",
                    params![account.provider_id.as_str(), account.account_id.as_str(), id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            payload.as_deref().map(decode).transpose()
        })
        .await
    }

    async fn capabilities(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
    ) -> Result<CapabilityMap, StoreError> {
        let account = account.clone();
        let conversation_id = conversation.remote_id.to_string();
        self.run(move |connection| {
            let mut statement = connection
                .prepare(
                    "SELECT capability, state FROM capabilities
                     WHERE provider_id = ?1 AND account_id = ?2 AND conversation_id = ?3",
                )
                .map_err(map_sqlite_error)?;
            let rows = statement
                .query_map(
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        conversation_id,
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(map_sqlite_error)?;
            let mut capabilities = CapabilityMap::default();
            for row in rows {
                let (capability, state) = row.map_err(map_sqlite_error)?;
                capabilities.set(decode(&capability)?, decode(&state)?);
            }
            Ok(capabilities)
        })
        .await
    }

    async fn sync_cursor(&self, account: &AccountRef) -> Result<Option<SyncCursor>, StoreError> {
        let account = account.clone();
        self.run(move |connection| {
            let payload: Option<String> = connection
                .query_row(
                    "SELECT payload FROM sync_cursors WHERE provider_id = ?1 AND account_id = ?2",
                    params![account.provider_id.as_str(), account.account_id.as_str()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            payload.as_deref().map(decode).transpose()
        })
        .await
    }

    async fn commit_sync_batch(&self, batch: SyncBatch) -> Result<SyncCommit, StoreError> {
        batch.validate().map_err(|_| StoreError::InvalidData)?;
        self.run(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_sqlite_error)?;
            let mut commit = SyncCommit::default();
            for change in &batch.changes {
                match change {
                    ProviderChange::AccountUpdated(account) => {
                        transaction
                            .execute(
                                "INSERT INTO accounts(provider_id, account_id, payload) VALUES (?1, ?2, ?3)
                                 ON CONFLICT(provider_id, account_id) DO UPDATE SET payload = excluded.payload",
                                params![
                                    account.id.provider_id.as_str(),
                                    account.id.account_id.as_str(),
                                    encode(account)?,
                                ],
                            )
                            .map_err(map_sqlite_error)?;
                    }
                    ProviderChange::ServerUpserted(server) => upsert_entity(
                        &transaction,
                        "servers",
                        &batch.account,
                        server.id.remote_id.as_str(),
                        &encode(server)?,
                    )?,
                    ProviderChange::ConversationUpserted(conversation) => {
                        upsert_entity(
                            &transaction,
                            "conversations",
                            &batch.account,
                            conversation.id.remote_id.as_str(),
                            &encode(conversation)?,
                        )?;
                        commit.updated_conversations.push(conversation.id.clone());
                    }
                    ProviderChange::UserUpserted(user) => upsert_entity(
                        &transaction,
                        "users",
                        &batch.account,
                        user.id.remote_id.as_str(),
                        &encode(user)?,
                    )?,
                    ProviderChange::RoleUpserted(role) => upsert_entity(
                        &transaction,
                        "roles",
                        &batch.account,
                        role.id.remote_id.as_str(),
                        &encode(role)?,
                    )?,
                    ProviderChange::MemberUpserted(member) => {
                        let key = format!(
                            "{}:{}:{}",
                            member.user_id.remote_id,
                            member.server_id.as_ref().map_or("", |server| server.remote_id.as_str()),
                            member.conversation_id.as_ref().map_or("", |conversation| conversation.remote_id.as_str()),
                        );
                        upsert_entity(
                            &transaction,
                            "members",
                            &batch.account,
                            &key,
                            &encode(member)?,
                        )?;
                    }
                    ProviderChange::TimelineEvent(event) => {
                        if insert_event(&transaction, event)? {
                            commit.new_events += 1;
                            if !commit.updated_conversations.contains(&event.conversation_id) {
                                commit.updated_conversations.push(event.conversation_id.clone());
                            }
                        }
                    }
                    ProviderChange::EntityRemoved(entity) => {
                        remove_entity(&transaction, entity)?;
                    }
                }
                commit.applied_change_count += 1;
            }
            if let Some(cursor) = &batch.next_cursor {
                transaction
                    .execute(
                        "INSERT INTO sync_cursors(provider_id, account_id, payload) VALUES (?1, ?2, ?3)
                         ON CONFLICT(provider_id, account_id) DO UPDATE SET payload = excluded.payload",
                        params![
                            batch.account.provider_id.as_str(),
                            batch.account.account_id.as_str(),
                            encode(cursor)?,
                        ],
                    )
                    .map_err(map_sqlite_error)?;
            }
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(commit)
        })
        .await
    }

    async fn enqueue_action(&self, request: ActionRequest) -> Result<OutboxEntry, StoreError> {
        self.run(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_sqlite_error)?;
            let account = &request.account;
            if let Some(existing) = read_outbox(&transaction, account, &request.operation_id)? {
                if existing.request != request {
                    return Err(StoreError::OperationConflict);
                }
                transaction.commit().map_err(map_sqlite_error)?;
                return Ok(existing);
            }
            let entry = OutboxEntry {
                request: request.clone(),
                state: OutboxState::Queued,
                outcome: None,
                failure: None,
            };
            transaction
                .execute(
                    "INSERT INTO outbox(provider_id, account_id, operation_id, status, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        request.operation_id.as_str(),
                        outbox_status(&entry.state),
                        encode(&entry)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(entry)
        })
        .await
    }

    async fn claim_action(
        &self,
        account: &AccountRef,
        operation_id: &OpaqueId,
    ) -> Result<bool, StoreError> {
        let account = account.clone();
        let operation_id = operation_id.clone();
        self.run(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_sqlite_error)?;
            let Some(mut entry) = read_outbox(&transaction, &account, &operation_id)? else {
                return Err(StoreError::NotFound);
            };
            if !matches!(
                entry.state,
                OutboxState::Queued | OutboxState::Failed { retryable: true }
            ) {
                transaction.commit().map_err(map_sqlite_error)?;
                return Ok(false);
            }
            entry.state = OutboxState::InFlight;
            entry.failure = None;
            transaction
                .execute(
                    "UPDATE outbox SET status = ?1, payload = ?2
                     WHERE provider_id = ?3 AND account_id = ?4 AND operation_id = ?5",
                    params![
                        outbox_status(&entry.state),
                        encode(&entry)?,
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        operation_id.as_str(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(true)
        })
        .await
    }

    async fn complete_action(
        &self,
        account: &AccountRef,
        operation_id: &OpaqueId,
        outcome: ActionOutcome,
    ) -> Result<OutboxEntry, StoreError> {
        let account = account.clone();
        let operation_id = operation_id.clone();
        self.run(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_sqlite_error)?;
            let mut entry =
                read_outbox(&transaction, &account, &operation_id)?.ok_or(StoreError::NotFound)?;
            if !matches!(entry.state, OutboxState::InFlight) {
                return Err(StoreError::OperationConflict);
            }
            entry.state = OutboxState::Accepted;
            entry.outcome = Some(outcome);
            entry.failure = None;
            transaction
                .execute(
                    "UPDATE outbox SET status = ?1, payload = ?2
                     WHERE provider_id = ?3 AND account_id = ?4 AND operation_id = ?5",
                    params![
                        outbox_status(&entry.state),
                        encode(&entry)?,
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        operation_id.as_str(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(entry)
        })
        .await
    }

    async fn fail_action(
        &self,
        account: &AccountRef,
        operation_id: &OpaqueId,
        failure: OutboxFailure,
        retryable: bool,
    ) -> Result<OutboxEntry, StoreError> {
        let account = account.clone();
        let operation_id = operation_id.clone();
        self.run(move |connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(map_sqlite_error)?;
            let mut entry =
                read_outbox(&transaction, &account, &operation_id)?.ok_or(StoreError::NotFound)?;
            if !matches!(entry.state, OutboxState::InFlight) {
                return Err(StoreError::OperationConflict);
            }
            entry.state = OutboxState::Failed { retryable };
            entry.failure = Some(failure);
            transaction
                .execute(
                    "UPDATE outbox SET status = ?1, payload = ?2
                     WHERE provider_id = ?3 AND account_id = ?4 AND operation_id = ?5",
                    params![
                        outbox_status(&entry.state),
                        encode(&entry)?,
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        operation_id.as_str(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(entry)
        })
        .await
    }

    async fn action(
        &self,
        account: &AccountRef,
        operation_id: &OpaqueId,
    ) -> Result<Option<OutboxEntry>, StoreError> {
        let account = account.clone();
        let operation_id = operation_id.clone();
        self.run(move |connection| read_outbox(connection, &account, &operation_id))
            .await
    }

    async fn queued_actions(
        &self,
        account: &AccountRef,
        limit: u16,
    ) -> Result<Vec<OutboxEntry>, StoreError> {
        let account = account.clone();
        self.run(move |connection| {
            let mut statement = connection
                .prepare(
                    "SELECT payload FROM outbox
                     WHERE provider_id = ?1 AND account_id = ?2
                       AND status IN ('queued', 'failed_retryable')
                     ORDER BY rowid LIMIT ?3",
                )
                .map_err(map_sqlite_error)?;
            let payloads = statement
                .query_map(
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        i64::from(limit),
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(map_sqlite_error)?;
            payloads
                .map(|payload| decode_outbox(&payload.map_err(map_sqlite_error)?))
                .collect()
        })
        .await
    }

    async fn set_capability(
        &self,
        account: &AccountRef,
        conversation: &EntityRef,
        capability: Capability,
        state: CapabilityState,
    ) -> Result<(), StoreError> {
        let account = account.clone();
        let conversation_id = conversation.remote_id.to_string();
        self.run(move |connection| {
            connection
                .execute(
                    "INSERT INTO capabilities(provider_id, account_id, conversation_id, capability, state)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(provider_id, account_id, conversation_id, capability)
                     DO UPDATE SET state = excluded.state",
                    params![
                        account.provider_id.as_str(),
                        account.account_id.as_str(),
                        conversation_id,
                        encode(&capability)?,
                        encode(&state)?,
                    ],
                )
                .map_err(map_sqlite_error)?;
            Ok(())
        })
        .await
    }
}

fn remove_entity(transaction: &Transaction<'_>, entity: &EntityRef) -> Result<(), StoreError> {
    let (table, id_column) = match &entity.kind {
        EntityKind::Server => ("servers", "remote_id"),
        EntityKind::Conversation => ("conversations", "remote_id"),
        EntityKind::Message => ("messages", "message_id"),
        EntityKind::Event => ("timeline_events", "event_id"),
        _ => {
            transaction
                .execute(
                    "DELETE FROM entities WHERE provider_id = ?1 AND account_id = ?2
                     AND entity_type = ?3 AND remote_id = ?4",
                    params![
                        entity.account.provider_id.as_str(),
                        entity.account.account_id.as_str(),
                        type_key(&entity.kind)?,
                        entity.remote_id.as_str(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            return Ok(());
        }
    };
    let sql = format!(
        "DELETE FROM {table} WHERE provider_id = ?1 AND account_id = ?2 AND {id_column} = ?3"
    );
    transaction
        .execute(
            &sql,
            params![
                entity.account.provider_id.as_str(),
                entity.account.account_id.as_str(),
                entity.remote_id.as_str(),
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
