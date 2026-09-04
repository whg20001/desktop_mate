use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::ai::provider::ChatMessage;

use super::{
    model::{
        ApprovedMemoryEvent, ConversationEvent, MemoryEntry, MemoryExport, MemoryId, MemoryKind,
        MemoryListFilter, MemoryScope,
    },
    provider::{MemoryError, MemoryResult, SessionStore},
};

pub struct SqliteSessionStore {
    connection: Mutex<Connection>,
}

impl SqliteSessionStore {
    pub fn open(database_path: impl AsRef<Path>) -> MemoryResult<Self> {
        let database_path = database_path.as_ref();
        ensure_local_storage_path(database_path)?;
        let parent = database_path
            .parent()
            .ok_or_else(|| MemoryError::Storage("database path has no parent directory".into()))?;
        fs::create_dir_all(parent).map_err(storage_error)?;

        let connection = Connection::open(&database_path).map_err(storage_error)?;
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 3000;

                CREATE TABLE IF NOT EXISTS conversation_events (
                    id TEXT PRIMARY KEY,
                    user_id TEXT NOT NULL,
                    character_id TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at_unix_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_conversation_scope_time
                    ON conversation_events(
                        user_id, character_id, session_id, created_at_unix_ms
                    );

                CREATE TABLE IF NOT EXISTS approved_memory_events (
                    id TEXT PRIMARY KEY,
                    user_id TEXT NOT NULL,
                    character_id TEXT NOT NULL,
                    session_id TEXT,
                    kind TEXT NOT NULL,
                    content TEXT NOT NULL,
                    importance REAL NOT NULL,
                    tags_json TEXT NOT NULL,
                    occurred_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    source_event_ids_json TEXT NOT NULL,
                    metadata_json TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_approved_scope_time
                    ON approved_memory_events(
                        user_id, character_id, session_id, created_at_unix_ms
                    );

                CREATE TABLE IF NOT EXISTS provider_memory_mappings (
                    local_memory_id TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    provider_memory_id TEXT NOT NULL,
                    PRIMARY KEY(local_memory_id, provider_id),
                    FOREIGN KEY(local_memory_id)
                        REFERENCES approved_memory_events(id) ON DELETE CASCADE
                );
                ",
            )
            .map_err(storage_error)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn conversation_events(&self, scope: &MemoryScope) -> MemoryResult<Vec<ConversationEvent>> {
        scope.validate().map_err(MemoryError::Storage)?;
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT id, user_id, character_id, session_id, role, content,
                        created_at_unix_ms
                 FROM conversation_events
                 WHERE user_id = ?1 AND character_id = ?2
                   AND (?3 IS NULL OR session_id = ?3)
                 ORDER BY created_at_unix_ms ASC, rowid ASC",
            )
            .map_err(storage_error)?;
        let events = statement
            .query_map(
                params![scope.user_id, scope.character_id, scope.session_id],
                conversation_from_row,
            )
            .map_err(storage_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage_error)?;
        Ok(events)
    }
}

#[async_trait]
impl SessionStore for SqliteSessionStore {
    async fn append_turn(
        &self,
        scope: &MemoryScope,
        messages: Vec<ChatMessage>,
    ) -> MemoryResult<()> {
        let session_id = scope.require_session_id().map_err(MemoryError::Storage)?;
        for message in &messages {
            validate_message(message)?;
        }

        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(storage_error)?;
        let timestamp = now_unix_ms();
        for (index, message) in messages.into_iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO conversation_events(
                        id, user_id, character_id, session_id, role, content,
                        created_at_unix_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        Uuid::new_v4().to_string(),
                        scope.user_id,
                        scope.character_id,
                        session_id,
                        message.role,
                        message.content,
                        timestamp.saturating_add(index as i64),
                    ],
                )
                .map_err(storage_error)?;
        }
        transaction.commit().map_err(storage_error)
    }

    async fn get_recent(
        &self,
        scope: &MemoryScope,
        limit: usize,
    ) -> MemoryResult<Vec<ChatMessage>> {
        let session_id = scope.require_session_id().map_err(MemoryError::Storage)?;
        if limit == 0 {
            return Ok(Vec::new());
        }
        let connection = self.connection.lock();
        let mut statement = connection
            .prepare(
                "SELECT role, content
                 FROM conversation_events
                 WHERE user_id = ?1 AND character_id = ?2 AND session_id = ?3
                 ORDER BY created_at_unix_ms DESC, rowid DESC
                 LIMIT ?4",
            )
            .map_err(storage_error)?;
        let rows = statement
            .query_map(
                params![
                    scope.user_id,
                    scope.character_id,
                    session_id,
                    limit.min(500) as i64
                ],
                |row| {
                    Ok(ChatMessage {
                        role: row.get(0)?,
                        content: row.get(1)?,
                    })
                },
            )
            .map_err(storage_error)?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(storage_error)?);
        }
        messages.reverse();
        Ok(messages)
    }

    async fn clear_scope(&self, scope: &MemoryScope) -> MemoryResult<()> {
        scope.validate().map_err(MemoryError::Storage)?;
        let mut connection = self.connection.lock();
        let transaction = connection.transaction().map_err(storage_error)?;
        transaction
            .execute(
                "DELETE FROM conversation_events
                 WHERE user_id = ?1 AND character_id = ?2
                   AND (?3 IS NULL OR session_id = ?3)",
                params![scope.user_id, scope.character_id, scope.session_id],
            )
            .map_err(storage_error)?;
        transaction
            .execute(
                "DELETE FROM approved_memory_events
                 WHERE user_id = ?1 AND character_id = ?2
                   AND (?3 IS NULL OR session_id = ?3)",
                params![scope.user_id, scope.character_id, scope.session_id],
            )
            .map_err(storage_error)?;
        transaction.commit().map_err(storage_error)
    }

    async fn export_scope(&self, scope: &MemoryScope) -> MemoryResult<MemoryExport> {
        let conversation_events = self.conversation_events(scope)?;
        let approved_memories = self
            .list_approved(
                scope,
                &MemoryListFilter {
                    kinds: Vec::new(),
                    limit: 10_000,
                },
            )
            .await?;
        Ok(MemoryExport {
            scope: scope.clone(),
            conversation_events,
            approved_memories,
            exported_at_unix_ms: now_unix_ms(),
        })
    }

    async fn save_approved(&self, event: ApprovedMemoryEvent) -> MemoryResult<()> {
        event.entry.scope.validate().map_err(MemoryError::Storage)?;
        let tags = serde_json::to_string(&event.entry.tags).map_err(storage_error)?;
        let sources =
            serde_json::to_string(&event.entry.source_event_ids).map_err(storage_error)?;
        let metadata = serde_json::to_string(&event.entry.metadata).map_err(storage_error)?;
        self.connection
            .lock()
            .execute(
                "INSERT INTO approved_memory_events(
                    id, user_id, character_id, session_id, kind, content, importance,
                    tags_json, occurred_at_unix_ms, created_at_unix_ms,
                    source_event_ids_json, metadata_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    event.id,
                    event.entry.scope.user_id,
                    event.entry.scope.character_id,
                    event.entry.scope.session_id,
                    kind_to_str(event.entry.kind),
                    event.entry.content,
                    event.entry.importance,
                    tags,
                    event.entry.occurred_at_unix_ms,
                    event.entry.created_at_unix_ms,
                    sources,
                    metadata,
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    async fn list_approved(
        &self,
        scope: &MemoryScope,
        filter: &MemoryListFilter,
    ) -> MemoryResult<Vec<ApprovedMemoryEvent>> {
        scope.validate().map_err(MemoryError::Storage)?;
        let connection = self.connection.lock();
        let fetch_limit = if filter.kinds.is_empty() {
            filter.limit.clamp(1, 10_000)
        } else {
            10_000
        };
        let mut statement = connection
            .prepare(
                "SELECT id, user_id, character_id, session_id, kind, content,
                        importance, tags_json, occurred_at_unix_ms,
                        created_at_unix_ms, source_event_ids_json, metadata_json
                 FROM approved_memory_events
                 WHERE user_id = ?1 AND character_id = ?2
                   AND (?3 IS NULL OR session_id = ?3)
                 ORDER BY created_at_unix_ms DESC, rowid DESC
                 LIMIT ?4",
            )
            .map_err(storage_error)?;
        let mut memories = statement
            .query_map(
                params![
                    scope.user_id,
                    scope.character_id,
                    scope.session_id,
                    fetch_limit as i64
                ],
                approved_from_row,
            )
            .map_err(storage_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage_error)?;
        if !filter.kinds.is_empty() {
            memories.retain(|event| filter.kinds.contains(&event.entry.kind));
        }
        memories.truncate(filter.limit);
        Ok(memories)
    }

    async fn remove_approved(&self, scope: &MemoryScope, id: &MemoryId) -> MemoryResult<bool> {
        scope.validate().map_err(MemoryError::Storage)?;
        let changed = self
            .connection
            .lock()
            .execute(
                "DELETE FROM approved_memory_events
                 WHERE id = ?1 AND user_id = ?2 AND character_id = ?3
                   AND (?4 IS NULL OR session_id = ?4)",
                params![id, scope.user_id, scope.character_id, scope.session_id],
            )
            .map_err(storage_error)?;
        Ok(changed > 0)
    }

    async fn save_provider_mapping(
        &self,
        local_id: &MemoryId,
        provider_id: &str,
        provider_memory_id: &MemoryId,
    ) -> MemoryResult<()> {
        self.connection
            .lock()
            .execute(
                "INSERT INTO provider_memory_mappings(
                    local_memory_id, provider_id, provider_memory_id
                 ) VALUES (?1, ?2, ?3)
                 ON CONFLICT(local_memory_id, provider_id)
                 DO UPDATE SET provider_memory_id = excluded.provider_memory_id",
                params![local_id, provider_id, provider_memory_id],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    async fn provider_mapping(
        &self,
        local_id: &MemoryId,
        provider_id: &str,
    ) -> MemoryResult<Option<MemoryId>> {
        self.connection
            .lock()
            .query_row(
                "SELECT provider_memory_id FROM provider_memory_mappings
                 WHERE local_memory_id = ?1 AND provider_id = ?2",
                params![local_id, provider_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(storage_error)
    }
}

fn conversation_from_row(row: &Row<'_>) -> rusqlite::Result<ConversationEvent> {
    Ok(ConversationEvent {
        id: row.get(0)?,
        scope: MemoryScope {
            user_id: row.get(1)?,
            character_id: row.get(2)?,
            session_id: Some(row.get(3)?),
        },
        role: row.get(4)?,
        content: row.get(5)?,
        created_at_unix_ms: row.get(6)?,
    })
}

fn approved_from_row(row: &Row<'_>) -> rusqlite::Result<ApprovedMemoryEvent> {
    let kind: String = row.get(4)?;
    let tags_json: String = row.get(7)?;
    let source_json: String = row.get(10)?;
    let metadata_json: String = row.get(11)?;
    Ok(ApprovedMemoryEvent {
        id: row.get(0)?,
        entry: MemoryEntry {
            scope: MemoryScope {
                user_id: row.get(1)?,
                character_id: row.get(2)?,
                session_id: row.get(3)?,
            },
            kind: str_to_kind(&kind)?,
            content: row.get(5)?,
            importance: row.get(6)?,
            tags: serde_json::from_str(&tags_json).map_err(json_from_sql_error)?,
            occurred_at_unix_ms: row.get(8)?,
            created_at_unix_ms: row.get(9)?,
            source_event_ids: serde_json::from_str(&source_json).map_err(json_from_sql_error)?,
            metadata: serde_json::from_str(&metadata_json).map_err(json_from_sql_error)?,
        },
    })
}

fn kind_to_str(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Semantic => "semantic",
        MemoryKind::Episodic => "episodic",
    }
}

fn str_to_kind(value: &str) -> rusqlite::Result<MemoryKind> {
    match value {
        "semantic" => Ok(MemoryKind::Semantic),
        "episodic" => Ok(MemoryKind::Episodic),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            format!("unknown memory kind: {value}").into(),
        )),
    }
}

fn json_from_sql_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn validate_message(message: &ChatMessage) -> MemoryResult<()> {
    if message.role.trim().is_empty() || message.role.len() > 32 {
        return Err(MemoryError::Storage("message role is invalid".into()));
    }
    if message.content.trim().is_empty() || message.content.len() > 64_000 {
        return Err(MemoryError::Storage("message content is invalid".into()));
    }
    Ok(())
}

fn storage_error(error: impl std::fmt::Display) -> MemoryError {
    MemoryError::Storage(error.to_string())
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn ensure_local_storage_path(path: &Path) -> MemoryResult<()> {
    use std::path::{Component, Prefix};

    if path.has_root() && !matches!(path.components().next(), Some(Component::Prefix(_))) {
        return Err(MemoryError::LocalityViolation(
            "root-relative memory paths are not allowed".into(),
        ));
    }
    if matches!(
        path.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _))
    ) {
        return Err(MemoryError::LocalityViolation(
            "UNC and network share paths cannot store memory data".into(),
        ));
    }

    #[cfg(windows)]
    {
        use windows::{
            core::PCWSTR,
            Win32::{Storage::FileSystem::GetDriveTypeW, System::WindowsProgramming::DRIVE_REMOTE},
        };

        let display = path.to_string_lossy();
        let bytes = display.as_bytes();
        if bytes.len() >= 3 && bytes[1] == b':' {
            let root = format!(
                "{}:{}",
                display.chars().next().unwrap(),
                std::path::MAIN_SEPARATOR
            );
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
            if drive_type == DRIVE_REMOTE {
                return Err(MemoryError::LocalityViolation(
                    "mapped network drives cannot store memory data".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(user: &str, character: &str, session: &str) -> MemoryScope {
        MemoryScope {
            user_id: user.into(),
            character_id: character.into(),
            session_id: Some(session.into()),
        }
    }

    fn store() -> SqliteSessionStore {
        let path = std::env::temp_dir().join(format!(
            "desktop-companion-memory-{}.sqlite3",
            Uuid::new_v4()
        ));
        SqliteSessionStore::open(path).unwrap()
    }

    #[test]
    fn recent_messages_are_chronological_and_scoped() {
        tauri::async_runtime::block_on(async {
            let store = store();
            let first = scope("u1", "c1", "s1");
            let second = scope("u1", "c1", "s2");
            store
                .append_turn(
                    &first,
                    vec![
                        ChatMessage::new("user", "one"),
                        ChatMessage::new("assistant", "two"),
                        ChatMessage::new("user", "three"),
                    ],
                )
                .await
                .unwrap();
            store
                .append_turn(&second, vec![ChatMessage::new("user", "other")])
                .await
                .unwrap();

            let recent = store.get_recent(&first, 2).await.unwrap();
            assert_eq!(
                recent
                    .iter()
                    .map(|message| message.content.as_str())
                    .collect::<Vec<_>>(),
                vec!["two", "three"]
            );
        });
    }

    #[test]
    fn clearing_one_session_preserves_other_scopes() {
        tauri::async_runtime::block_on(async {
            let store = store();
            let first = scope("u1", "c1", "s1");
            let second = scope("u1", "c1", "s2");
            store
                .append_turn(&first, vec![ChatMessage::new("user", "first")])
                .await
                .unwrap();
            store
                .append_turn(&second, vec![ChatMessage::new("user", "second")])
                .await
                .unwrap();

            store.clear_scope(&first).await.unwrap();
            assert!(store.get_recent(&first, 10).await.unwrap().is_empty());
            assert_eq!(store.get_recent(&second, 10).await.unwrap().len(), 1);
        });
    }

    #[test]
    fn rejects_unc_storage() {
        let separator = std::path::MAIN_SEPARATOR;
        let path = format!("{separator}{separator}server{separator}share{separator}memory.sqlite3");
        assert!(SqliteSessionStore::open(path).is_err());
    }
}
