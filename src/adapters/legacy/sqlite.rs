//! Production adapters connecting legacy import to Git plumbing and the SQLite store.

use std::path::Path;

use crate::adapters::git::GitCli;
use crate::adapters::sqlite::{RuntimeProjection, SqliteStore};
use crate::ports::process::ProcessRunner;

use super::repo_json::{
    GitObjectLookup, LegacyBundleValidation, LegacyMigrationReceipt, PreparedLegacyImport,
};

const RECORD_KIND: &str = "legacy-migration";

/// Read-only legacy object verifier backed by bounded native Git plumbing.
pub(crate) struct GitLegacyLookup<'a, R> {
    git: &'a GitCli<R>,
    repository_root: &'a Path,
}

impl<'a, R> GitLegacyLookup<'a, R> {
    pub(crate) fn new(git: &'a GitCli<R>, repository_root: &'a Path) -> Self {
        Self {
            git,
            repository_root,
        }
    }
}

impl<R: ProcessRunner> GitObjectLookup for GitLegacyLookup<'_, R> {
    fn object_exists(&self, oid: &str) -> Result<bool, String> {
        let output = self
            .git
            .run(
                self.repository_root,
                ["cat-file", "-e", &format!("{oid}^{{object}}")],
            )
            .map_err(|error| error.to_string())?;
        match output.exit_code {
            0 => Ok(true),
            1 | 128 => Ok(false),
            code => Err(format!(
                "Git object lookup exited {code}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        }
    }

    fn validate_bundle(&self, bundle: &Path) -> Result<LegacyBundleValidation, String> {
        let verify = self
            .git
            .run(
                self.repository_root,
                ["bundle".as_ref(), "verify".as_ref(), bundle.as_os_str()],
            )
            .map_err(|error| error.to_string())?;
        if verify.exit_code != 0 {
            return Ok(LegacyBundleValidation::Unavailable {
                reason: format!(
                    "git bundle verify exited {}: {}",
                    verify.exit_code,
                    String::from_utf8_lossy(&verify.stderr).trim()
                ),
            });
        }
        let heads = self
            .git
            .run(
                self.repository_root,
                ["bundle".as_ref(), "list-heads".as_ref(), bundle.as_os_str()],
            )
            .map_err(|error| error.to_string())?;
        if heads.exit_code != 0 {
            return Ok(LegacyBundleValidation::Unavailable {
                reason: format!(
                    "git bundle list-heads exited {}: {}",
                    heads.exit_code,
                    String::from_utf8_lossy(&heads.stderr).trim()
                ),
            });
        }
        let advertised_oids = heads
            .stdout
            .split(|byte| *byte == b'\n')
            .filter_map(|line| line.split(u8::is_ascii_whitespace).next())
            .filter(|oid| matches!(oid.len(), 40 | 64) && oid.iter().all(u8::is_ascii_hexdigit))
            .map(|oid| String::from_utf8_lossy(oid).into_owned())
            .collect();
        Ok(LegacyBundleValidation::Verified { advertised_oids })
    }
}

pub(crate) fn legacy_import_projection(
    prepared: &PreparedLegacyImport,
) -> Result<RuntimeProjection, String> {
    let bytes = serde_json::to_vec(prepared).map_err(|error| error.to_string())?;
    Ok(RuntimeProjection::Record {
        kind: RECORD_KIND.to_owned(),
        id: prepared.receipt.migration_id.clone(),
        value: bytes,
    })
}

pub(crate) fn existing_legacy_receipt(
    store: &SqliteStore,
    migration_id: &str,
) -> Result<Option<LegacyMigrationReceipt>, String> {
    store
        .runtime_record(RECORD_KIND, migration_id)
        .map_err(|error| error.to_string())?
        .map(|bytes| {
            serde_json::from_slice::<PreparedLegacyImport>(&bytes)
                .map(|stored| stored.receipt)
                .map_err(|error| {
                    format!("invalid stored legacy migration `{migration_id}`: {error}")
                })
        })
        .transpose()
}
