mod journal;
mod migrate;
mod operation;
mod projection;
mod row;

use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::adapters::os::safe_path::SafeDestination;
use crate::ports::journal::{EventRecord, JournalHead};
use crate::ports::operation::{OperationRecord, OperationStatus, PreparedOperationRecord};
use crate::ports::projection::ProjectionUpdate;

pub(crate) const APPLICATION_ID: i32 = 0x4A4A_4B31;
pub(crate) const STORAGE_SCHEMA_VERSION: u32 = 1;
pub(crate) const ENVELOPE_VERSION: u16 = 1;
const RUNTIME_BACKUP_GIT_PROJECTION: &str = "runtime-backup-git-v1";
const RUNTIME_BACKUP_GIT_BUNDLE_PROJECTION: &str = "runtime-backup-git-objects-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalMode {
    Wal,
}

#[derive(Clone, Debug)]
pub(crate) struct StoreOpenOptions {
    pub busy_timeout: Duration,
}

impl Default for StoreOpenOptions {
    fn default() -> Self {
        Self {
            busy_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum StoreError {
    #[error("SQLite store error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("store filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid store data: {0}")]
    InvalidData(String),
    #[error("journal head changed: expected sequence {expected_seq}, found {actual_seq}")]
    HeadChanged { expected_seq: u64, actual_seq: u64 },
    #[error("event chain mismatch at operation ordinal {ordinal}")]
    EventChainMismatch { ordinal: u32 },
    #[error("operation {operation_id} already exists with a different request")]
    IdempotencyConflict { operation_id: Uuid },
    #[error("invalid operation transition from {from:?} to {to:?}")]
    InvalidOperationTransition {
        from: OperationStatus,
        to: OperationStatus,
    },
    #[error(
        "projection {name} is stale: journal is at {journal_seq}, projection is at {projection_seq}"
    )]
    ProjectionStale {
        name: String,
        journal_seq: u64,
        projection_seq: u64,
    },
    #[error(
        "projection {name} reducer version {actual} is incompatible with required version {expected}"
    )]
    ReducerVersionMismatch {
        name: String,
        expected: u32,
        actual: u32,
    },
    #[error("migration checksum mismatch for version {version}")]
    MigrationChecksumMismatch { version: u32 },
    #[error("unsupported SQLite application id {0:#x}")]
    WrongApplicationId(i32),
    #[error("unsupported storage schema version {0}")]
    UnsupportedSchema(u32),
    #[error("JJK-E-STORAGE-UNSAFE: SQLite WAL durability is unavailable ({0})")]
    UnsafeStorage(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemaInfo {
    pub application_id: i32,
    pub schema_version: u32,
    pub journal_mode: JournalMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackupInfo {
    pub path: PathBuf,
    pub schema: SchemaInfo,
    pub head: JournalHead,
}

pub(crate) struct SqliteStore {
    connection: Connection,
    path: PathBuf,
    mode: JournalMode,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct RuntimeStateRow {
    pub state_id: String,
    pub git_oid: String,
    pub kind: String,
    pub label: String,
    pub message: String,
    pub attempt_id: String,
    pub logical_parent: Option<String>,
    pub created_seq: u64,
    pub archived: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeStateInsert {
    pub state_id: Uuid,
    pub attempt_id: Uuid,
    pub logical_parent: Option<Uuid>,
    pub workspace_id: Uuid,
    pub git_algorithm: String,
    pub git_oid: String,
    pub head_oid: Option<String>,
    pub kind: String,
    pub label: String,
    pub message: Option<String>,
    pub relative_locator: Vec<u8>,
}

const RUNTIME_NAVIGATION_PROJECTION: &str = "runtime-navigation-v1";
const RUNTIME_CONTROL_PROJECTION: &str = "runtime-control-history-v1";
const RUNTIME_ANNOTATIONS_PROJECTION: &str = "runtime-annotations-v1";
const RUNTIME_RECORDS_PROJECTION: &str = "runtime-records-v1";

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct RuntimeNavigation {
    pub entries: Vec<String>,
    pub cursor: Option<usize>,
}
#[derive(Clone, Debug)]
pub(crate) enum RuntimeProjection {
    Record {
        kind: String,
        id: String,
        value: Vec<u8>,
    },
    State {
        state: RuntimeStateInsert,
    },
    Activate {
        state: RuntimeStateRow,
        workspace_id: Uuid,
        relative_locator: Vec<u8>,
        head_oid: String,
    },
    Fork {
        source: RuntimeStateRow,
        attempt_id: Uuid,
        objective: String,
        workspace_id: Option<Uuid>,
        relative_locator: Option<Vec<u8>>,
        head_oid: Option<String>,
    },
    Archive {
        state: RuntimeStateRow,
        archived: bool,
    },
    Star {
        state: RuntimeStateRow,
        enabled: bool,
    },
    PickedState {
        state: RuntimeStateInsert,
        source_state: Uuid,
        provenance_id: Uuid,
        navigation: RuntimeNavigation,
    },
    ControlGit {
        before: RuntimeGitSnapshot,
        after: RuntimeGitSnapshot,
    },
    ControlRestore {
        to_cursor: usize,
    },
    Raw(ProjectionUpdate),
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct RuntimeGitRef {
    pub name: Vec<u8>,
    pub target: Vec<u8>,
    pub symbolic: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum RuntimeWorktreeEntry {
    Regular {
        path: Vec<u8>,
        mode: u32,
        bytes: Vec<u8>,
    },
    Symlink {
        path: Vec<u8>,
        target: Vec<u8>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct RuntimeGitSnapshot {
    pub refs: Vec<RuntimeGitRef>,
    pub head_symbolic: Option<Vec<u8>>,
    pub head_oid: Option<Vec<u8>>,
    pub index: Vec<u8>,
    pub worktree: Vec<RuntimeWorktreeEntry>,
}

#[derive(Clone, Debug)]
pub(crate) struct RuntimeControlRestore {
    pub current: Option<RuntimeStateRow>,
    pub git: RuntimeGitSnapshot,
    pub from_cursor: usize,
    pub to_cursor: usize,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RuntimeControlHistory {
    snapshots: Vec<RuntimeControlSnapshot>,
    cursor: usize,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RuntimeControlSnapshot {
    states: Vec<RuntimeStateProjection>,
    attempts: Vec<RuntimeAttemptProjection>,
    worktrees: Vec<RuntimeWorktreeProjection>,
    provenance: Vec<RuntimeProvenanceProjection>,
    navigation: Vec<(Vec<u8>, Vec<u8>)>,
    git: Option<RuntimeGitSnapshot>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RuntimeStateProjection {
    state_id: Vec<u8>,
    created_seq: u64,
    kind: String,
    git_algorithm: String,
    git_oid: String,
    attempt_id: Vec<u8>,
    label: String,
    message: Option<String>,
    archived: i64,
    last_event_seq: u64,
    logical_parent: Option<Vec<u8>>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RuntimeAttemptProjection {
    attempt_id: Vec<u8>,
    root_state_id: Vec<u8>,
    objective: String,
    current_tip_state_id: Option<Vec<u8>>,
    archived: i64,
    last_event_seq: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RuntimeWorktreeProjection {
    worktree_id: Vec<u8>,
    attempt_id: Option<Vec<u8>>,
    active_state_id: Option<Vec<u8>>,
    relative_locator: Vec<u8>,
    head_oid: Option<String>,
    index_tree_oid: Option<String>,
    dirty_digest: Option<Vec<u8>>,
    last_event_seq: u64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct RuntimeProvenanceProjection {
    source_state_id: Vec<u8>,
    result_state_id: Vec<u8>,
    relation: String,
    provenance_id: Vec<u8>,
    created_seq: u64,
}

impl SqliteStore {
    pub(crate) fn database_path(git_common_dir: &Path) -> PathBuf {
        git_common_dir.join("jjk").join("state.sqlite3")
    }

    pub(crate) fn open(
        git_common_dir: &Path,
        repo_id: Uuid,
        repository_root_token: &[u8],
        created_at_utc: &str,
        options: StoreOpenOptions,
    ) -> Result<Self, StoreError> {
        let path = Self::database_path(git_common_dir);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut store = Self {
            connection,
            path,
            mode: JournalMode::Wal,
        };
        store.configure(options.busy_timeout)?;
        migrate::run(&mut store.connection, created_at_utc)?;
        store.initialize_or_verify(repo_id, repository_root_token, created_at_utc)?;
        Ok(store)
    }
    pub(crate) fn open_existing(
        git_common_dir: &Path,
        repository_root_token: &[u8],
        options: StoreOpenOptions,
    ) -> Result<Self, StoreError> {
        let path = Self::database_path(git_common_dir);
        if !path.is_file() {
            return Err(StoreError::InvalidData(
                "JJK is not initialized; run `jjk setup`".into(),
            ));
        }
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let mut store = Self {
            connection,
            path,
            mode: JournalMode::Wal,
        };
        store.configure(options.busy_timeout)?;
        migrate::run(&mut store.connection, "1970-01-01T00:00:00Z")?;
        let repo_id = store.repository_id()?;
        store.initialize_or_verify(repo_id, repository_root_token, "1970-01-01T00:00:00Z")?;
        Ok(store)
    }

    fn configure(&mut self, busy_timeout: Duration) -> Result<(), StoreError> {
        self.connection.busy_timeout(busy_timeout)?;
        self.connection.pragma_update(None, "foreign_keys", true)?;
        self.connection
            .pragma_update(None, "trusted_schema", false)?;
        self.connection.pragma_update(None, "synchronous", "FULL")?;
        self.connection
            .pragma_update(None, "wal_autocheckpoint", 1000)?;
        let actual: String =
            self.connection
                .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if !actual.eq_ignore_ascii_case("wal") {
            return Err(StoreError::UnsafeStorage(format!(
                "SQLite selected journal mode {actual}"
            )));
        }
        self.mode = JournalMode::Wal;
        Ok(())
    }

    fn initialize_or_verify(
        &mut self,
        repo_id: Uuid,
        root_token: &[u8],
        created_at_utc: &str,
    ) -> Result<(), StoreError> {
        let existing: Option<(Vec<u8>, Vec<u8>, u32)> = self.connection.query_row(
            "SELECT repo_id, repository_root_token, storage_schema_version FROM journal_meta WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?;
        match existing {
            None => {
                self.connection.execute(
                    "INSERT INTO journal_meta (singleton, repo_id, repository_root_token, envelope_version, storage_schema_version, created_at_utc) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
                    params![repo_id.as_bytes(), root_token, ENVELOPE_VERSION, STORAGE_SCHEMA_VERSION, created_at_utc],
                )?;
            }
            Some((stored_repo, stored_token, version)) => {
                if stored_repo != repo_id.as_bytes() || stored_token != root_token {
                    return Err(StoreError::InvalidData(
                        "store belongs to a different safe space".into(),
                    ));
                }
                if version != STORAGE_SCHEMA_VERSION {
                    return Err(StoreError::UnsupportedSchema(version));
                }
            }
        }
        let application_id: i32 =
            self.connection
                .pragma_query_value(None, "application_id", |row| row.get(0))?;
        if application_id != APPLICATION_ID {
            return Err(StoreError::WrongApplicationId(application_id));
        }
        let head = journal::head_in(&self.connection)?;
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO projection_meta (projection_name, reducer_version, projected_through_seq, projected_through_hash, projection_digest) VALUES (?1, 1, 0, zeroblob(32), zeroblob(32)) ON CONFLICT(projection_name) DO NOTHING",
            [RUNTIME_RECORDS_PROJECTION],
        )?;
        projection::advance_all(&tx, head.local_seq, head.event_hash)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn read_backup_primary_workspace(database: &Path) -> Result<Uuid, StoreError> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        verify_connection(&connection, "backup")?;
        let rows = connection
            .prepare("SELECT worktree_id FROM worktree_current LIMIT 2")?
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() != 1 {
            return Err(StoreError::InvalidData(format!(
                "backup requires exactly one primary workspace, found {}",
                rows.len()
            )));
        }
        Uuid::from_slice(&rows[0]).map_err(|error| {
            StoreError::InvalidData(format!("invalid backup workspace id: {error}"))
        })
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn verified_backup_to(
        &self,
        destination: SafeDestination,
        snapshot: &RuntimeGitSnapshot,
        object_bundle: &[u8],
    ) -> Result<BackupInfo, StoreError> {
        self.backup_to_reserved(destination, snapshot, object_bundle)
    }
    pub(crate) fn schema_info(&self) -> Result<SchemaInfo, StoreError> {
        let application_id = self
            .connection
            .pragma_query_value(None, "application_id", |row| row.get(0))?;
        let schema_version = self.connection.query_row(
            "SELECT storage_schema_version FROM journal_meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(SchemaInfo {
            application_id,
            schema_version,
            journal_mode: self.mode,
        })
    }
    pub(crate) fn repository_uuid(&self) -> Result<Uuid, StoreError> {
        self.repository_id()
    }
    pub(crate) fn verify_backup_file(database: &Path) -> Result<BackupInfo, StoreError> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let info = backup_info(&connection, database)?;
        read_backup_git_snapshot_from(&connection)?;
        read_backup_git_bundle_from(&connection)?;
        Ok(info)
    }
    pub(crate) fn read_backup_git_snapshot(
        database: &Path,
    ) -> Result<RuntimeGitSnapshot, StoreError> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        verify_connection(&connection, "backup")?;
        read_backup_git_snapshot_from(&connection)
    }
    pub(crate) fn read_backup_git_bundle(database: &Path) -> Result<Vec<u8>, StoreError> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        verify_connection(&connection, "backup")?;
        read_backup_git_bundle_from(&connection)
    }
    pub(crate) fn rebind_backup_file(
        database: &Path,
        repository_root_token: &[u8],
    ) -> Result<(), StoreError> {
        let connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        verify_connection(&connection, "backup")?;
        let changed = connection.execute(
            "UPDATE journal_meta SET repository_root_token = ?1 WHERE singleton = 1",
            params![repository_root_token],
        )?;
        if changed != 1 {
            return Err(StoreError::InvalidData(
                "backup has no journal metadata".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn rebind_primary_workspace(
        database: &Path,
        workspace_id: Uuid,
        relative_locator: &[u8],
    ) -> Result<(), StoreError> {
        let connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        verify_connection(&connection, "backup")?;
        let count: u64 =
            connection.query_row("SELECT COUNT(*) FROM worktree_current", [], |row| {
                row.get(0)
            })?;
        if count != 1 {
            return Err(StoreError::InvalidData(format!(
                "backup load requires exactly one primary workspace, found {count}"
            )));
        }
        connection.execute(
            "UPDATE worktree_current SET worktree_id=?1, relative_locator=?2",
            params![workspace_id.as_bytes(), relative_locator],
        )?;
        let navigation:Option<(Vec<u8>,Vec<u8>,u64)>=connection.query_row("SELECT record_key,record_value,last_event_seq FROM projection_records WHERE projection_name=?1",[RUNTIME_NAVIGATION_PROJECTION],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        if let Some((old, value, seq)) = navigation {
            connection.execute(
                "DELETE FROM projection_records WHERE projection_name=?1 AND record_key=?2",
                params![RUNTIME_NAVIGATION_PROJECTION, old],
            )?;
            connection.execute("INSERT INTO projection_records(projection_name,record_key,record_value,last_event_seq)VALUES(?1,?2,?3,?4)",params![RUNTIME_NAVIGATION_PROJECTION,workspace_id.as_bytes(),value,seq])?;
        }
        Ok(())
    }

    pub(crate) fn state_rows(&self) -> Result<Vec<RuntimeStateRow>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT hex(s.state_id), s.git_oid, s.kind, s.label, COALESCE(s.message, ''), \
                    hex(s.attempt_id), NULLIF(hex(p.parent_state_id), ''), s.created_seq, s.archived \
             FROM states s LEFT JOIN state_logical_parents p ON p.child_state_id = s.state_id \
             ORDER BY s.created_seq",
        )?;
        statement
            .query_map([], |row| {
                Ok(RuntimeStateRow {
                    state_id: row.get(0)?,
                    git_oid: row.get(1)?,
                    kind: row.get(2)?,
                    label: row.get(3)?,
                    message: row.get(4)?,
                    attempt_id: row.get(5)?,
                    logical_parent: row.get(6)?,
                    created_seq: row.get(7)?,
                    archived: row.get::<_, i64>(8)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub(crate) fn sole_workspace_id(&self) -> Result<Option<Uuid>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT worktree_id FROM worktree_current LIMIT 2")?;
        let rows = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() != 1 {
            return Ok(None);
        }
        Ok(Some(Uuid::from_slice(&rows[0]).map_err(|error| {
            StoreError::InvalidData(format!("invalid workspace id: {error}"))
        })?))
    }
    pub(crate) fn workspace_id_for_locator(
        &self,
        relative_locator: &[u8],
    ) -> Result<Option<Uuid>, StoreError> {
        let workspace: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT worktree_id FROM worktree_current WHERE relative_locator = ?1",
                params![relative_locator],
                |row| row.get(0),
            )
            .optional()?;
        workspace
            .map(|bytes| {
                Uuid::from_slice(&bytes).map_err(|error| {
                    StoreError::InvalidData(format!("invalid workspace id: {error}"))
                })
            })
            .transpose()
    }
    pub(crate) fn workspace_id_for_head(&self, head_oid: &str) -> Result<Option<Uuid>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT worktree_id FROM worktree_current WHERE head_oid = ?1 LIMIT 2")?;
        let rows = statement
            .query_map([head_oid], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() != 1 {
            return Ok(None);
        }
        Ok(Some(Uuid::from_slice(&rows[0]).map_err(|error| {
            StoreError::InvalidData(format!("invalid workspace id: {error}"))
        })?))
    }
    pub(crate) fn current_state_row(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<RuntimeStateRow>, StoreError> {
        self.connection
            .query_row(
                "SELECT hex(s.state_id), s.git_oid, s.kind, s.label, COALESCE(s.message, ''), \
                        hex(s.attempt_id), NULLIF(hex(p.parent_state_id), ''), s.created_seq, s.archived \
                 FROM worktree_current w \
                 JOIN states s ON s.state_id = w.active_state_id \
                 LEFT JOIN state_logical_parents p ON p.child_state_id = s.state_id \
                 WHERE w.worktree_id = ?1",
                params![workspace_id.as_bytes()],
                |row| {
                    Ok(RuntimeStateRow {
                        state_id: row.get(0)?,
                        git_oid: row.get(1)?,
                        kind: row.get(2)?,
                        label: row.get(3)?,
                        message: row.get(4)?,
                        attempt_id: row.get(5)?,
                        logical_parent: row.get(6)?,
                        created_seq: row.get(7)?,
                        archived: row.get::<_, i64>(8)? != 0,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }
    pub(crate) fn current_attempt_id(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<Uuid>, StoreError> {
        let attempt: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT attempt_id FROM worktree_current WHERE worktree_id = ?1",
                params![workspace_id.as_bytes()],
                |row| row.get(0),
            )
            .optional()?;
        attempt
            .map(|bytes| {
                Uuid::from_slice(&bytes).map_err(|error| {
                    StoreError::InvalidData(format!("invalid workspace attempt id: {error}"))
                })
            })
            .transpose()
    }

    pub(crate) fn resolve_state_row(&self, query: &str) -> Result<RuntimeStateRow, StoreError> {
        let normalized = query
            .parse::<crate::domain::StateId>()
            .map(|id| hex::encode_upper(id.into_bytes()))
            .unwrap_or_else(|_| {
                query
                    .strip_prefix("refs/jjk/states/")
                    .unwrap_or(query)
                    .to_ascii_uppercase()
            });
        let matches = self
            .state_rows()?
            .into_iter()
            .filter(|state| {
                state.state_id.starts_with(&normalized)
                    || state.label == query
                    || format!("refs/jjk/states/{}", state.state_id).eq_ignore_ascii_case(query)
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [state] => Ok(state.clone()),
            [] => Err(StoreError::InvalidData(format!(
                "state `{query}` not found"
            ))),
            _ => Err(StoreError::InvalidData(format!(
                "state `{query}` is ambiguous"
            ))),
        }
    }
    pub(crate) fn state_is_starred(&self, state_id: &str) -> Result<bool, StoreError> {
        let state_id = parse_hex_uuid(state_id, "state id")?;
        let value: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = ?2",
                params![RUNTIME_ANNOTATIONS_PROJECTION, state_id.as_bytes()],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| {
                    StoreError::InvalidData(format!("invalid state annotation projection: {error}"))
                })
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }
    pub(crate) fn logical_children(
        &self,
        state: &RuntimeStateRow,
    ) -> Result<Vec<RuntimeStateRow>, StoreError> {
        let parent = state.state_id.clone();
        Ok(self
            .state_rows()?
            .into_iter()
            .filter(|candidate| candidate.logical_parent.as_deref() == Some(parent.as_str()))
            .collect())
    }

    pub(crate) fn fork_runtime_attempt(
        &mut self,
        event: &EventRecord,
        source: &RuntimeStateRow,
        attempt_id: Uuid,
        objective: &str,
    ) -> Result<JournalHead, StoreError> {
        let source_id = parse_hex_uuid(&source.state_id, "source state id")?;
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            tx.execute(
                "INSERT INTO attempts (attempt_id, root_state_id, objective, current_tip_state_id, archived, last_event_seq) VALUES (?1, ?2, ?3, NULL, 0, ?4)",
                params![attempt_id.as_bytes(), source_id.as_bytes(), objective, sequences[0]],
            )?;
            Ok(())
        })
    }

    pub(crate) fn archive_runtime_state(
        &mut self,
        event: &EventRecord,
        state: &RuntimeStateRow,
        archived: bool,
    ) -> Result<JournalHead, StoreError> {
        let state_id = parse_hex_uuid(&state.state_id, "state id")?;
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            let changed = tx.execute(
                "UPDATE states SET archived = ?2, last_event_seq = ?3 WHERE state_id = ?1 AND archived != ?2",
                params![state_id.as_bytes(), i64::from(archived), sequences[0]],
            )?;
            if changed != 1 {
                return Err(StoreError::InvalidData(if archived { "state is already archived".into() } else { "state is not archived".into() }));
            }
            Ok(())
        })
    }

    pub(crate) fn import_runtime_history(
        &mut self,
        events: &[EventRecord],
        states: &[RuntimeStateInsert],
    ) -> Result<JournalHead, StoreError> {
        if events.len() != states.len() {
            return Err(StoreError::InvalidData(
                "history import event/state cardinality mismatch".into(),
            ));
        }
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head,events,|tx,sequences|{
            for (state,seq) in states.iter().zip(sequences.iter().copied()){
                tx.execute("INSERT INTO attempts (attempt_id,root_state_id,objective,current_tip_state_id,archived,last_event_seq) VALUES (?1,?2,?3,?2,0,?4)",params![state.attempt_id.as_bytes(),state.state_id.as_bytes(),state.label,seq])?;
                tx.execute("INSERT INTO states (state_id,created_seq,kind,git_algorithm,git_oid,attempt_id,label,message,archived,last_event_seq) VALUES (?1,?2,'imported',?3,?4,?5,?6,?7,0,?2)",params![state.state_id.as_bytes(),seq,state.git_algorithm,state.git_oid,state.attempt_id.as_bytes(),state.label,state.message])?;
                if let Some(parent)=state.logical_parent{tx.execute("INSERT INTO state_logical_parents(child_state_id,parent_state_id,created_seq) VALUES (?1,?2,?3)",params![state.state_id.as_bytes(),parent.as_bytes(),seq])?;}
            }
            Ok(())
        })
    }
    pub(crate) fn append_runtime_state(
        &mut self,
        event: &EventRecord,
        state: &RuntimeStateInsert,
    ) -> Result<JournalHead, StoreError> {
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            let seq = sequences[0];
            if state.logical_parent.is_none() {
                tx.execute(
                    "INSERT INTO attempts (attempt_id, root_state_id, objective, current_tip_state_id, archived, last_event_seq) VALUES (?1, ?2, ?3, ?2, 0, ?4)",
                    params![state.attempt_id.as_bytes(), state.state_id.as_bytes(), state.label, seq],
                )?;
            }
            tx.execute(
                "INSERT INTO states (state_id, created_seq, kind, git_algorithm, git_oid, attempt_id, label, message, archived, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?2)",
                params![state.state_id.as_bytes(), seq, state.kind, state.git_algorithm, state.git_oid, state.attempt_id.as_bytes(), state.label, state.message],
            )?;
            if let Some(parent) = state.logical_parent {
                tx.execute(
                    "INSERT INTO state_logical_parents (child_state_id, parent_state_id, created_seq) VALUES (?1, ?2, ?3)",
                    params![state.state_id.as_bytes(), parent.as_bytes(), seq],
                )?;
                tx.execute(
                    "UPDATE attempts SET current_tip_state_id = ?2, last_event_seq = ?3 WHERE attempt_id = ?1",
                    params![state.attempt_id.as_bytes(), state.state_id.as_bytes(), seq],
                )?;
            }
            tx.execute("DELETE FROM worktree_current WHERE worktree_id = ?1", params![state.workspace_id.as_bytes()])?;
            tx.execute(
                "INSERT INTO worktree_current (worktree_id, attempt_id, active_state_id, relative_locator, head_oid, index_tree_oid, dirty_digest, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, zeroblob(32), ?7)",
                params![state.workspace_id.as_bytes(), state.attempt_id.as_bytes(), state.state_id.as_bytes(), state.relative_locator, state.head_oid, state.git_oid, seq],
            )?;
            Ok(())
        })
    }

    pub(crate) fn activate_runtime_state(
        &mut self,
        event: &EventRecord,
        state: &RuntimeStateRow,
        workspace_id: Uuid,
        relative_locator: &[u8],
        head_oid: &str,
    ) -> Result<JournalHead, StoreError> {
        let state_id = parse_hex_uuid(&state.state_id, "state id")?;
        let attempt_id = parse_hex_uuid(&state.attempt_id, "attempt id")?;
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            tx.execute("DELETE FROM worktree_current WHERE worktree_id = ?1", params![workspace_id.as_bytes()])?;
            tx.execute(
                "INSERT INTO worktree_current (worktree_id, attempt_id, active_state_id, relative_locator, head_oid, index_tree_oid, dirty_digest, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, zeroblob(32), ?7)",
                params![workspace_id.as_bytes(), attempt_id.as_bytes(), state_id.as_bytes(), relative_locator, head_oid, state.git_oid, sequences[0]],
            )?;
            Ok(())
        })
    }

    pub(crate) fn runtime_navigation(
        &self,
        workspace_id: Uuid,
    ) -> Result<RuntimeNavigation, StoreError> {
        let value: Option<Vec<u8>> = self.connection.query_row(
            "SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = ?2",
            params![RUNTIME_NAVIGATION_PROJECTION, workspace_id.as_bytes()], |row| row.get(0),
        ).optional()?;
        value
            .map(|bytes| {
                serde_json::from_slice(&bytes).map_err(|error| {
                    StoreError::InvalidData(format!("invalid navigation projection: {error}"))
                })
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub(crate) fn navigate_runtime_history(
        &mut self,
        event: &EventRecord,
        workspace_id: Uuid,
        relative_locator: &[u8],
        head_oid: &str,
        direction: i8,
    ) -> Result<(RuntimeStateRow, usize, usize), StoreError> {
        let current = self
            .current_state_row(workspace_id)?
            .ok_or_else(|| StoreError::InvalidData("no current JJK state exists".into()))?;
        let mut navigation = self.runtime_navigation(workspace_id)?;
        if navigation.entries.is_empty() {
            navigation.entries.push(current.state_id.clone());
            navigation.cursor = Some(0);
        }
        let from = navigation.cursor.ok_or_else(|| {
            StoreError::InvalidData("navigation history has no current position".into())
        })?;
        let to = if direction < 0 {
            from.checked_sub(1)
        } else {
            from.checked_add(1)
                .filter(|index| *index < navigation.entries.len())
        }
        .ok_or_else(|| {
            StoreError::InvalidData(
                if direction < 0 {
                    "no earlier navigation state"
                } else {
                    "no later navigation state"
                }
                .into(),
            )
        })?;
        let target = self.resolve_state_row(&navigation.entries[to])?;
        navigation.cursor = Some(to);
        let state_id = parse_hex_uuid(&target.state_id, "state id")?;
        let attempt_id = parse_hex_uuid(&target.attempt_id, "attempt id")?;
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            let seq = sequences[0];
            tx.execute("DELETE FROM worktree_current WHERE worktree_id = ?1", params![workspace_id.as_bytes()])?;
            tx.execute("INSERT INTO worktree_current (worktree_id, attempt_id, active_state_id, relative_locator, head_oid, index_tree_oid, dirty_digest, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, zeroblob(32), ?7)", params![workspace_id.as_bytes(), attempt_id.as_bytes(), state_id.as_bytes(), relative_locator, head_oid, target.git_oid, seq])?;
            put_runtime_record(tx, RUNTIME_NAVIGATION_PROJECTION, workspace_id.as_bytes(), &navigation, seq)?;
            Ok(())
        })?;
        Ok((target, from, to))
    }

    pub(crate) fn append_runtime_record(
        &mut self,
        event: &EventRecord,
        kind: &str,
        id: &str,
        value: &[u8],
    ) -> Result<JournalHead, StoreError> {
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            let key = format!("{kind}\0{id}");
            projection::put_record(
                tx,
                RUNTIME_RECORDS_PROJECTION,
                1,
                key.as_bytes(),
                value,
                sequences[0],
            )
        })
    }

    pub(crate) fn runtime_record(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let key = format!("{kind}\0{id}");
        self.connection.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = ?2", params![RUNTIME_RECORDS_PROJECTION, key.as_bytes()], |row| row.get(0)).optional().map_err(StoreError::from)
    }

    pub(crate) fn append_picked_runtime_state(
        &mut self,
        event: &EventRecord,
        state: &RuntimeStateInsert,
        source_state: Uuid,
        provenance_id: Uuid,
        navigation: RuntimeNavigation,
    ) -> Result<JournalHead, StoreError> {
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            insert_runtime_state(tx, state, sequences[0])?;
            tx.execute("INSERT INTO state_provenance_edges (source_state_id, result_state_id, relation, provenance_id, created_seq) VALUES (?1, ?2, 'composed-from', ?3, ?4)", params![source_state.as_bytes(), state.state_id.as_bytes(), provenance_id.as_bytes(), sequences[0]])?;
            put_runtime_record(tx, RUNTIME_NAVIGATION_PROJECTION, state.workspace_id.as_bytes(), &navigation, sequences[0])?;
            Ok(())
        })
    }

    pub(crate) fn plan_runtime_control_restore(
        &self,
        direction: i8,
        workspace_id: Uuid,
    ) -> Result<RuntimeControlRestore, StoreError> {
        let bytes: Vec<u8> = self.connection.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = x'00'", [RUNTIME_CONTROL_PROJECTION], |row| row.get(0)).optional()?.ok_or_else(|| StoreError::InvalidData("no control history exists".into()))?;
        let history: RuntimeControlHistory = serde_json::from_slice(&bytes).map_err(|error| {
            StoreError::InvalidData(format!("invalid control history: {error}"))
        })?;
        let to = if direction < 0 {
            history.cursor.checked_sub(1)
        } else {
            history
                .cursor
                .checked_add(1)
                .filter(|index| *index < history.snapshots.len())
        }
        .ok_or_else(|| {
            StoreError::InvalidData(
                if direction < 0 {
                    "no earlier control snapshot to undo to"
                } else {
                    "no later control snapshot to redo to"
                }
                .into(),
            )
        })?;
        let snapshot = &history.snapshots[to];
        let active = snapshot
            .worktrees
            .iter()
            .find(|row| row.worktree_id == workspace_id.as_bytes())
            .and_then(|row| row.active_state_id.as_ref());
        let current = active
            .and_then(|id| snapshot.states.iter().find(|state| &state.state_id == id))
            .map(runtime_state_from_projection)
            .transpose()?;
        let git = snapshot.git.clone().ok_or_else(|| {
            StoreError::InvalidData("control snapshot predates exact Git artifact capture".into())
        })?;
        Ok(RuntimeControlRestore {
            current,
            git,
            from_cursor: history.cursor,
            to_cursor: to,
        })
    }

    pub(crate) fn apply_runtime_control_restore(
        &mut self,
        event: &EventRecord,
        to_cursor: usize,
    ) -> Result<JournalHead, StoreError> {
        let bytes: Vec<u8> = self.connection.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = x'00'", [RUNTIME_CONTROL_PROJECTION], |row| row.get(0))?;
        let mut history: RuntimeControlHistory =
            serde_json::from_slice(&bytes).map_err(|error| {
                StoreError::InvalidData(format!("invalid control history: {error}"))
            })?;
        let snapshot =
            history.snapshots.get(to_cursor).cloned().ok_or_else(|| {
                StoreError::InvalidData("control history cursor is invalid".into())
            })?;
        let head = <Self as crate::ports::journal::Journal>::head(self)?;
        self.append_atomic(head, std::slice::from_ref(event), |tx, sequences| {
            restore_control_snapshot(tx, &snapshot)?;
            history.cursor = to_cursor;
            projection::put_record(
                tx,
                RUNTIME_CONTROL_PROJECTION,
                1,
                &[0],
                &serde_json::to_vec(&history)
                    .map_err(|error| StoreError::InvalidData(error.to_string()))?,
                sequences[0],
            )?;
            Ok(())
        })
    }

    pub(crate) fn runtime_git_snapshot_for_state(
        &self,
        workspace_id: Uuid,
        state_id: &str,
    ) -> Result<Option<RuntimeGitSnapshot>, StoreError> {
        let bytes: Option<Vec<u8>> = self.connection.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = x'00'", [RUNTIME_CONTROL_PROJECTION], |row| row.get(0)).optional()?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let history: RuntimeControlHistory = serde_json::from_slice(&bytes).map_err(|error| {
            StoreError::InvalidData(format!("invalid control history: {error}"))
        })?;
        let wanted = parse_hex_uuid(state_id, "state id")?;
        Ok(history.snapshots.iter().rev().find_map(|snapshot| {
            snapshot
                .worktrees
                .iter()
                .any(|row| {
                    row.worktree_id == workspace_id.as_bytes()
                        && row.active_state_id.as_deref() == Some(wanted.as_bytes())
                })
                .then(|| snapshot.git.clone())
                .flatten()
        }))
    }

    pub(crate) fn append_atomic<R>(
        &mut self,
        expected_head: JournalHead,
        events: &[EventRecord],
        reduce: R,
    ) -> Result<JournalHead, StoreError>
    where
        R: FnOnce(&rusqlite::Transaction<'_>, &[u64]) -> Result<(), StoreError>,
    {
        let repo_id = self.repository_id()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.pragma_update(None, "defer_foreign_keys", true)?;
        let actual_head = journal::head_in(&tx)?;
        if actual_head != expected_head {
            return Err(StoreError::HeadChanged {
                expected_seq: expected_head.local_seq,
                actual_seq: actual_head.local_seq,
            });
        }
        let mut prior_hash = actual_head.event_hash;
        let mut sequences = Vec::with_capacity(events.len());
        for event in events {
            event
                .validate_for_append(repo_id, ENVELOPE_VERSION)
                .map_err(|reason| StoreError::InvalidData(reason.into()))?;
            if event.previous_event_hash != prior_hash {
                return Err(StoreError::EventChainMismatch {
                    ordinal: event.operation_ordinal,
                });
            }
            let seq = journal::insert(&tx, event)?;
            sequences.push(seq);
            prior_hash = event.event_hash;
        }
        reduce(&tx, &sequences)?;
        projection::advance_all(
            &tx,
            sequences.last().copied().unwrap_or(actual_head.local_seq),
            prior_hash,
        )?;
        tx.commit()?;
        Ok(JournalHead {
            local_seq: sequences.last().copied().unwrap_or(actual_head.local_seq),
            event_hash: prior_hash,
        })
    }

    pub(crate) fn prepare(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        prepared: &PreparedOperationRecord,
    ) -> Result<OperationRecord, StoreError> {
        if event.operation_id != prepared.operation_id || event.operation_ordinal != 0 {
            return Err(StoreError::InvalidData(
                "prepared event must be ordinal zero for its operation".into(),
            ));
        }
        if let Some(existing) = operation::get(&self.connection, prepared.operation_id)? {
            if existing.request_hash == prepared.request_hash {
                return Ok(existing);
            }
            return Err(StoreError::IdempotencyConflict {
                operation_id: prepared.operation_id,
            });
        }
        self.append_atomic(expected_head, std::slice::from_ref(event), |tx, seqs| {
            operation::insert_prepared(tx, prepared, seqs[0])
        })?;
        operation::get(&self.connection, prepared.operation_id)?
            .ok_or_else(|| StoreError::InvalidData("prepared operation was not persisted".into()))
    }

    pub(crate) fn transition_operation(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation_id: Uuid,
        to: OperationStatus,
        result: Option<&[u8]>,
    ) -> Result<OperationRecord, StoreError> {
        if event.operation_id != operation_id {
            return Err(StoreError::InvalidData(
                "lifecycle event belongs to a different operation".into(),
            ));
        }
        let expected_ordinal = self.connection.query_row(
            "SELECT COUNT(*) FROM events WHERE operation_id = ?1",
            params![operation_id.as_bytes()],
            |row| row.get::<_, u32>(0),
        )?;
        if event.operation_ordinal != expected_ordinal {
            return Err(StoreError::InvalidData(format!(
                "operation event ordinal must be {expected_ordinal}, found {}",
                event.operation_ordinal
            )));
        }
        let current = operation::get(&self.connection, operation_id)?.ok_or_else(|| {
            StoreError::InvalidData(format!("operation {operation_id} does not exist"))
        })?;
        if !current.status.can_transition_to(to) {
            return Err(StoreError::InvalidOperationTransition {
                from: current.status,
                to,
            });
        }
        self.append_atomic(expected_head, std::slice::from_ref(event), |tx, seqs| {
            operation::transition(tx, operation_id, to, seqs[0], result)
        })?;
        operation::get(&self.connection, operation_id)?
            .ok_or_else(|| StoreError::InvalidData("transitioned operation disappeared".into()))
    }

    fn repository_id(&self) -> Result<Uuid, StoreError> {
        let bytes: Vec<u8> = self.connection.query_row(
            "SELECT repo_id FROM journal_meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        Uuid::from_slice(&bytes)
            .map_err(|error| StoreError::InvalidData(format!("invalid repository id: {error}")))
    }
    pub(crate) fn integrity_check(&self) -> Result<(), StoreError> {
        verify_connection(&self.connection, "store")
    }
    fn backup_to_reserved(
        &self,
        destination: SafeDestination,
        snapshot: &RuntimeGitSnapshot,
        object_bundle: &[u8],
    ) -> Result<BackupInfo, StoreError> {
        let mut target = Connection::open_in_memory()?;
        {
            let backup = rusqlite::backup::Backup::new(&self.connection, &mut target)?;
            backup.run_to_completion(128, Duration::from_millis(10), None)?;
        }
        let info = backup_info(&target, destination.path())?;
        attach_backup_git_snapshot_to(&mut target, snapshot)?;
        attach_backup_git_bundle_to(&mut target, object_bundle)?;
        verify_connection(&target, "backup")?;
        read_backup_git_snapshot_from(&target)?;
        read_backup_git_bundle_from(&target)?;
        let bytes = target.serialize(rusqlite::MAIN_DB)?;
        let mut staging = destination.create_staging_file()?;
        staging.file_mut().seek(SeekFrom::Start(0))?;
        staging.file_mut().write_all(&bytes)?;
        staging.file_mut().set_len(
            u64::try_from(bytes.len())
                .map_err(|_| StoreError::InvalidData("backup is too large to publish".into()))?,
        )?;
        staging.verify_contents(&bytes)?;
        let path = destination.publish(staging)?;
        Ok(BackupInfo { path, ..info })
    }

    pub(crate) fn backup_to(&self, destination: &Path) -> Result<BackupInfo, StoreError> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut target = Connection::open(destination)?;
        {
            let backup = rusqlite::backup::Backup::new(&self.connection, &mut target)?;
            backup.run_to_completion(128, Duration::from_millis(10), None)?;
        }
        verify_connection(&target, "backup")?;
        let application_id = target.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if application_id != APPLICATION_ID {
            return Err(StoreError::WrongApplicationId(application_id));
        }
        let schema_version = target.query_row(
            "SELECT storage_schema_version FROM journal_meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        if schema_version != STORAGE_SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema(schema_version));
        }
        let head = journal::head_in(&target)?;
        Ok(BackupInfo {
            path: destination.to_owned(),
            schema: SchemaInfo {
                application_id,
                schema_version,
                journal_mode: JournalMode::Wal,
            },
            head,
        })
    }
}
fn put_runtime_record<T: serde::Serialize>(
    tx: &rusqlite::Transaction<'_>,
    projection_name: &str,
    key: &[u8],
    value: &T,
    seq: u64,
) -> Result<(), StoreError> {
    projection::put_record(
        tx,
        projection_name,
        1,
        key,
        &serde_json::to_vec(value).map_err(|error| StoreError::InvalidData(error.to_string()))?,
        seq,
    )
}
fn insert_runtime_state(
    tx: &rusqlite::Transaction<'_>,
    state: &RuntimeStateInsert,
    seq: u64,
) -> Result<(), StoreError> {
    tx.execute("INSERT INTO states (state_id, created_seq, kind, git_algorithm, git_oid, attempt_id, label, message, archived, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?2)", params![state.state_id.as_bytes(), seq, state.kind, state.git_algorithm, state.git_oid, state.attempt_id.as_bytes(), state.label, state.message])?;
    if let Some(parent) = state.logical_parent {
        tx.execute("INSERT INTO state_logical_parents (child_state_id, parent_state_id, created_seq) VALUES (?1, ?2, ?3)", params![state.state_id.as_bytes(), parent.as_bytes(), seq])?;
    }
    tx.execute(
        "UPDATE attempts SET current_tip_state_id = ?2, last_event_seq = ?3 WHERE attempt_id = ?1",
        params![state.attempt_id.as_bytes(), state.state_id.as_bytes(), seq],
    )?;
    tx.execute(
        "DELETE FROM worktree_current WHERE worktree_id = ?1",
        params![state.workspace_id.as_bytes()],
    )?;
    tx.execute("INSERT INTO worktree_current (worktree_id, attempt_id, active_state_id, relative_locator, head_oid, index_tree_oid, dirty_digest, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, zeroblob(32), ?7)", params![state.workspace_id.as_bytes(), state.attempt_id.as_bytes(), state.state_id.as_bytes(), state.relative_locator, state.head_oid, state.git_oid, seq])?;
    Ok(())
}
fn apply_runtime_projection(
    tx: &rusqlite::Transaction<'_>,
    projection: &RuntimeProjection,
    seq: u64,
) -> Result<(), StoreError> {
    match projection {
        RuntimeProjection::Record { kind, id, value } => {
            let key = format!("{kind}\0{id}");
            projection::put_record(
                tx,
                RUNTIME_RECORDS_PROJECTION,
                1,
                key.as_bytes(),
                value,
                seq,
            )
        }
        RuntimeProjection::State { state } => {
            if state.logical_parent.is_none() {
                tx.execute(
                    "INSERT INTO attempts (attempt_id, root_state_id, objective, current_tip_state_id, archived, last_event_seq) VALUES (?1, ?2, ?3, ?2, 0, ?4)",
                    params![state.attempt_id.as_bytes(), state.state_id.as_bytes(), state.label, seq],
                )?;
            }
            insert_runtime_state(tx, state, seq)
        }
        RuntimeProjection::Activate {
            state,
            workspace_id,
            relative_locator,
            head_oid,
        } => {
            let state_id = parse_hex_uuid(&state.state_id, "state id")?;
            let attempt_id = parse_hex_uuid(&state.attempt_id, "attempt id")?;
            tx.execute(
                "DELETE FROM worktree_current WHERE worktree_id = ?1",
                params![workspace_id.as_bytes()],
            )?;
            tx.execute(
                "INSERT INTO worktree_current (worktree_id, attempt_id, active_state_id, relative_locator, head_oid, index_tree_oid, dirty_digest, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, zeroblob(32), ?7)",
                params![workspace_id.as_bytes(), attempt_id.as_bytes(), state_id.as_bytes(), relative_locator, head_oid, state.git_oid, seq],
            )?;
            Ok(())
        }
        RuntimeProjection::Fork {
            source,
            attempt_id,
            objective,
            workspace_id,
            relative_locator,
            head_oid,
        } => {
            let source_id = parse_hex_uuid(&source.state_id, "source state id")?;
            tx.execute(
                "INSERT INTO attempts (attempt_id, root_state_id, objective, current_tip_state_id, archived, last_event_seq) VALUES (?1, ?2, ?3, NULL, 0, ?4)",
                params![attempt_id.as_bytes(), source_id.as_bytes(), objective, seq],
            )?;
            if let Some(workspace_id) = workspace_id {
                let relative_locator = relative_locator.as_deref().ok_or_else(|| {
                    StoreError::InvalidData("materialized fork has no worktree locator".into())
                })?;
                let head_oid = head_oid.as_deref().ok_or_else(|| {
                    StoreError::InvalidData("materialized fork has no head OID".into())
                })?;
                tx.execute(
                    "DELETE FROM worktree_current WHERE worktree_id = ?1",
                    params![workspace_id.as_bytes()],
                )?;
                tx.execute(
                    "INSERT INTO worktree_current (worktree_id, attempt_id, active_state_id, relative_locator, head_oid, index_tree_oid, dirty_digest, last_event_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, zeroblob(32), ?7)",
                    params![workspace_id.as_bytes(), attempt_id.as_bytes(), source_id.as_bytes(), relative_locator, head_oid, source.git_oid, seq],
                )?;
            }
            Ok(())
        }
        RuntimeProjection::Archive { state, archived } => {
            let state_id = parse_hex_uuid(&state.state_id, "state id")?;
            let changed = tx.execute("UPDATE states SET archived = ?2, last_event_seq = ?3 WHERE state_id = ?1 AND archived != ?2", params![state_id.as_bytes(), i64::from(*archived), seq])?;
            if changed != 1 {
                return Err(StoreError::InvalidData(if *archived {
                    "state is already archived".into()
                } else {
                    "state is not archived".into()
                }));
            }
            Ok(())
        }
        RuntimeProjection::Star { state, enabled } => {
            let state_id = parse_hex_uuid(&state.state_id, "state id")?;
            put_runtime_record(
                tx,
                RUNTIME_ANNOTATIONS_PROJECTION,
                state_id.as_bytes(),
                enabled,
                seq,
            )
        }
        RuntimeProjection::PickedState {
            state,
            source_state,
            provenance_id,
            navigation,
        } => {
            insert_runtime_state(tx, state, seq)?;
            tx.execute("INSERT INTO state_provenance_edges (source_state_id, result_state_id, relation, provenance_id, created_seq) VALUES (?1, ?2, 'composed-from', ?3, ?4)", params![source_state.as_bytes(), state.state_id.as_bytes(), provenance_id.as_bytes(), seq])?;
            put_runtime_record(
                tx,
                RUNTIME_NAVIGATION_PROJECTION,
                state.workspace_id.as_bytes(),
                navigation,
                seq,
            )
        }
        RuntimeProjection::ControlGit { before, after } => {
            record_control_snapshot(tx, seq, before, after)
        }
        RuntimeProjection::ControlRestore { to_cursor } => {
            let bytes: Vec<u8> = tx.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = x'00'", [RUNTIME_CONTROL_PROJECTION], |row| row.get(0))?;
            let mut history: RuntimeControlHistory =
                serde_json::from_slice(&bytes).map_err(|error| {
                    StoreError::InvalidData(format!("invalid control history: {error}"))
                })?;
            let snapshot = history.snapshots.get(*to_cursor).cloned().ok_or_else(|| {
                StoreError::InvalidData("control history cursor is invalid".into())
            })?;
            restore_control_snapshot(tx, &snapshot)?;
            history.cursor = *to_cursor;
            projection::put_record(
                tx,
                RUNTIME_CONTROL_PROJECTION,
                1,
                &[0],
                &serde_json::to_vec(&history)
                    .map_err(|error| StoreError::InvalidData(error.to_string()))?,
                seq,
            )
        }
        RuntimeProjection::Raw(update) => projection::put_record(
            tx,
            &update.projection_name,
            update.reducer_version,
            &update.key,
            &update.value,
            seq,
        ),
    }
}
fn capture_control_snapshot(connection: &Connection) -> Result<RuntimeControlSnapshot, StoreError> {
    let states = {
        let mut s = connection.prepare("SELECT s.state_id,s.created_seq,s.kind,s.git_algorithm,s.git_oid,s.attempt_id,s.label,s.message,s.archived,s.last_event_seq,p.parent_state_id FROM states s LEFT JOIN state_logical_parents p ON p.child_state_id=s.state_id ORDER BY s.created_seq")?;
        s.query_map([], |r| {
            Ok(RuntimeStateProjection {
                state_id: r.get(0)?,
                created_seq: r.get(1)?,
                kind: r.get(2)?,
                git_algorithm: r.get(3)?,
                git_oid: r.get(4)?,
                attempt_id: r.get(5)?,
                label: r.get(6)?,
                message: r.get(7)?,
                archived: r.get(8)?,
                last_event_seq: r.get(9)?,
                logical_parent: r.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let attempts = {
        let mut s=connection.prepare("SELECT attempt_id,root_state_id,objective,current_tip_state_id,archived,last_event_seq FROM attempts ORDER BY attempt_id")?;
        s.query_map([], |r| {
            Ok(RuntimeAttemptProjection {
                attempt_id: r.get(0)?,
                root_state_id: r.get(1)?,
                objective: r.get(2)?,
                current_tip_state_id: r.get(3)?,
                archived: r.get(4)?,
                last_event_seq: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let worktrees = {
        let mut s=connection.prepare("SELECT worktree_id,attempt_id,active_state_id,relative_locator,head_oid,index_tree_oid,dirty_digest,last_event_seq FROM worktree_current ORDER BY worktree_id")?;
        s.query_map([], |r| {
            Ok(RuntimeWorktreeProjection {
                worktree_id: r.get(0)?,
                attempt_id: r.get(1)?,
                active_state_id: r.get(2)?,
                relative_locator: r.get(3)?,
                head_oid: r.get(4)?,
                index_tree_oid: r.get(5)?,
                dirty_digest: r.get(6)?,
                last_event_seq: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let provenance = {
        let mut s=connection.prepare("SELECT source_state_id,result_state_id,relation,provenance_id,created_seq FROM state_provenance_edges ORDER BY source_state_id,result_state_id,relation,provenance_id")?;
        s.query_map([], |r| {
            Ok(RuntimeProvenanceProjection {
                source_state_id: r.get(0)?,
                result_state_id: r.get(1)?,
                relation: r.get(2)?,
                provenance_id: r.get(3)?,
                created_seq: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let navigation = {
        let mut s=connection.prepare("SELECT record_key,record_value FROM projection_records WHERE projection_name=?1 ORDER BY record_key")?;
        s.query_map([RUNTIME_NAVIGATION_PROJECTION], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    Ok(RuntimeControlSnapshot {
        states,
        attempts,
        worktrees,
        provenance,
        navigation,
        git: None,
    })
}
fn ensure_initial_control_snapshot(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM projection_records WHERE projection_name=?1 AND record_key=x'00')", [RUNTIME_CONTROL_PROJECTION], |row| row.get(0))?;
    if !exists {
        let history = RuntimeControlHistory {
            snapshots: vec![capture_control_snapshot(tx)?],
            cursor: 0,
        };
        projection::put_record(
            tx,
            RUNTIME_CONTROL_PROJECTION,
            1,
            &[0],
            &serde_json::to_vec(&history)
                .map_err(|error| StoreError::InvalidData(error.to_string()))?,
            1,
        )?;
    }
    Ok(())
}
fn record_control_snapshot(
    tx: &rusqlite::Transaction<'_>,
    seq: u64,
    before: &RuntimeGitSnapshot,
    after: &RuntimeGitSnapshot,
) -> Result<(), StoreError> {
    let bytes: Option<Vec<u8>> = tx.query_row("SELECT record_value FROM projection_records WHERE projection_name=?1 AND record_key=x'00'", [RUNTIME_CONTROL_PROJECTION], |row| row.get(0)).optional()?;
    let mut history = match bytes {
        Some(bytes) => serde_json::from_slice::<RuntimeControlHistory>(&bytes)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?,
        None => RuntimeControlHistory {
            snapshots: Vec::new(),
            cursor: 0,
        },
    };
    if history.snapshots.is_empty() {
        let mut initial = capture_control_snapshot(tx)?;
        initial.git = Some(before.clone());
        history.snapshots.push(initial);
        history.cursor = 0;
    } else {
        history.snapshots.truncate(history.cursor + 1);
    }
    let mut next = capture_control_snapshot(tx)?;
    next.git = Some(after.clone());
    history.snapshots.push(next);
    history.cursor = history.snapshots.len() - 1;
    projection::put_record(
        tx,
        RUNTIME_CONTROL_PROJECTION,
        1,
        &[0],
        &serde_json::to_vec(&history)
            .map_err(|error| StoreError::InvalidData(error.to_string()))?,
        seq,
    )
}
fn restore_control_snapshot(
    tx: &rusqlite::Transaction<'_>,
    snapshot: &RuntimeControlSnapshot,
) -> Result<(), StoreError> {
    for table in [
        "worktree_current",
        "state_provenance_edges",
        "state_logical_parents",
        "attempts",
        "states",
    ] {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }
    for s in &snapshot.states {
        tx.execute("INSERT INTO states(state_id,created_seq,kind,git_algorithm,git_oid,attempt_id,label,message,archived,last_event_seq)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![s.state_id,s.created_seq,s.kind,s.git_algorithm,s.git_oid,s.attempt_id,s.label,s.message,s.archived,s.last_event_seq])?;
    }
    for a in &snapshot.attempts {
        tx.execute("INSERT INTO attempts(attempt_id,root_state_id,objective,current_tip_state_id,archived,last_event_seq)VALUES(?1,?2,?3,?4,?5,?6)",params![a.attempt_id,a.root_state_id,a.objective,a.current_tip_state_id,a.archived,a.last_event_seq])?;
    }
    for s in &snapshot.states {
        if let Some(parent) = &s.logical_parent {
            tx.execute("INSERT INTO state_logical_parents(child_state_id,parent_state_id,created_seq)VALUES(?1,?2,?3)",params![s.state_id,parent,s.created_seq])?;
        }
    }
    for p in &snapshot.provenance {
        tx.execute("INSERT INTO state_provenance_edges(source_state_id,result_state_id,relation,provenance_id,created_seq)VALUES(?1,?2,?3,?4,?5)",params![p.source_state_id,p.result_state_id,p.relation,p.provenance_id,p.created_seq])?;
    }
    for w in &snapshot.worktrees {
        tx.execute("INSERT INTO worktree_current(worktree_id,attempt_id,active_state_id,relative_locator,head_oid,index_tree_oid,dirty_digest,last_event_seq)VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![w.worktree_id,w.attempt_id,w.active_state_id,w.relative_locator,w.head_oid,w.index_tree_oid,w.dirty_digest,w.last_event_seq])?;
    }
    tx.execute(
        "DELETE FROM projection_records WHERE projection_name=?1",
        [RUNTIME_NAVIGATION_PROJECTION],
    )?;
    for (key, value) in &snapshot.navigation {
        projection::put_record(tx, RUNTIME_NAVIGATION_PROJECTION, 1, key, value, 1)?;
    }
    Ok(())
}
fn runtime_state_from_projection(
    s: &RuntimeStateProjection,
) -> Result<RuntimeStateRow, StoreError> {
    Ok(RuntimeStateRow {
        state_id: hex::encode_upper(&s.state_id),
        git_oid: s.git_oid.clone(),
        kind: s.kind.clone(),
        label: s.label.clone(),
        message: s.message.clone().unwrap_or_default(),
        attempt_id: hex::encode_upper(&s.attempt_id),
        logical_parent: s.logical_parent.as_ref().map(hex::encode_upper),
        created_seq: s.created_seq,
        archived: s.archived != 0,
    })
}

fn parse_hex_uuid(value: &str, field: &str) -> Result<Uuid, StoreError> {
    let bytes = hex::decode(value)
        .map_err(|error| StoreError::InvalidData(format!("invalid {field}: {error}")))?;
    Uuid::from_slice(&bytes)
        .map_err(|error| StoreError::InvalidData(format!("invalid {field}: {error}")))
}

fn verify_connection(connection: &Connection, label: &str) -> Result<(), StoreError> {
    let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(StoreError::InvalidData(format!(
            "{label} integrity_check: {result}"
        )));
    }
    let foreign_key_failure: Option<String> = connection
        .query_row(
            "SELECT CAST(\"table\" AS TEXT) FROM pragma_foreign_key_check LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(table) = foreign_key_failure {
        return Err(StoreError::InvalidData(format!(
            "{label} foreign key violation in {table}"
        )));
    }
    Ok(())
}

fn backup_info(connection: &Connection, path: &Path) -> Result<BackupInfo, StoreError> {
    verify_connection(connection, "backup")?;
    let application_id = connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(StoreError::WrongApplicationId(application_id));
    }
    let schema_version = connection.query_row(
        "SELECT storage_schema_version FROM journal_meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if schema_version != STORAGE_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(schema_version));
    }
    let head = journal::head_in(connection)?;
    Ok(BackupInfo {
        path: path.to_owned(),
        schema: SchemaInfo {
            application_id,
            schema_version,
            journal_mode: JournalMode::Wal,
        },
        head,
    })
}

fn attach_backup_git_snapshot_to(
    connection: &mut Connection,
    snapshot: &RuntimeGitSnapshot,
) -> Result<(), StoreError> {
    let head = journal::head_in(connection)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    projection::put_record(
        &tx,
        RUNTIME_BACKUP_GIT_PROJECTION,
        1,
        &[0],
        &serde_json::to_vec(snapshot).map_err(|error| {
            StoreError::InvalidData(format!("backup Git snapshot is not serializable: {error}"))
        })?,
        head.local_seq,
    )?;
    tx.commit()?;
    Ok(())
}

fn read_backup_git_snapshot_from(
    connection: &Connection,
) -> Result<RuntimeGitSnapshot, StoreError> {
    let bytes: Vec<u8> = connection.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = x'00'", [RUNTIME_BACKUP_GIT_PROJECTION], |row| row.get(0)).map_err(|error| match error { rusqlite::Error::QueryReturnedNoRows => StoreError::InvalidData("backup is missing required embedded Git snapshot".into()), other => StoreError::Sqlite(other) })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        StoreError::InvalidData(format!("invalid embedded backup Git snapshot: {error}"))
    })
}

fn attach_backup_git_bundle_to(
    connection: &mut Connection,
    bundle: &[u8],
) -> Result<(), StoreError> {
    if bundle.is_empty() {
        return Err(StoreError::InvalidData(
            "backup Git object bundle is empty".into(),
        ));
    }
    let head = journal::head_in(connection)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    projection::put_record(
        &tx,
        RUNTIME_BACKUP_GIT_BUNDLE_PROJECTION,
        1,
        &[0],
        bundle,
        head.local_seq,
    )?;
    tx.commit()?;
    Ok(())
}

fn read_backup_git_bundle_from(connection: &Connection) -> Result<Vec<u8>, StoreError> {
    let bytes: Vec<u8> = connection.query_row("SELECT record_value FROM projection_records WHERE projection_name = ?1 AND record_key = x'00'", [RUNTIME_BACKUP_GIT_BUNDLE_PROJECTION], |row| row.get(0)).map_err(|error| match error { rusqlite::Error::QueryReturnedNoRows => StoreError::InvalidData("backup is missing required embedded Git object bundle".into()), other => StoreError::Sqlite(other) })?;
    if bytes.is_empty() {
        return Err(StoreError::InvalidData(
            "embedded backup Git object bundle is empty".into(),
        ));
    }
    Ok(bytes)
}
fn backup_boundary(
    connection: &Connection,
) -> Result<crate::app::command::backup::BackupBoundary, StoreError> {
    verify_connection(connection, "backup")?;
    let application_id: i32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != APPLICATION_ID {
        return Err(StoreError::WrongApplicationId(application_id));
    }
    let schema_version: u32 = connection.query_row(
        "SELECT storage_schema_version FROM journal_meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if schema_version != STORAGE_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(schema_version));
    }
    let repo_bytes: Vec<u8> = connection.query_row(
        "SELECT repo_id FROM journal_meta WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    let repo_uuid = Uuid::from_slice(&repo_bytes)
        .map_err(|error| StoreError::InvalidData(format!("invalid repository id: {error}")))?;
    let repository_id = crate::domain::id::RepoId::from_uuid(repo_uuid)
        .map_err(|error| StoreError::InvalidData(error.to_string()))?
        .to_string();

    let mut migration_set = Sha256::new();
    let mut migrations = connection
        .prepare("SELECT version, name, checksum FROM schema_migrations ORDER BY version")?;
    let rows = migrations.query_map([], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Vec<u8>>(2)?,
        ))
    })?;
    for row in rows {
        let (version, name, checksum) = row?;
        migration_set.update(version.to_be_bytes());
        migration_set.update((name.len() as u64).to_be_bytes());
        migration_set.update(name.as_bytes());
        migration_set.update((checksum.len() as u64).to_be_bytes());
        migration_set.update(checksum);
    }

    let head = journal::head_in(connection)?;
    let mut pending = connection.prepare(
        "SELECT hex(operation_id), status, last_event_seq FROM operations WHERE status NOT IN ('committed', 'aborted') ORDER BY prepared_seq, operation_id",
    )?;
    let pending_rows = pending
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let operation_boundary = if pending_rows.is_empty() {
        "all-terminal".to_owned()
    } else {
        let bytes = serde_json::to_vec(&pending_rows)
            .map_err(|error| StoreError::InvalidData(format!("operation boundary: {error}")))?;
        format!("pending-sha256:{}", hex::encode(Sha256::digest(bytes)))
    };
    Ok(crate::app::command::backup::BackupBoundary {
        repository_id,
        schema: crate::app::command::backup::SchemaIdentity {
            format: "jjk-store".into(),
            major: u16::try_from(schema_version)
                .map_err(|_| StoreError::InvalidData("schema version exceeds u16".into()))?,
            minor: 0,
            migration_set_sha256: hex::encode(migration_set.finalize()),
        },
        journal_head: crate::app::command::backup::JournalHeadManifest {
            through_seq: head.local_seq,
            through_event_hash: hex::encode(head.event_hash),
        },
        operation_boundary,
    })
}

impl crate::app::command::backup::BackupStore for SqliteStore {
    fn online_backup_to(
        &self,
        destination: &Path,
    ) -> Result<crate::app::command::backup::BackupBoundary, String> {
        self.backup_to(destination)
            .map_err(|error| error.to_string())?;
        let connection = Connection::open_with_flags(destination, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
        backup_boundary(&connection).map_err(|error| error.to_string())
    }

    fn verify_backup(
        &self,
        database: &Path,
    ) -> Result<crate::app::command::backup::BackupBoundary, String> {
        let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| error.to_string())?;
        backup_boundary(&connection).map_err(|error| error.to_string())
    }
}

impl crate::app::transaction::RepositoryStore for SqliteStore {
    type Error = StoreError;
    type Projection = RuntimeProjection;

    fn head(&self) -> Result<JournalHead, Self::Error> {
        <Self as crate::ports::journal::Journal>::head(self)
    }

    fn prepare(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation: &PreparedOperationRecord,
    ) -> Result<OperationRecord, Self::Error> {
        Self::prepare(self, expected_head, event, operation)
    }

    fn record_transition(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation_id: Uuid,
        status: OperationStatus,
        result: Option<&[u8]>,
    ) -> Result<OperationRecord, Self::Error> {
        Self::transition_operation(self, expected_head, event, operation_id, status, result)
    }

    fn commit_verified(
        &mut self,
        expected_head: JournalHead,
        events: &[EventRecord],
        projections: &[RuntimeProjection],
        operation_id: Uuid,
        result: &[u8],
    ) -> Result<OperationRecord, Self::Error> {
        if events.is_empty() {
            return Err(StoreError::InvalidData(
                "verified commit requires a terminal event".into(),
            ));
        }
        let current = operation::get(&self.connection, operation_id)?.ok_or_else(|| {
            StoreError::InvalidData(format!("operation {operation_id} does not exist"))
        })?;
        if current.status == OperationStatus::Committed {
            return Ok(current);
        }
        if current.status != OperationStatus::Verifying {
            return Err(StoreError::InvalidOperationTransition {
                from: current.status,
                to: OperationStatus::Committed,
            });
        }
        let mut expected_ordinal = self.connection.query_row(
            "SELECT COUNT(*) FROM events WHERE operation_id = ?1",
            params![operation_id.as_bytes()],
            |row| row.get::<_, u32>(0),
        )?;
        for event in events {
            if event.operation_id != operation_id || event.operation_ordinal != expected_ordinal {
                return Err(StoreError::InvalidData(format!(
                    "operation terminal event ordinal must be {expected_ordinal}, found {}",
                    event.operation_ordinal
                )));
            }
            expected_ordinal += 1;
        }
        self.append_atomic(expected_head, events, |tx, sequences| {
            let seq = *sequences.first().expect("events checked non-empty");
            for update in projections {
                apply_runtime_projection(tx, update, seq)?;
            }
            operation::transition(
                tx,
                operation_id,
                OperationStatus::Committed,
                *sequences.last().expect("events checked non-empty"),
                Some(result),
            )
        })?;
        operation::get(&self.connection, operation_id)?
            .ok_or_else(|| StoreError::InvalidData("committed operation disappeared".into()))
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::journal::{ActorKind, GENESIS_HASH, Journal, PayloadCodec};
    use crate::ports::operation::{OperationStore, RecoveryDisposition};
    use crate::ports::projection::ProjectionStore;

    fn store() -> (tempfile::TempDir, SqliteStore, Uuid) {
        let directory = tempfile::tempdir().expect("tempdir");
        let repo_id = Uuid::now_v7();
        let store = SqliteStore::open(
            directory.path(),
            repo_id,
            b"root-token",
            "2026-08-28T00:00:00Z",
            StoreOpenOptions::default(),
        )
        .expect("open store");
        (directory, store, repo_id)
    }

    fn event(repo_id: Uuid, operation_id: Uuid, ordinal: u32, head: JournalHead) -> EventRecord {
        let mut event_hash = [0_u8; 32];
        event_hash[..16].copy_from_slice(Uuid::now_v7().as_bytes());
        event_hash[31] = 1;
        EventRecord {
            event_id: Uuid::now_v7(),
            repo_id,
            event_type: "TestEvent".into(),
            event_schema_version: 1,
            envelope_version: ENVELOPE_VERSION,
            operation_id,
            operation_ordinal: ordinal,
            actor_id: Uuid::now_v7(),
            actor_kind: ActorKind::System,
            recorded_at_utc: "2026-08-28T00:00:00Z".into(),
            observed_at_utc: None,
            repository_fingerprint: vec![1],
            payload_codec: PayloadCodec::CanonicalCborV1,
            payload: vec![],
            provenance: vec![],
            evidence_manifest: vec![],
            dedup_key: None,
            previous_event_hash: head.event_hash,
            event_hash,
        }
    }
    #[test]
    fn current_orientation_reads_are_bounded_with_large_history() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let (_directory, mut store, repo_id) = store();
        let workspace_id = Uuid::now_v7();
        let attempt_id = Uuid::now_v7();
        let mut parent = None;
        for index in 0..1_000_u64 {
            let head = Journal::head(&store).expect("journal head");
            let operation_id = Uuid::now_v7();
            let state_id = Uuid::now_v7();
            let state = RuntimeStateInsert {
                state_id,
                attempt_id,
                logical_parent: parent,
                workspace_id,
                git_algorithm: "sha1".into(),
                git_oid: format!("{index:040x}"),
                head_oid: Some(format!("{index:040x}")),
                kind: "checkpoint".into(),
                label: format!("state-{index}"),
                message: None,
                relative_locator: Vec::new(),
            };
            store
                .append_runtime_state(&event(repo_id, operation_id, 0, head), &state)
                .expect("append state");
            parent = Some(state_id);
        }

        let vm_steps = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&vm_steps);
        store.connection.progress_handler(
            1,
            Some(move || {
                count.fetch_add(1, Ordering::Relaxed);
                false
            }),
        );
        let current = store
            .current_state_row(workspace_id)
            .expect("read current state")
            .expect("current state");
        store.connection.progress_handler(0, None::<fn() -> bool>);

        assert_eq!(current.git_oid, format!("{:040x}", 999));
        let steps = vm_steps.load(Ordering::Relaxed);
        assert!(
            steps < 200,
            "current-state lookup used {steps} SQLite VM steps for 1,000 states"
        );
    }
    #[test]
    fn online_backup_contract_reports_verified_boundary() {
        use crate::app::command::backup::BackupStore;

        let (directory, store, repo_id) = store();
        let backup = directory.path().join("backup/state.sqlite3");
        let created = store.online_backup_to(&backup).expect("online backup");
        let verified = store.verify_backup(&backup).expect("verified backup");
        assert_eq!(created, verified);
        assert_eq!(
            created
                .repository_id
                .parse::<crate::domain::id::RepoId>()
                .unwrap()
                .as_uuid(),
            repo_id
        );
        assert_eq!(created.schema.format, "jjk-store");
        assert_eq!(created.schema.major, 1);
        assert_eq!(created.schema.migration_set_sha256.len(), 64);
        assert_eq!(created.journal_head.through_seq, 0);
        assert_eq!(created.operation_boundary, "all-terminal");
    }

    fn prepared(operation_id: Uuid, request_byte: u8) -> PreparedOperationRecord {
        PreparedOperationRecord {
            operation_id,
            request_hash: [request_byte; 32],
            command_kind: "test".into(),
            precondition_fingerprint: vec![1],
            expected_effects: vec![2],
            recovery_artifact_hash: None,
        }
    }

    #[test]
    fn rejects_stale_head_without_appending() {
        let (_directory, mut store, repo_id) = store();
        let original = JournalHead {
            local_seq: 0,
            event_hash: GENESIS_HASH,
        };
        store
            .append_atomic(
                original,
                &[event(repo_id, Uuid::now_v7(), 0, original)],
                |_tx, _| Ok(()),
            )
            .expect("first append");
        let stale = event(repo_id, Uuid::now_v7(), 0, original);
        assert!(matches!(
            store.append_atomic(original, &[stale], |_tx, _| Ok(())),
            Err(StoreError::HeadChanged {
                expected_seq: 0,
                actual_seq: 1
            })
        ));
        assert_eq!(Journal::head(&store).expect("head").local_seq, 1);
    }

    #[test]
    fn reducer_failure_rolls_back_event_and_projection() {
        let (_directory, mut store, repo_id) = store();
        let head = Journal::head(&store).expect("head");
        let record = event(repo_id, Uuid::now_v7(), 0, head);
        let result = store.append_atomic(head, &[record], |tx, sequences| {
            projection::put_record(tx, "test", 1, b"key", b"value", sequences[0])?;
            Err(StoreError::InvalidData("injected reducer failure".into()))
        });
        assert!(matches!(result, Err(StoreError::InvalidData(_))));
        assert_eq!(Journal::head(&store).expect("head"), head);
        assert!(matches!(
            store.projection_snapshot("test", 1),
            Err(StoreError::ProjectionStale { .. })
        ));
    }

    #[test]
    fn prepare_retry_is_idempotent_and_conflicting_request_is_rejected() {
        let (_directory, mut store, repo_id) = store();
        let operation_id = Uuid::now_v7();
        let initial_head = Journal::head(&store).expect("head");
        let prepared_event = event(repo_id, operation_id, 0, initial_head);
        let request = prepared(operation_id, 7);
        let first = store
            .prepare(initial_head, &prepared_event, &request)
            .expect("prepare");
        assert_eq!(
            store
                .prepare(initial_head, &prepared_event, &request)
                .expect("retry"),
            first
        );
        assert_eq!(Journal::head(&store).expect("head").local_seq, 1);
        assert!(
            matches!(store.prepare(initial_head, &prepared_event, &prepared(operation_id, 8)), Err(StoreError::IdempotencyConflict { operation_id: id }) if id == operation_id)
        );
    }

    #[test]
    fn invalid_transition_is_rejected_without_a_lifecycle_event() {
        let (_directory, mut store, repo_id) = store();
        let operation_id = Uuid::now_v7();
        let head = Journal::head(&store).expect("head");
        store
            .prepare(
                head,
                &event(repo_id, operation_id, 0, head),
                &prepared(operation_id, 4),
            )
            .expect("prepare");
        let head = Journal::head(&store).expect("head");
        let committed = event(repo_id, operation_id, 1, head);
        assert!(matches!(
            store.transition_operation(
                head,
                &committed,
                operation_id,
                OperationStatus::Committed,
                None
            ),
            Err(StoreError::InvalidOperationTransition {
                from: OperationStatus::Prepared,
                to: OperationStatus::Committed
            })
        ));
        assert_eq!(Journal::head(&store).expect("head"), head);
    }

    #[test]
    fn prepared_operation_is_discovered_for_recovery() {
        let (_directory, mut store, repo_id) = store();
        let operation_id = Uuid::now_v7();
        let head = Journal::head(&store).expect("head");
        store
            .prepare(
                head,
                &event(repo_id, operation_id, 0, head),
                &prepared(operation_id, 3),
            )
            .expect("prepare");
        let candidates = store.recovery_candidates().expect("recovery candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].operation.operation_id, operation_id);
        assert_eq!(
            candidates[0].disposition,
            RecoveryDisposition::AbortUnapplied
        );
    }

    #[test]
    fn migration_is_idempotent_across_reopen() {
        let (directory, store, repo_id) = store();
        drop(store);
        let reopened = SqliteStore::open(
            directory.path(),
            repo_id,
            b"root-token",
            "2026-08-28T00:00:01Z",
            StoreOpenOptions::default(),
        )
        .expect("reopen migrated store");
        assert_eq!(
            reopened.schema_info().expect("schema").schema_version,
            STORAGE_SCHEMA_VERSION
        );
        reopened.integrity_check().expect("integrity");
    }
}
