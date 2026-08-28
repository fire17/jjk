//! Integrity and compatibility diagnostics for stores and artifacts.

use super::backup::{BackupStore, GitBundleVerifier, inspect_freeze, verify_backup_artifact};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntegrityCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DoctorReport {
    pub checks: Vec<IntegrityCheck>,
}
impl DoctorReport {
    pub(crate) fn is_healthy(&self) -> bool {
        self.checks
            .iter()
            .all(|check| !matches!(check.status, CheckStatus::Fail | CheckStatus::Unsupported))
    }
    fn record(&mut self, name: &str, result: Result<String, String>) {
        self.checks.push(match result {
            Ok(detail) => IntegrityCheck {
                name: name.into(),
                status: CheckStatus::Pass,
                detail,
            },
            Err(detail) => IntegrityCheck {
                name: name.into(),
                status: CheckStatus::Fail,
                detail,
            },
        });
    }
}

pub(crate) fn doctor_store(store: &dyn BackupStore, database: &Path) -> DoctorReport {
    let mut report = DoctorReport::default();
    report.record(
        "sqlite-integrity",
        store.verify_backup(database).map(|boundary| {
            format!(
                "repository {}; journal through {}",
                boundary.repository_id, boundary.journal_head.through_seq
            )
        }),
    );
    report
}

pub(crate) fn doctor_backup(
    artifact: &Path,
    store: &dyn BackupStore,
    git: &dyn GitBundleVerifier,
) -> DoctorReport {
    let mut report = DoctorReport::default();
    match verify_backup_artifact(artifact, store, git) {
        Ok(verified) => {
            report.record(
                "backup-integrity",
                Ok(format!("{} bytes verified", verified.total_bytes)),
            );
            report.record(
                "git-object-closure",
                Ok(format!(
                    "{} required OIDs verified",
                    verified.manifest.required_oids.len()
                )),
            );
            report.checks.push(compatibility_check(
                verified.manifest.schema.major,
                verified.manifest.schema.minor,
                1,
                0,
            ));
        }
        Err(error) => report.record("backup-integrity", Err(error.to_string())),
    }
    report
}

pub(crate) fn doctor_freeze(artifact: &Path, git: &dyn GitBundleVerifier) -> DoctorReport {
    let mut report = DoctorReport::default();
    report.record(
        "freeze-integrity",
        inspect_freeze(artifact, git)
            .map(|manifest| format!("{} required OIDs verified", manifest.required_oids.len()))
            .map_err(|error| error.to_string()),
    );
    report
}

pub(crate) fn compatibility_check(
    store_major: u16,
    store_minor: u16,
    supported_major: u16,
    max_write_minor: u16,
) -> IntegrityCheck {
    if store_major != supported_major {
        return IntegrityCheck {
            name: "schema-compatibility".into(),
            status: CheckStatus::Unsupported,
            detail: format!(
                "store schema {store_major}.{store_minor} is incompatible with major {supported_major}; read-only diagnostics only"
            ),
        };
    }
    if store_minor > max_write_minor {
        return IntegrityCheck {
            name: "schema-compatibility".into(),
            status: CheckStatus::Warn,
            detail: format!(
                "schema is readable but newer than write-safe minor {max_write_minor}; mutations disabled"
            ),
        };
    }
    IntegrityCheck {
        name: "schema-compatibility".into(),
        status: CheckStatus::Pass,
        detail: format!("schema {store_major}.{store_minor} is write-compatible"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incompatible_major_is_unhealthy_and_read_only() {
        let check = compatibility_check(2, 0, 1, 0);
        assert_eq!(check.status, CheckStatus::Unsupported);
        assert!(check.detail.contains("read-only"));
        assert!(
            !DoctorReport {
                checks: vec![check]
            }
            .is_healthy()
        );
    }

    #[test]
    fn newer_minor_warns_and_disables_mutation_without_failing_read() {
        let check = compatibility_check(1, 2, 1, 0);
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.detail.contains("mutations disabled"));
        assert!(
            DoctorReport {
                checks: vec![check]
            }
            .is_healthy()
        );
    }
}
