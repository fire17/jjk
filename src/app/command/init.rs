//! Initialization-time one-way legacy migration orchestration.

use std::path::{Path, PathBuf};

use crate::adapters::legacy::repo_json::{
    GitObjectLookup, LegacyImportError, LegacyImportPlan, LegacyImportSink,
    LegacyMigrationReceipt,
};

#[derive(Clone, Debug)]
pub(crate) struct LegacyMigrationPreview {
    pub migration_id: String,
    pub source_id: String,
    pub source_files: usize,
    pub source_bytes: u64,
    pub entities: usize,
    pub quarantined: usize,
    pub warnings: Vec<String>,
    pub required_oids: Vec<String>,
    pub rollback_capsule: PathBuf,
}

pub(crate) fn preview_legacy_migration(
    repository_root: &Path,
    git: &dyn GitObjectLookup,
) -> Result<(LegacyImportPlan, LegacyMigrationPreview), LegacyImportError> {
    let plan = LegacyImportPlan::discover(repository_root, git)?;
    let rollback_capsule = repository_root
        .join(".jjk/migrations/legacy-v1")
        .join(&plan.migration_id);
    let preview = LegacyMigrationPreview {
        migration_id: plan.migration_id.clone(),
        source_id: plan.source_id.clone(),
        source_files: plan.files.len(),
        source_bytes: plan.files.iter().map(|file| file.size_bytes).sum(),
        entities: plan.entities.len(),
        quarantined: plan.quarantined.len(),
        warnings: plan.warnings.clone(),
        required_oids: plan.required_oids.clone(),
        rollback_capsule,
    };
    Ok((plan, preview))
}

pub(crate) fn apply_legacy_migration(
    plan: &LegacyImportPlan,
    rollback_capsule: &Path,
    sink: &mut dyn LegacyImportSink,
) -> Result<LegacyMigrationReceipt, LegacyImportError> {
    plan.preserve_sources(rollback_capsule)?;
    plan.apply(sink)
}
