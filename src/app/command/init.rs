//! Initialization-time one-way legacy migration orchestration.

use std::path::{Path, PathBuf};

use crate::adapters::legacy::repo_json::{
    GitObjectLookup, LegacyImportError, LegacyImportPlan, LegacyImportSink, LegacyMigrationReceipt,
    LegacyRecoveryOutcome, LegacyRollbackManifest, PreparedLegacyImport, recover_legacy_capsule,
};

#[derive(Clone, Debug)]
pub(crate) struct LegacyMigrationPreview {
    pub migration_id: String,
    pub source_id: String,
    pub input_sha256: String,
    pub source_files: usize,
    pub source_bytes: u64,
    pub entity_counts: std::collections::BTreeMap<String, u64>,
    pub entities: usize,
    pub quarantine_counts: std::collections::BTreeMap<String, u64>,
    pub quarantined: usize,
    pub warnings: Vec<String>,
    pub required_oids: Vec<String>,
    pub rollback_capsule: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedLegacyMigration {
    plan: LegacyImportPlan,
    preview: LegacyMigrationPreview,
}

impl PreparedLegacyMigration {
    pub(crate) fn preview(&self) -> &LegacyMigrationPreview {
        &self.preview
    }
}

pub(crate) fn prepare_legacy_import(
    prepared: &PreparedLegacyMigration,
) -> Result<PreparedLegacyImport, LegacyImportError> {
    prepared.plan.prepare_import()
}

pub(crate) fn preserve_legacy_sources(
    prepared: &PreparedLegacyMigration,
) -> Result<LegacyRollbackManifest, LegacyImportError> {
    prepared
        .plan
        .preserve_sources(&prepared.preview.rollback_capsule)
}

pub(crate) fn inspect_legacy_migration_receipt(
    prepared: &PreparedLegacyMigration,
    receipt: Option<LegacyMigrationReceipt>,
) -> Result<LegacyMigrationStatus, LegacyImportError> {
    match receipt {
        None => Ok(LegacyMigrationStatus::Pending),
        Some(receipt) if receipt.input_sha256 == prepared.plan.input_sha256 => {
            Ok(LegacyMigrationStatus::AlreadyApplied(receipt))
        }
        Some(_) => Err(LegacyImportError::Invalid {
            path: "migration receipt".into(),
            message: "legacy source identity was already imported from different bytes".into(),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LegacyMigrationStatus {
    Pending,
    AlreadyApplied(LegacyMigrationReceipt),
}

pub(crate) fn detect_legacy_metadata(repository_root: &Path) -> bool {
    repository_root.join(".jjk/repo.json").is_file()
}

pub(crate) fn preview_legacy_migration(
    repository_root: &Path,
    control_root: &Path,
    git: &dyn GitObjectLookup,
) -> Result<PreparedLegacyMigration, LegacyImportError> {
    let plan = LegacyImportPlan::discover(repository_root, git)?;
    let rollback_capsule = control_root
        .join("migrations/legacy-v1")
        .join(&plan.migration_id);
    let mut entity_counts = std::collections::BTreeMap::new();
    for entity in &plan.entities {
        *entity_counts.entry(entity.entity_kind.clone()).or_insert(0) += 1;
    }
    let mut quarantine_counts = std::collections::BTreeMap::new();
    for record in &plan.quarantined {
        *quarantine_counts
            .entry(record.entity_kind.clone())
            .or_insert(0) += 1;
    }
    let preview = LegacyMigrationPreview {
        migration_id: plan.migration_id.clone(),
        source_id: plan.source_id.clone(),
        input_sha256: plan.input_sha256.clone(),
        source_files: plan.files.len(),
        source_bytes: plan.files.iter().map(|file| file.size_bytes).sum(),
        entity_counts,
        entities: plan.entities.len(),
        quarantine_counts,
        quarantined: plan.quarantined.len(),
        warnings: plan.warnings.clone(),
        required_oids: plan.required_oids.clone(),
        rollback_capsule,
    };
    Ok(PreparedLegacyMigration { plan, preview })
}

pub(crate) fn inspect_legacy_migration_status(
    prepared: &PreparedLegacyMigration,
    sink: &mut dyn LegacyImportSink,
) -> Result<LegacyMigrationStatus, LegacyImportError> {
    let receipt = sink
        .existing_receipt(&prepared.plan.migration_id)
        .map_err(LegacyImportError::Sink)?;
    inspect_legacy_migration_receipt(prepared, receipt)
}

pub(crate) fn apply_legacy_migration(
    prepared: &PreparedLegacyMigration,
    sink: &mut dyn LegacyImportSink,
) -> Result<LegacyMigrationReceipt, LegacyImportError> {
    prepared
        .plan
        .preserve_sources(&prepared.preview.rollback_capsule)?;
    prepared.plan.apply(sink)
}

pub(crate) fn recover_legacy_assets(
    rollback_capsule: &Path,
    recovery_root: &Path,
) -> Result<LegacyRecoveryOutcome, LegacyImportError> {
    recover_legacy_capsule(rollback_capsule, recovery_root)
}
