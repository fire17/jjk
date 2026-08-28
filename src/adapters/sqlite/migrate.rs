use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::{APPLICATION_ID, STORAGE_SCHEMA_VERSION, StoreError};

const INITIAL_SQL: &str = include_str!("../../../migrations/0001_initial.sql");

pub(super) fn run(connection: &mut Connection, applied_at_utc: &str) -> Result<(), StoreError> {
    let application_id: i32 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != 0 && application_id != APPLICATION_ID {
        return Err(StoreError::WrongApplicationId(application_id));
    }
    let has_migrations: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    let checksum: [u8; 32] = Sha256::digest(INITIAL_SQL.as_bytes()).into();
    if has_migrations {
        let stored: Option<Vec<u8>> = connection
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match stored {
            Some(stored) if stored == checksum => return verify_version(connection),
            Some(_) => return Err(StoreError::MigrationChecksumMismatch { version: 1 }),
            None => {
                return Err(StoreError::InvalidData(
                    "schema_migrations exists without initial migration".into(),
                ));
            }
        }
    }
    let tx = connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
    tx.execute_batch(INITIAL_SQL)?;
    tx.pragma_update(None, "user_version", STORAGE_SCHEMA_VERSION)?;
    tx.execute(
        "INSERT INTO schema_migrations (version, name, checksum, applied_at_utc) VALUES (1, 'initial', ?1, ?2)",
        params![checksum.as_slice(), applied_at_utc],
    )?;
    tx.commit()?;
    verify_version(connection)
}

fn verify_version(connection: &Connection) -> Result<(), StoreError> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version != STORAGE_SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(version));
    }
    Ok(())
}
