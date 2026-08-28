//! Integrity and compatibility diagnostics for stores and artifacts.

use std::path::Path;
use super::backup::{BackupStore, GitBundleVerifier, inspect_freeze, preview_load, verify_backup_artifact};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckStatus { Pass, Warn, Fail, Unsupported }
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntegrityCheck { pub name: String, pub status: CheckStatus, pub detail: String }
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DoctorReport { pub checks: Vec<IntegrityCheck> }
impl DoctorReport {
    pub(crate) fn is_healthy(&self) -> bool { self.checks.iter().all(|c| !matches!(c.status, CheckStatus::Fail | CheckStatus::Unsupported)) }
    fn record(&mut self, name: &str, result: Result<(), String>) { self.checks.push(match result { Ok(()) => IntegrityCheck { name: name.into(), status: CheckStatus::Pass, detail: "verified".into() }, Err(detail) => IntegrityCheck { name: name.into(), status: CheckStatus::Fail, detail } }); }
}

pub(crate) fn doctor_store(store: &dyn BackupStore, database: &Path) -> DoctorReport {
    let mut report = DoctorReport::default();
    report.record("sqlite-integrity", store.verify_backup(database));
    report
}

pub(crate) fn doctor_backup(artifact: &Path, scratch: &Path, git: &dyn GitBundleVerifier) -> DoctorReport {
    let mut report = DoctorReport::default();
    report.record("backup-checksums", verify_backup_artifact(artifact).map(|_| ()).map_err(|e| e.to_string()));
    match preview_load(artifact, scratch, git) {
        Ok(preview) => report.record("git-object-closure", if preview.missing_oids.is_empty() { Ok(()) } else { Err(format!("missing required OIDs: {:?}", preview.missing_oids)) }),
        Err(error) => report.record("backup-load-preview", Err(error.to_string())),
    }
    report
}

pub(crate) fn doctor_freeze(artifact: &Path, git: &dyn GitBundleVerifier) -> DoctorReport {
    let mut report = DoctorReport::default();
    match inspect_freeze(artifact) {
        Ok(manifest) => match git.verify_bundle(&artifact.join("git/objects.bundle")) {
            Ok(objects) => { let known = objects.into_iter().collect::<std::collections::BTreeSet<_>>(); let missing = manifest.required_oids.iter().filter(|oid| !known.contains(*oid)).collect::<Vec<_>>(); report.record("freeze-integrity", if missing.is_empty() { Ok(()) } else { Err(format!("missing required OIDs: {missing:?}")) }); }
            Err(error) => report.record("freeze-integrity", Err(error)),
        },
        Err(error) => report.record("freeze-integrity", Err(error.to_string())),
    }
    report
}

pub(crate) fn compatibility_check(store_major: u16, store_minor: u16, supported_major: u16, max_write_minor: u16) -> IntegrityCheck {
    if store_major != supported_major { return IntegrityCheck { name: "schema-compatibility".into(), status: CheckStatus::Unsupported, detail: format!("store schema {store_major}.{store_minor} is incompatible with major {supported_major}") }; }
    if store_minor > max_write_minor { return IntegrityCheck { name: "schema-compatibility".into(), status: CheckStatus::Warn, detail: format!("schema is readable but newer than write-safe minor {max_write_minor}; mutations disabled") }; }
    IntegrityCheck { name: "schema-compatibility".into(), status: CheckStatus::Pass, detail: format!("schema {store_major}.{store_minor} is write-compatible") }
}
