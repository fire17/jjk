mod journal;
mod migrate;
mod operation;
mod projection;
mod row;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use thiserror::Error;
use uuid::Uuid;

use crate::ports::journal::{EventRecord, JournalHead};
use crate::ports::operation::{OperationRecord, OperationStatus, PreparedOperationRecord};

pub(crate) const APPLICATION_ID: i32 = 0x4A4A_4B31;
pub(crate) const STORAGE_SCHEMA_VERSION: u32 = 1;
pub(crate) const ENVELOPE_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum JournalMode {
    Wal,
    Delete,
}

#[derive(Clone, Debug)]
pub(crate) struct StoreOpenOptions {
    pub journal_mode: JournalMode,
    pub busy_timeout: Duration,
}

impl Default for StoreOpenOptions {
    fn default() -> Self {
        Self {
            journal_mode: JournalMode::Wal,
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
    InvalidOperationTransition { from: OperationStatus, to: OperationStatus },
    #[error("projection {name} is stale: journal is at {journal_seq}, projection is at {projection_seq}")]
    ProjectionStale { name: String, journal_seq: u64, projection_seq: u64 },
    #[error("projection {name} reducer version {actual} is incompatible with required version {expected}")]
    ReducerVersionMismatch { name: String, expected: u32, actual: u32 },
    #[error("migration checksum mismatch for version {version}")]
    MigrationChecksumMismatch { version: u32 },
    #[error("unsupported SQLite application id {0:#x}")]
    WrongApplicationId(i32),
    #[error("unsupported storage schema version {0}")]
    UnsupportedSchema(u32),
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
        let mut store = Self { connection, path, mode: options.journal_mode };
        store.configure(options.busy_timeout)?;
        migrate::run(&mut store.connection, created_at_utc)?;
        store.initialize_or_verify(repo_id, repository_root_token, created_at_utc)?;
        Ok(store)
    }

    fn configure(&mut self, busy_timeout: Duration) -> Result<(), StoreError> {
        self.connection.busy_timeout(busy_timeout)?;
        self.connection.pragma_update(None, "foreign_keys", true)?;
        self.connection.pragma_update(None, "trusted_schema", false)?;
        self.connection.pragma_update(None, "synchronous", "FULL")?;
        self.connection.pragma_update(None, "wal_autocheckpoint", 1000)?;
        let requested = match self.mode { JournalMode::Wal => "WAL", JournalMode::Delete => "DELETE" };
        let actual: String = self.connection.pragma_update_and_check(None, "journal_mode", requested, |row| row.get(0))?;
        self.mode = match actual.to_ascii_lowercase().as_str() {
            "wal" => JournalMode::Wal,
            "delete" => JournalMode::Delete,
            other => return Err(StoreError::InvalidData(format!("SQLite selected unsupported journal mode {other}"))),
        };
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
                    return Err(StoreError::InvalidData("store belongs to a different safe space".into()));
                }
                if version != STORAGE_SCHEMA_VERSION {
                    return Err(StoreError::UnsupportedSchema(version));
                }
            }
        }
        let application_id: i32 = self.connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        if application_id != APPLICATION_ID { return Err(StoreError::WrongApplicationId(application_id)); }
        Ok(())
    }

    pub(crate) fn path(&self) -> &Path { &self.path }

    pub(crate) fn schema_info(&self) -> Result<SchemaInfo, StoreError> {
        let application_id = self.connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let schema_version = self.connection.query_row("SELECT storage_schema_version FROM journal_meta WHERE singleton = 1", [], |row| row.get(0))?;
        Ok(SchemaInfo { application_id, schema_version, journal_mode: self.mode })
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
        let tx = self.connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let actual_head = journal::head_in(&tx)?;
        if actual_head != expected_head {
            return Err(StoreError::HeadChanged { expected_seq: expected_head.local_seq, actual_seq: actual_head.local_seq });
        }
        let mut prior_hash = actual_head.event_hash;
        let mut sequences = Vec::with_capacity(events.len());
        for event in events {
            if event.previous_event_hash != prior_hash {
                return Err(StoreError::EventChainMismatch { ordinal: event.operation_ordinal });
            }
            let seq = journal::insert(&tx, event)?;
            sequences.push(seq);
            prior_hash = event.event_hash;
        }
        reduce(&tx, &sequences)?;
        projection::advance_all(&tx, sequences.last().copied().unwrap_or(actual_head.local_seq), prior_hash)?;
        tx.commit()?;
        Ok(JournalHead { local_seq: sequences.last().copied().unwrap_or(actual_head.local_seq), event_hash: prior_hash })
    }

    pub(crate) fn prepare(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        prepared: &PreparedOperationRecord,
    ) -> Result<OperationRecord, StoreError> {
        if let Some(existing) = operation::get(&self.connection, prepared.operation_id)? {
            if existing.request_hash == prepared.request_hash { return Ok(existing); }
            return Err(StoreError::IdempotencyConflict { operation_id: prepared.operation_id });
        }
        self.append_atomic(expected_head, std::slice::from_ref(event), |tx, seqs| operation::insert_prepared(tx, prepared, seqs[0]))?;
        operation::get(&self.connection, prepared.operation_id)?.ok_or_else(|| StoreError::InvalidData("prepared operation was not persisted".into()))
    }

    pub(crate) fn transition_operation(
        &mut self,
        expected_head: JournalHead,
        event: &EventRecord,
        operation_id: Uuid,
        to: OperationStatus,
        result: Option<&[u8]>,
    ) -> Result<OperationRecord, StoreError> {
        let current = operation::get(&self.connection, operation_id)?.ok_or_else(|| StoreError::InvalidData(format!("operation {operation_id} does not exist")))?;
        if !current.status.can_transition_to(to) {
            return Err(StoreError::InvalidOperationTransition { from: current.status, to });
        }
        self.append_atomic(expected_head, std::slice::from_ref(event), |tx, seqs| operation::transition(tx, operation_id, to, seqs[0], result))?;
        operation::get(&self.connection, operation_id)?.ok_or_else(|| StoreError::InvalidData("transitioned operation disappeared".into()))
    }

    pub(crate) fn integrity_check(&self) -> Result<(), StoreError> {
        let result: String = self.connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result != "ok" { return Err(StoreError::InvalidData(format!("integrity_check: {result}"))); }
        let foreign_key_failure: Option<String> = self.connection.query_row("SELECT CAST(table AS TEXT) FROM pragma_foreign_key_check LIMIT 1", [], |row| row.get(0)).optional()?;
        if let Some(table) = foreign_key_failure { return Err(StoreError::InvalidData(format!("foreign key violation in {table}"))); }
        Ok(())
    }

    pub(crate) fn backup_to(&self, destination: &Path) -> Result<BackupInfo, StoreError> {
        if let Some(parent) = destination.parent() { fs::create_dir_all(parent)?; }
        let mut target = Connection::open(destination)?;
        {
            let backup = rusqlite::backup::Backup::new(&self.connection, &mut target)?;
            backup.run_to_completion(128, Duration::from_millis(10), None)?;
        }
        let result: String = target.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if result != "ok" { return Err(StoreError::InvalidData(format!("backup integrity_check: {result}"))); }
        let application_id = target.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let schema_version = target.query_row("SELECT storage_schema_version FROM journal_meta WHERE singleton = 1", [], |row| row.get(0))?;
        let head = journal::head_in(&target)?;
        Ok(BackupInfo { path: destination.to_owned(), schema: SchemaInfo { application_id, schema_version, journal_mode: JournalMode::Delete }, head })
    }
}

impl crate::app::transaction::RepositoryStore for SqliteStore {
    type Error = StoreError;

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
        operation_id: Uuid,
        result: &[u8],
    ) -> Result<OperationRecord, Self::Error> {
        if events.is_empty() {
            return Err(StoreError::InvalidData("verified commit requires a terminal event".into()));
        }
        self.append_atomic(expected_head, events, |tx, sequences| {
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
