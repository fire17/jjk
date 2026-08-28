//! Read-only importer for the legacy `.jjk/repo.json` version 1 store.
//!
//! The adapter deliberately owns no SQLite or Git implementation. It parses and
//! verifies legacy bytes, builds a deterministic import plan, and applies that
//! plan through [`LegacyImportSink`] in one caller-owned transaction.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const LEGACY_SCHEMA: &str = "jjk-repo-json-v1";
const TARGET_SCHEMA: &str = "jjk-store/1.0";
const ROLLBACK_MANIFEST: &str = ".jjk-legacy-rollback.json";

#[derive(Debug, thiserror::Error)]
pub(crate) enum LegacyImportError {
    #[error("legacy repo not found at {0}")]
    MissingRepo(PathBuf),
    #[error("cannot read legacy source {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid legacy JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("unsupported legacy schema version {0}; expected 1")]
    UnsupportedVersion(u64),
    #[error("invalid legacy data at {path}: {message}")]
    Invalid { path: String, message: String },
    #[error("legacy state {state_id} references missing Git objects: {oids:?}")]
    MissingGitObjects { state_id: String, oids: Vec<String> },
    #[error("legacy import sink rejected the plan: {0}")]
    Sink(String),
    #[error("cannot preserve legacy source {path}: {source}")]
    Preserve {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("preserved source digest mismatch for {0}")]
    PreserveDigest(PathBuf),
    #[error("invalid rollback capsule manifest in {path}: {message}")]
    CapsuleManifest { path: PathBuf, message: String },
    #[error("cannot recover legacy assets at {path}: {source}")]
    Recover {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("legacy recovery destination already contains different data: {0}")]
    RecoveryDestinationExists(PathBuf),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyRepoV1 {
    pub version: u64,
    pub safe_space_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub settings: LegacySettings,
    pub states: Vec<LegacyState>,
    pub lanes: BTreeMap<String, LegacyLane>,
    pub branch_lane_map: BTreeMap<String, String>,
    #[serde(default)]
    pub allow_main_branch_save: Option<bool>,
    #[serde(default)]
    pub return_context: Option<LegacyReturnContext>,
    #[serde(default)]
    pub current_state_history: Option<LegacyNavigation>,
    #[serde(default)]
    pub timeshifts: Vec<LegacyTimeshift>,
    #[serde(default)]
    pub freezes: Vec<LegacyFreezeRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacySettings {
    pub watch_debounce_ms: i64,
    pub auto_state_prefix: String,
    #[serde(default)]
    pub show_workspace_snapshots_in_git: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyState {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub description: String,
    pub created_at: String,
    pub branch: String,
    pub lane: String,
    #[serde(default)]
    pub continuation_branch: Option<String>,
    pub commit: String,
    #[serde(default)]
    pub parent_commit: Option<String>,
    #[serde(default)]
    pub parent_state_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub stats: LegacyStats,
    #[serde(default)]
    pub metadata: Option<LegacyStateMetadata>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyStats {
    pub changed_files: i64,
    #[serde(default)]
    pub inserted_lines: Option<i64>,
    #[serde(default)]
    pub deleted_lines: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyStateMetadata {
    #[serde(default)]
    pub git_commit: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub cherry: Option<String>,
    #[serde(default)]
    pub stash_from_branch: Option<String>,
    #[serde(default)]
    pub stash_from_state_id: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub deleted_branch: Option<String>,
    #[serde(default)]
    pub deleted_location: Option<LegacyDeletedLocation>,
    #[serde(default)]
    pub prior_contexts: Vec<LegacyPriorContext>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyDeletedLocation {
    pub branch: String,
    pub lane: String,
    #[serde(default)]
    pub continuation_branch: Option<String>,
    #[serde(default)]
    pub parent_state_id: Option<String>,
    pub was_lane_current: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyPriorContext {
    pub branch: String,
    pub lane: String,
    #[serde(default)]
    pub continuation_branch: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyLane {
    pub name: String,
    pub branch: String,
    pub base_ref: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub current_state_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyReturnContext {
    pub state_id: String,
    pub source_branch: String,
    pub source_lane: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LegacyNavigation {
    pub entries: Vec<String>,
    pub index: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyTimeshift {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub branch: String,
    pub lane: String,
    #[serde(default)]
    pub state_id: Option<String>,
    pub relative_cwd: String,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyFreezeRecord {
    pub id: String,
    pub state_id: String,
    pub created_at: String,
    pub bundle_path: String,
    pub manifest_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyWorkspaceSnapshot {
    pub id: String,
    pub created_at: String,
    pub reason: String,
    pub repo: LegacyRepoV1,
    pub git: LegacyGitSnapshot,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyGitSnapshot {
    #[serde(default)]
    pub current_branch: Option<String>,
    #[serde(default)]
    pub head_commit: Option<String>,
    #[serde(default)]
    pub branches: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LegacySnapshotHistory {
    pub version: u64,
    pub index: i64,
    pub entries: Vec<LegacyWorkspaceSnapshot>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct SourceFile {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub mode: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyIdMapEntry {
    pub source_id: String,
    pub entity_kind: String,
    pub legacy_key: String,
    pub target_id: String,
    pub source_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct QuarantinedRecord {
    pub source_path: String,
    pub entity_kind: String,
    pub legacy_key: String,
    pub reason: String,
    pub raw_sha256: String,
    pub raw: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ImportedEntity {
    pub entity_kind: String,
    pub legacy_key: String,
    pub target_id: String,
    pub legacy_ordinal: u64,
    pub payload: Value,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct LegacyImportPlan {
    pub migration_id: String,
    pub source_id: String,
    pub source_schema: String,
    pub target_schema: String,
    pub input_sha256: String,
    pub files: Vec<SourceFile>,
    pub id_map: Vec<LegacyIdMapEntry>,
    pub entities: Vec<ImportedEntity>,
    pub quarantined: Vec<QuarantinedRecord>,
    pub warnings: Vec<String>,
    pub required_oids: Vec<String>,
    #[serde(skip)]
    source_root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyMigrationReceipt {
    pub migration_id: String,
    pub input_sha256: String,
    pub row_counts: BTreeMap<String, u64>,
    pub verification_sha256: String,
    pub already_imported: bool,
}

/// Complete deterministic projection payload produced before the mutation coordinator borrows
/// the store. Persisting this value is the only database effect of a legacy import.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct PreparedLegacyImport {
    pub receipt: LegacyMigrationReceipt,
    pub plan_input_sha256: String,
    pub id_map: Vec<LegacyIdMapEntry>,
    pub entities: Vec<ImportedEntity>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyRollbackManifest {
    pub format_version: u64,
    pub migration_id: String,
    pub source_id: String,
    pub input_sha256: String,
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyRecoveryOutcome {
    pub destination: PathBuf,
    pub files_recovered: usize,
    pub bytes_recovered: u64,
    pub already_recovered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LegacyBundleValidation {
    Verified { advertised_oids: Vec<String> },
    Unavailable { reason: String },
}

pub(crate) fn legacy_source_identity_receipt_key(source_id: &str) -> String {
    format!("mig_{}", &sha256_hex(source_id.as_bytes())[..26])
}

fn legacy_migration_id(source_id: &str) -> String {
    legacy_source_identity_receipt_key(source_id)
}

pub(crate) trait GitObjectLookup {
    fn object_exists(&self, oid: &str) -> Result<bool, String>;

    fn validate_bundle(&self, _bundle: &Path) -> Result<LegacyBundleValidation, String> {
        Ok(LegacyBundleValidation::Unavailable {
            reason: "Git bundle verification is not implemented by this lookup".into(),
        })
    }
}

pub(crate) trait LegacyImportSink {
    fn existing_receipt(
        &mut self,
        migration_id: &str,
    ) -> Result<Option<LegacyMigrationReceipt>, String>;
    fn begin_import(&mut self, plan: &LegacyImportPlan) -> Result<(), String>;
    fn put_id_mapping(&mut self, mapping: &LegacyIdMapEntry) -> Result<(), String>;
    fn put_entity(&mut self, entity: &ImportedEntity) -> Result<(), String>;
    fn commit_import(&mut self, receipt: &LegacyMigrationReceipt) -> Result<(), String>;
    fn abort_import(&mut self);
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyFreezeManifest {
    id: String,
    state: LegacyState,
    created_at: String,
    generated_at: String,
}

impl LegacyImportPlan {
    pub(crate) fn discover(
        repository_root: &Path,
        git: &dyn GitObjectLookup,
    ) -> Result<Self, LegacyImportError> {
        let jjk_root = repository_root.join(".jjk");
        let repo_path = jjk_root.join("repo.json");
        if !repo_path.is_file() {
            return Err(LegacyImportError::MissingRepo(repo_path));
        }

        let files = inventory(&jjk_root)?;
        let repo_bytes = read_bytes(&repo_path)?;
        let raw: Value =
            serde_json::from_slice(&repo_bytes).map_err(|source| LegacyImportError::Json {
                path: repo_path.clone(),
                source,
            })?;
        let version = raw.get("version").and_then(Value::as_u64).ok_or_else(|| {
            LegacyImportError::Invalid {
                path: "version".into(),
                message: "must be integer 1".into(),
            }
        })?;
        if version != 1 {
            return Err(LegacyImportError::UnsupportedVersion(version));
        }
        let mut normalized = raw.clone();
        let timeshift_values = take_optional_array(&mut normalized, "timeshifts")?;
        let freeze_values = take_optional_array(&mut normalized, "freezes")?;
        let mut repo: LegacyRepoV1 =
            serde_json::from_value(normalized).map_err(|source| LegacyImportError::Json {
                path: repo_path.clone(),
                source,
            })?;
        let mut quarantined = Vec::new();
        repo.timeshifts = parse_optional_timeshifts(timeshift_values, &repo, &mut quarantined)?;
        repo.freezes = parse_optional_freezes(freeze_values, &repo, &mut quarantined)?;
        validate_repo(&repo)?;

        let input_sha256 = inventory_digest(&files);
        let source_id = format!("repo-v1:{}:{}", repo.safe_space_id, repo.created_at);
        let source_sha = sha256_hex(&repo_bytes);
        let migration_id = legacy_migration_id(&source_id);
        let mut id_map = Vec::new();
        let mut entities = Vec::new();
        let mut warnings = Vec::new();
        let mut required_oids = BTreeSet::new();

        push_entity(
            &source_id,
            &source_sha,
            "repository",
            &repo.safe_space_id,
            0,
            &repo,
            &mut id_map,
            &mut entities,
        )?;

        let mut missing_by_state = BTreeMap::<String, BTreeSet<String>>::new();
        for (ordinal, state) in repo.states.iter().enumerate() {
            for oid in std::iter::once(&state.commit).chain(state.parent_commit.iter()) {
                required_oids.insert(oid.clone());
                if !git.object_exists(oid).map_err(LegacyImportError::Sink)? {
                    missing_by_state
                        .entry(state.id.clone())
                        .or_default()
                        .insert(oid.clone());
                }
            }
            push_entity(
                &source_id,
                &source_sha,
                "state",
                &state.id,
                ordinal as u64,
                state,
                &mut id_map,
                &mut entities,
            )?;
        }
        if !missing_by_state.is_empty() {
            let state_id = missing_by_state
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            let oids = missing_by_state
                .into_values()
                .flatten()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            return Err(LegacyImportError::MissingGitObjects { state_id, oids });
        }

        for (ordinal, (key, lane)) in repo.lanes.iter().enumerate() {
            if lane.name != *key {
                warnings.push(format!(
                    "lane key `{key}` differs from stored name `{}`",
                    lane.name
                ));
            }
            push_entity(
                &source_id,
                &source_sha,
                "attempt",
                key,
                ordinal as u64,
                lane,
                &mut id_map,
                &mut entities,
            )?;
        }

        let mut branches = BTreeSet::new();
        branches.extend(repo.branch_lane_map.keys().cloned());
        branches.extend(repo.states.iter().map(|state| state.branch.clone()));
        branches.extend(repo.lanes.values().map(|lane| lane.branch.clone()));
        for (ordinal, branch) in branches.into_iter().enumerate() {
            push_entity(
                &source_id,
                &source_sha,
                "branch",
                &branch,
                ordinal as u64,
                &branch,
                &mut id_map,
                &mut entities,
            )?;
        }

        for (ordinal, capture) in repo.timeshifts.iter().enumerate() {
            if !safe_relative_path(Path::new(&capture.relative_cwd)) {
                quarantine_value(
                    "repo.json",
                    "timeshift",
                    &capture.id,
                    "relativeCwd escapes repository root",
                    capture,
                    &mut quarantined,
                )?;
                continue;
            }
            push_entity(
                &source_id,
                &source_sha,
                "timeshift",
                &capture.id,
                ordinal as u64,
                capture,
                &mut id_map,
                &mut entities,
            )?;
        }

        for (ordinal, freeze) in repo.freezes.iter().enumerate() {
            import_freeze(
                &jjk_root,
                &source_id,
                &source_sha,
                freeze,
                ordinal as u64,
                git,
                &mut id_map,
                &mut entities,
                &mut quarantined,
                &mut required_oids,
            )?;
        }

        import_optional_history(
            &jjk_root,
            &source_id,
            &source_sha,
            &mut id_map,
            &mut entities,
            &mut quarantined,
        )?;
        import_optional_backups(
            &jjk_root,
            &source_id,
            &source_sha,
            git,
            &mut id_map,
            &mut entities,
            &mut quarantined,
            &mut required_oids,
        )?;

        Ok(Self {
            migration_id,
            source_id,
            source_schema: LEGACY_SCHEMA.into(),
            target_schema: TARGET_SCHEMA.into(),
            input_sha256,
            files,
            id_map,
            entities,
            quarantined,
            warnings,
            required_oids: required_oids.into_iter().collect(),
            source_root: jjk_root,
        })
    }

    pub(crate) fn preserve_sources(
        &self,
        capsule: &Path,
    ) -> Result<LegacyRollbackManifest, LegacyImportError> {
        let manifest = self.rollback_manifest();
        if capsule.exists() {
            verify_capsule(capsule, &manifest)?;
            return Ok(manifest);
        }
        let staging = staging_path(capsule);
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|source| LegacyImportError::Preserve {
                path: staging.clone(),
                source,
            })?;
        }
        fs::create_dir_all(&staging).map_err(|source| LegacyImportError::Preserve {
            path: staging.clone(),
            source,
        })?;
        let result = (|| {
            for source in &self.files {
                copy_verified_file(
                    &self.source_root.join(&source.relative_path),
                    &staging.join(&source.relative_path),
                    source,
                )?;
            }
            write_rollback_manifest(&staging, &manifest)?;
            sync_tree(&staging, &manifest.files)?;
            verify_capsule(&staging, &manifest)?;
            if let Some(parent) = capsule.parent() {
                fs::create_dir_all(parent).map_err(|source| LegacyImportError::Preserve {
                    path: parent.to_path_buf(),
                    source,
                })?;
                sync_directory(parent)?;
            }
            fs::rename(&staging, capsule).map_err(|source| LegacyImportError::Preserve {
                path: capsule.to_path_buf(),
                source,
            })?;
            if let Some(parent) = capsule.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result.map(|()| manifest)
    }

    fn rollback_manifest(&self) -> LegacyRollbackManifest {
        LegacyRollbackManifest {
            format_version: 1,
            migration_id: self.migration_id.clone(),
            source_id: self.source_id.clone(),
            input_sha256: self.input_sha256.clone(),
            files: self.files.clone(),
        }
    }

    /// Builds the exact import projection without touching a store. The caller may safely do this
    /// before entering the sole mutation coordinator, then publish the returned bytes atomically
    /// with the verified import fact.
    pub(crate) fn prepare_import(&self) -> Result<PreparedLegacyImport, LegacyImportError> {
        let mut row_counts = BTreeMap::new();
        for entity in &self.entities {
            *row_counts.entry(entity.entity_kind.clone()).or_insert(0) += 1;
        }
        let receipt = LegacyMigrationReceipt {
            migration_id: self.migration_id.clone(),
            input_sha256: self.input_sha256.clone(),
            verification_sha256: plan_verification_digest(self, &row_counts)?,
            row_counts,
            already_imported: false,
        };
        Ok(PreparedLegacyImport {
            receipt,
            plan_input_sha256: self.input_sha256.clone(),
            id_map: self.id_map.clone(),
            entities: self.entities.clone(),
        })
    }

    pub(crate) fn apply(
        &self,
        sink: &mut dyn LegacyImportSink,
    ) -> Result<LegacyMigrationReceipt, LegacyImportError> {
        if let Some(mut receipt) = sink
            .existing_receipt(&self.migration_id)
            .map_err(LegacyImportError::Sink)?
        {
            if receipt.input_sha256 != self.input_sha256 {
                return Err(LegacyImportError::Invalid {
                    path: "migration receipt".into(),
                    message: "legacy source identity was already imported from different bytes"
                        .into(),
                });
            }
            receipt.already_imported = true;
            return Ok(receipt);
        }
        sink.begin_import(self).map_err(LegacyImportError::Sink)?;
        let result = (|| {
            let prepared = self.prepare_import()?;
            for mapping in &prepared.id_map {
                sink.put_id_mapping(mapping)
                    .map_err(LegacyImportError::Sink)?;
            }
            for entity in &prepared.entities {
                sink.put_entity(entity).map_err(LegacyImportError::Sink)?;
            }
            sink.commit_import(&prepared.receipt)
                .map_err(LegacyImportError::Sink)?;
            Ok(prepared.receipt)
        })();
        if result.is_err() {
            sink.abort_import();
        }
        result
    }
}

fn take_optional_array(root: &mut Value, field: &str) -> Result<Vec<Value>, LegacyImportError> {
    let object = root
        .as_object_mut()
        .ok_or_else(|| LegacyImportError::Invalid {
            path: "repo.json".into(),
            message: "top level must be an object".into(),
        })?;
    match object.remove(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => Ok(values),
        Some(_) => invalid(field, "must be an array when present"),
    }
}

fn parse_optional_timeshifts(
    values: Vec<Value>,
    repo: &LegacyRepoV1,
    quarantined: &mut Vec<QuarantinedRecord>,
) -> Result<Vec<LegacyTimeshift>, LegacyImportError> {
    let state_ids = repo
        .states
        .iter()
        .map(|state| state.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut result = Vec::new();
    for (index, raw) in values.into_iter().enumerate() {
        let key = raw
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("ordinal-{index}"));
        let capture = match serde_json::from_value::<LegacyTimeshift>(raw.clone()) {
            Ok(capture) => capture,
            Err(error) => {
                quarantine_raw(
                    "repo.json",
                    "timeshift",
                    &key,
                    &format!("invalid optional record: {error}"),
                    raw,
                    quarantined,
                )?;
                continue;
            }
        };
        let reason = if capture.id.is_empty() {
            Some("id must not be empty")
        } else if timestamp("timeshift.createdAt", &capture.created_at).is_err() {
            Some("createdAt is not RFC3339")
        } else if !repo.lanes.contains_key(&capture.lane) {
            Some("lane references unknown lane")
        } else if capture
            .state_id
            .as_ref()
            .is_some_and(|id| !state_ids.contains(id.as_str()))
        {
            Some("stateId references unknown state")
        } else {
            None
        };
        if let Some(reason) = reason {
            quarantine_raw("repo.json", "timeshift", &key, reason, raw, quarantined)?;
        } else {
            result.push(capture);
        }
    }
    Ok(result)
}

fn parse_optional_freezes(
    values: Vec<Value>,
    repo: &LegacyRepoV1,
    quarantined: &mut Vec<QuarantinedRecord>,
) -> Result<Vec<LegacyFreezeRecord>, LegacyImportError> {
    let state_ids = repo
        .states
        .iter()
        .map(|state| state.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut result = Vec::new();
    for (index, raw) in values.into_iter().enumerate() {
        let key = raw
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("ordinal-{index}"));
        let freeze = match serde_json::from_value::<LegacyFreezeRecord>(raw.clone()) {
            Ok(freeze) => freeze,
            Err(error) => {
                quarantine_raw(
                    "repo.json",
                    "legacy_freeze",
                    &key,
                    &format!("invalid optional record: {error}"),
                    raw,
                    quarantined,
                )?;
                continue;
            }
        };
        let reason = if freeze.id.is_empty() {
            Some("id must not be empty")
        } else if timestamp("freeze.createdAt", &freeze.created_at).is_err() {
            Some("createdAt is not RFC3339")
        } else if !state_ids.contains(freeze.state_id.as_str()) {
            Some("stateId references unknown state")
        } else {
            None
        };
        if let Some(reason) = reason {
            quarantine_raw("repo.json", "legacy_freeze", &key, reason, raw, quarantined)?;
        } else {
            result.push(freeze);
        }
    }
    Ok(result)
}

fn import_optional_history(
    jjk_root: &Path,
    source_id: &str,
    source_sha: &str,
    mappings: &mut Vec<LegacyIdMapEntry>,
    entities: &mut Vec<ImportedEntity>,
    quarantined: &mut Vec<QuarantinedRecord>,
) -> Result<(), LegacyImportError> {
    let path = jjk_root.join("history.json");
    if !path.is_file() {
        return Ok(());
    }
    let raw = read_optional_value(&path)?;
    let history = match serde_json::from_value::<LegacySnapshotHistory>(raw.clone()) {
        Ok(history) => history,
        Err(error) => {
            return quarantine_raw(
                "history.json",
                "control_history",
                "history",
                &error.to_string(),
                raw,
                quarantined,
            );
        }
    };
    if let Err(error) = validate_history(&history) {
        return quarantine_raw(
            "history.json",
            "control_history",
            "history",
            &error.to_string(),
            raw,
            quarantined,
        );
    }
    for (ordinal, snapshot) in history.entries.iter().enumerate() {
        if let Err(error) = validate_snapshot(snapshot, &format!("history.entries[{ordinal}]")) {
            return quarantine_raw(
                "history.json",
                "control_history",
                "history",
                &error.to_string(),
                raw,
                quarantined,
            );
        }
    }
    for (ordinal, snapshot) in history.entries.iter().enumerate() {
        push_entity(
            source_id,
            source_sha,
            "control_snapshot",
            &snapshot.id,
            ordinal as u64,
            snapshot,
            mappings,
            entities,
        )?;
    }
    Ok(())
}

fn import_optional_backups(
    jjk_root: &Path,
    source_id: &str,
    source_sha: &str,
    git: &dyn GitObjectLookup,
    mappings: &mut Vec<LegacyIdMapEntry>,
    entities: &mut Vec<ImportedEntity>,
    quarantined: &mut Vec<QuarantinedRecord>,
    required_oids: &mut BTreeSet<String>,
) -> Result<(), LegacyImportError> {
    let backups = jjk_root.join("backups");
    if !backups.is_dir() {
        return Ok(());
    }
    let mut paths = regular_children(&backups)?;
    paths.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("json"));
    for (ordinal, path) in paths.iter().enumerate() {
        let source_path = format!("backups/{}", file_name(path));
        let raw = read_optional_value(path)?;
        let snapshot = match serde_json::from_value::<LegacyWorkspaceSnapshot>(raw.clone()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                quarantine_raw(
                    &source_path,
                    "legacy_backup",
                    &file_name(path),
                    &error.to_string(),
                    raw,
                    quarantined,
                )?;
                continue;
            }
        };
        if let Err(error) = validate_snapshot(&snapshot, &source_path) {
            quarantine_raw(
                &source_path,
                "legacy_backup",
                &snapshot.id,
                &error.to_string(),
                raw,
                quarantined,
            )?;
            continue;
        }
        let oids = snapshot_oids(&snapshot);
        required_oids.extend(oids.iter().cloned());
        let mut missing = Vec::new();
        for oid in oids {
            if !git.object_exists(&oid).map_err(LegacyImportError::Sink)? {
                missing.push(oid);
            }
        }
        if !missing.is_empty() {
            quarantine_raw(
                &source_path,
                "legacy_backup",
                &snapshot.id,
                &format!("missing Git objects: {}", missing.join(",")),
                raw,
                quarantined,
            )?;
            continue;
        }
        push_entity(
            source_id,
            source_sha,
            "legacy_backup",
            &snapshot.id,
            ordinal as u64,
            &snapshot,
            mappings,
            entities,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn import_freeze(
    jjk_root: &Path,
    source_id: &str,
    source_sha: &str,
    freeze: &LegacyFreezeRecord,
    ordinal: u64,
    git: &dyn GitObjectLookup,
    mappings: &mut Vec<LegacyIdMapEntry>,
    entities: &mut Vec<ImportedEntity>,
    quarantined: &mut Vec<QuarantinedRecord>,
    required_oids: &mut BTreeSet<String>,
) -> Result<(), LegacyImportError> {
    let raw = serde_json::to_value(freeze).map_err(|error| LegacyImportError::Invalid {
        path: format!("freeze.{}", freeze.id),
        message: error.to_string(),
    })?;
    let (bundle, manifest_path) = match (
        safe_source_path(jjk_root, &freeze.bundle_path),
        safe_source_path(jjk_root, &freeze.manifest_path),
    ) {
        (Ok(bundle), Ok(manifest)) if bundle.is_file() && manifest.is_file() => (bundle, manifest),
        _ => {
            return quarantine_raw(
                "repo.json",
                "legacy_freeze",
                &freeze.id,
                "freeze pair missing or path escapes .jjk",
                raw,
                quarantined,
            );
        }
    };
    let manifest_raw = read_optional_value(&manifest_path)?;
    let manifest = match serde_json::from_value::<LegacyFreezeManifest>(manifest_raw) {
        Ok(manifest) => manifest,
        Err(error) => {
            return quarantine_raw(
                "repo.json",
                "legacy_freeze",
                &freeze.id,
                &format!("invalid freeze manifest: {error}"),
                raw,
                quarantined,
            );
        }
    };
    if manifest.id != freeze.id
        || manifest.state.id != freeze.state_id
        || manifest.created_at != freeze.created_at
    {
        return quarantine_raw(
            "repo.json",
            "legacy_freeze",
            &freeze.id,
            "freeze manifest identity does not match repo record",
            raw,
            quarantined,
        );
    }
    if let Err(error) = timestamp("freeze.manifest.generatedAt", &manifest.generated_at) {
        return quarantine_raw(
            "repo.json",
            "legacy_freeze",
            &freeze.id,
            &error.to_string(),
            raw,
            quarantined,
        );
    }
    if let Err(error) = validate_state(
        &manifest.state,
        0,
        &BTreeSet::from([manifest.state.id.as_str()]),
    ) {
        return quarantine_raw(
            "repo.json",
            "legacy_freeze",
            &freeze.id,
            &error.to_string(),
            raw,
            quarantined,
        );
    }
    required_oids.insert(manifest.state.commit.clone());
    match git
        .validate_bundle(&bundle)
        .map_err(LegacyImportError::Sink)?
    {
        LegacyBundleValidation::Verified { advertised_oids }
            if advertised_oids
                .iter()
                .any(|oid| oid == &manifest.state.commit) =>
        {
            push_entity(
                source_id,
                source_sha,
                "legacy_freeze",
                &freeze.id,
                ordinal,
                freeze,
                mappings,
                entities,
            )
        }
        LegacyBundleValidation::Verified { .. } => quarantine_raw(
            "repo.json",
            "legacy_freeze",
            &freeze.id,
            "bundle does not advertise the frozen state commit",
            raw,
            quarantined,
        ),
        LegacyBundleValidation::Unavailable { reason } => quarantine_raw(
            "repo.json",
            "legacy_freeze",
            &freeze.id,
            &format!("bundle verification unavailable: {reason}"),
            raw,
            quarantined,
        ),
    }
}

fn snapshot_oids(snapshot: &LegacyWorkspaceSnapshot) -> BTreeSet<String> {
    let mut oids = BTreeSet::new();
    if let Some(head) = &snapshot.git.head_commit {
        oids.insert(head.clone());
    }
    oids.extend(snapshot.git.branches.values().cloned());
    for state in &snapshot.repo.states {
        oids.insert(state.commit.clone());
        oids.extend(state.parent_commit.iter().cloned());
    }
    oids
}

fn validate_repo(repo: &LegacyRepoV1) -> Result<(), LegacyImportError> {
    if repo.version != 1 {
        return Err(LegacyImportError::UnsupportedVersion(repo.version));
    }
    nonempty("safeSpaceId", &repo.safe_space_id)?;
    timestamp("createdAt", &repo.created_at)?;
    timestamp("updatedAt", &repo.updated_at)?;
    if !(10..=600_000).contains(&repo.settings.watch_debounce_ms) {
        return invalid("settings.watchDebounceMs", "must be within 10..=600000");
    }
    let state_ids = repo
        .states
        .iter()
        .map(|state| state.id.as_str())
        .collect::<BTreeSet<_>>();
    if state_ids.len() != repo.states.len() {
        return invalid("states", "contains duplicate state IDs");
    }
    for (index, state) in repo.states.iter().enumerate() {
        validate_state(state, index, &state_ids)?;
    }
    for (key, lane) in &repo.lanes {
        nonempty(&format!("lanes.{key}.name"), &lane.name)?;
        nonempty(&format!("lanes.{key}.branch"), &lane.branch)?;
        nonempty(&format!("lanes.{key}.baseRef"), &lane.base_ref)?;
        timestamp(&format!("lanes.{key}.createdAt"), &lane.created_at)?;
        timestamp(&format!("lanes.{key}.updatedAt"), &lane.updated_at)?;
        if lane
            .current_state_id
            .as_ref()
            .is_some_and(|tip| !state_ids.contains(tip.as_str()))
        {
            return invalid(
                &format!("lanes.{key}.currentStateId"),
                "references unknown state",
            );
        }
    }
    for (branch, lane) in &repo.branch_lane_map {
        nonempty("branchLaneMap.branch", branch)?;
        if !repo.lanes.contains_key(lane) {
            return invalid(
                &format!("branchLaneMap.{branch}"),
                "references unknown lane",
            );
        }
    }
    if let Some(context) = &repo.return_context {
        if !state_ids.contains(context.state_id.as_str()) {
            return invalid("returnContext.stateId", "references unknown state");
        }
        if !repo.lanes.contains_key(&context.source_lane) {
            return invalid("returnContext.sourceLane", "references unknown lane");
        }
    }
    if let Some(history) = &repo.current_state_history {
        validate_navigation(history, &state_ids)?;
    }
    for (index, capture) in repo.timeshifts.iter().enumerate() {
        nonempty(&format!("timeshifts[{index}].id"), &capture.id)?;
        timestamp(
            &format!("timeshifts[{index}].createdAt"),
            &capture.created_at,
        )?;
        if !repo.lanes.contains_key(&capture.lane) {
            return invalid(
                &format!("timeshifts[{index}].lane"),
                "references unknown lane",
            );
        }
        if capture
            .state_id
            .as_ref()
            .is_some_and(|id| !state_ids.contains(id.as_str()))
        {
            return invalid(
                &format!("timeshifts[{index}].stateId"),
                "references unknown state",
            );
        }
    }
    let freeze_ids = repo
        .freezes
        .iter()
        .map(|freeze| freeze.id.as_str())
        .collect::<BTreeSet<_>>();
    if freeze_ids.len() != repo.freezes.len() {
        return invalid("freezes", "contains duplicate freeze IDs");
    }
    for (index, freeze) in repo.freezes.iter().enumerate() {
        nonempty(&format!("freezes[{index}].id"), &freeze.id)?;
        timestamp(&format!("freezes[{index}].createdAt"), &freeze.created_at)?;
        if !state_ids.contains(freeze.state_id.as_str()) {
            return invalid(
                &format!("freezes[{index}].stateId"),
                "references unknown state",
            );
        }
    }
    Ok(())
}

fn validate_state(
    state: &LegacyState,
    index: usize,
    state_ids: &BTreeSet<&str>,
) -> Result<(), LegacyImportError> {
    let root = format!("states[{index}]");
    nonempty(&format!("{root}.id"), &state.id)?;
    if !matches!(
        state.kind.as_str(),
        "new" | "git" | "save" | "stash" | "cherry" | "step" | "nice" | "star" | "auto"
    ) {
        return invalid(&format!("{root}.kind"), "unknown legacy state kind");
    }
    nonempty(&format!("{root}.branch"), &state.branch)?;
    nonempty(&format!("{root}.lane"), &state.lane)?;
    timestamp(&format!("{root}.createdAt"), &state.created_at)?;
    git_oid(&format!("{root}.commit"), &state.commit)?;
    if let Some(parent) = &state.parent_commit {
        git_oid(&format!("{root}.parentCommit"), parent)?;
    }
    if state
        .parent_state_id
        .as_ref()
        .is_some_and(|parent| !state_ids.contains(parent.as_str()))
    {
        return invalid(&format!("{root}.parentStateId"), "references unknown state");
    }
    if state.stats.changed_files < 0
        || state.stats.inserted_lines.is_some_and(|value| value < 0)
        || state.stats.deleted_lines.is_some_and(|value| value < 0)
    {
        return invalid(&format!("{root}.stats"), "counts cannot be negative");
    }
    if let Some(metadata) = &state.metadata {
        if let Some(oid) = &metadata.git_commit {
            git_oid(&format!("{root}.metadata.gitCommit"), oid)?;
            if oid != &state.commit {
                return invalid(
                    &format!("{root}.metadata.gitCommit"),
                    "does not equal state commit",
                );
            }
        }
        for (path, referenced) in [
            ("base", metadata.base.as_ref()),
            ("cherry", metadata.cherry.as_ref()),
            ("stashFromStateId", metadata.stash_from_state_id.as_ref()),
        ] {
            if referenced.is_some_and(|id| !state_ids.contains(id.as_str())) {
                return invalid(
                    &format!("{root}.metadata.{path}"),
                    "references unknown state",
                );
            }
        }
        if let Some(deleted_at) = &metadata.deleted_at {
            timestamp(&format!("{root}.metadata.deletedAt"), deleted_at)?;
        }
        if metadata
            .deleted_location
            .as_ref()
            .and_then(|location| location.parent_state_id.as_ref())
            .is_some_and(|id| !state_ids.contains(id.as_str()))
        {
            return invalid(
                &format!("{root}.metadata.deletedLocation.parentStateId"),
                "references unknown state",
            );
        }
        for (ordinal, context) in metadata.prior_contexts.iter().enumerate() {
            timestamp(
                &format!("{root}.metadata.priorContexts[{ordinal}].updatedAt"),
                &context.updated_at,
            )?;
        }
    }
    Ok(())
}

fn validate_navigation(
    history: &LegacyNavigation,
    states: &BTreeSet<&str>,
) -> Result<(), LegacyImportError> {
    if history.entries.is_empty() {
        if history.index != -1 {
            return invalid("currentStateHistory.index", "must be -1 for empty history");
        }
    } else if history.index < 0 || history.index as usize >= history.entries.len() {
        return invalid("currentStateHistory.index", "outside entries bounds");
    }
    if history
        .entries
        .iter()
        .any(|id| !states.contains(id.as_str()))
    {
        return invalid("currentStateHistory.entries", "contains unknown state");
    }
    Ok(())
}

fn validate_history(history: &LegacySnapshotHistory) -> Result<(), LegacyImportError> {
    if history.version != 1 {
        return invalid("history.version", "must equal 1");
    }
    if history.entries.is_empty() && history.index != -1 {
        return invalid("history.index", "must be -1 for empty history");
    }
    if !history.entries.is_empty()
        && (history.index < 0 || history.index as usize >= history.entries.len())
    {
        return invalid("history.index", "outside entries bounds");
    }
    Ok(())
}

fn validate_snapshot(
    snapshot: &LegacyWorkspaceSnapshot,
    root: &str,
) -> Result<(), LegacyImportError> {
    timestamp(&format!("{root}.createdAt"), &snapshot.created_at)?;
    validate_repo(&snapshot.repo)?;
    if let Some(head) = &snapshot.git.head_commit {
        git_oid(&format!("{root}.git.headCommit"), head)?;
    }
    for (branch, oid) in &snapshot.git.branches {
        git_oid(&format!("{root}.git.branches.{branch}"), oid)?;
    }
    Ok(())
}

fn push_entity<T: Serialize>(
    source_id: &str,
    source_sha: &str,
    kind: &str,
    key: &str,
    ordinal: u64,
    value: &T,
    mappings: &mut Vec<LegacyIdMapEntry>,
    entities: &mut Vec<ImportedEntity>,
) -> Result<(), LegacyImportError> {
    let target_id = deterministic_id(kind, source_id, key);
    mappings.push(LegacyIdMapEntry {
        source_id: source_id.into(),
        entity_kind: kind.into(),
        legacy_key: key.into(),
        target_id: target_id.clone(),
        source_sha256: source_sha.into(),
    });
    let payload = serde_json::to_value(value).map_err(|error| LegacyImportError::Invalid {
        path: format!("{kind}.{key}"),
        message: error.to_string(),
    })?;
    entities.push(ImportedEntity {
        entity_kind: kind.into(),
        legacy_key: key.into(),
        target_id,
        legacy_ordinal: ordinal,
        payload,
    });
    Ok(())
}

fn quarantine_value<T: Serialize>(
    source_path: &str,
    kind: &str,
    key: &str,
    reason: &str,
    value: &T,
    quarantined: &mut Vec<QuarantinedRecord>,
) -> Result<(), LegacyImportError> {
    let raw = serde_json::to_value(value).map_err(|error| LegacyImportError::Invalid {
        path: format!("{kind}.{key}"),
        message: error.to_string(),
    })?;
    quarantine_raw(source_path, kind, key, reason, raw, quarantined)
}

fn quarantine_raw(
    source_path: &str,
    kind: &str,
    key: &str,
    reason: &str,
    raw: Value,
    quarantined: &mut Vec<QuarantinedRecord>,
) -> Result<(), LegacyImportError> {
    let bytes = serde_json::to_vec(&raw).map_err(|error| LegacyImportError::Invalid {
        path: format!("{kind}.{key}"),
        message: error.to_string(),
    })?;
    quarantined.push(QuarantinedRecord {
        source_path: source_path.into(),
        entity_kind: kind.into(),
        legacy_key: key.into(),
        reason: reason.into(),
        raw_sha256: sha256_hex(&bytes),
        raw: Some(raw),
    });
    Ok(())
}

fn deterministic_id(kind: &str, source_id: &str, key: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"jjk-legacy-id-v1\0");
    digest.update(kind.as_bytes());
    digest.update([0]);
    digest.update(source_id.as_bytes());
    digest.update([0]);
    digest.update(key.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    // Imported IDs are stable across preflight/retry and retain the canonical
    // UUIDv7 type marker. The legacy source timestamp is carried separately as
    // provenance; it is never guessed from these deterministic bytes.
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let prefix = match kind {
        "repository" => "repo_",
        "state" => "st_",
        "attempt" => "at_",
        "branch" => "br_",
        "timeshift" => "tsh_",
        "legacy_backup" => "bak_",
        _ => "artifact_",
    };
    format!("{prefix}{}", crockford_uuid(Uuid::from_bytes(bytes)))
}

fn crockford_uuid(uuid: Uuid) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut value = uuid.as_u128();
    let mut output = [b'0'; 26];
    for byte in output.iter_mut().rev() {
        *byte = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(output.to_vec()).expect("Crockford alphabet is UTF-8")
}

fn inventory(jjk_root: &Path) -> Result<Vec<SourceFile>, LegacyImportError> {
    let repo = jjk_root.join("repo.json");
    if !repo.is_file() {
        return Err(LegacyImportError::MissingRepo(repo));
    }
    let mut paths = vec![repo];
    let history = jjk_root.join("history.json");
    if history.exists() {
        reject_non_regular_source(&history)?;
        paths.push(history);
    }
    for directory in [jjk_root.join("backups"), jjk_root.join("freezes")] {
        if directory.exists() {
            let metadata =
                fs::symlink_metadata(&directory).map_err(|source| LegacyImportError::Read {
                    path: directory.clone(),
                    source,
                })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return invalid(
                    &directory.display().to_string(),
                    "legacy source directory must be a real directory",
                );
            }
            paths.extend(regular_children(&directory)?);
        }
    }
    paths.sort();
    let mut result = Vec::new();
    for path in paths {
        reject_non_regular_source(&path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|source| LegacyImportError::Read {
            path: path.clone(),
            source,
        })?;
        let bytes = read_bytes(&path)?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        };
        #[cfg(not(unix))]
        let mode = 0;
        result.push(SourceFile {
            relative_path: path
                .strip_prefix(jjk_root)
                .expect("inventory rooted beneath .jjk")
                .to_string_lossy()
                .replace('\\', "/"),
            size_bytes: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
            mode,
        });
    }
    Ok(result)
}

fn inventory_digest(files: &[SourceFile]) -> String {
    let mut digest = Sha256::new();
    for file in files {
        digest.update(file.relative_path.as_bytes());
        digest.update([0]);
        digest.update(file.size_bytes.to_be_bytes());
        digest.update(hex::decode(&file.sha256).expect("internally generated SHA-256"));
    }
    hex::encode(digest.finalize())
}

fn verify_capsule(
    capsule: &Path,
    expected: &LegacyRollbackManifest,
) -> Result<(), LegacyImportError> {
    let manifest = read_rollback_manifest(capsule)?;
    if &manifest != expected || inventory_digest(&manifest.files) != manifest.input_sha256 {
        return Err(LegacyImportError::PreserveDigest(capsule.to_owned()));
    }
    verify_file_set(capsule, &manifest.files, true)
}

pub(crate) fn recover_legacy_capsule(
    capsule: &Path,
    destination: &Path,
) -> Result<LegacyRecoveryOutcome, LegacyImportError> {
    let manifest = read_rollback_manifest(capsule)?;
    if inventory_digest(&manifest.files) != manifest.input_sha256 {
        return Err(LegacyImportError::PreserveDigest(capsule.to_owned()));
    }
    verify_file_set(capsule, &manifest.files, true)?;
    if destination.exists() {
        let metadata =
            fs::symlink_metadata(destination).map_err(|source| LegacyImportError::Recover {
                path: destination.to_owned(),
                source,
            })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(LegacyImportError::RecoveryDestinationExists(
                destination.to_owned(),
            ));
        }
        let mut missing = Vec::new();
        for source in &manifest.files {
            let target = destination.join(&source.relative_path);
            if target.exists() {
                reject_non_regular_source(&target)?;
                let bytes = read_bytes(&target)?;
                if bytes.len() as u64 != source.size_bytes || sha256_hex(&bytes) != source.sha256 {
                    return Err(LegacyImportError::RecoveryDestinationExists(target));
                }
            } else {
                missing.push(source);
            }
        }
        if missing.is_empty() {
            return Ok(recovery_outcome(destination, &manifest, true));
        }
        for source in missing {
            copy_verified_file(
                &capsule.join(&source.relative_path),
                &destination.join(&source.relative_path),
                source,
            )?;
        }
        sync_tree(destination, &manifest.files)?;
        return Ok(recovery_outcome(destination, &manifest, false));
    }
    let staging = staging_path(destination);
    if staging.exists() {
        return Err(LegacyImportError::RecoveryDestinationExists(staging));
    }
    fs::create_dir_all(&staging).map_err(|source| LegacyImportError::Recover {
        path: staging.clone(),
        source,
    })?;
    let result = (|| {
        for source in &manifest.files {
            copy_verified_file(
                &capsule.join(&source.relative_path),
                &staging.join(&source.relative_path),
                source,
            )?;
        }
        verify_file_set(&staging, &manifest.files, false)?;
        sync_tree(&staging, &manifest.files)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| LegacyImportError::Recover {
                path: parent.to_owned(),
                source,
            })?;
            sync_directory(parent)?;
        }
        fs::rename(&staging, destination).map_err(|source| LegacyImportError::Recover {
            path: destination.to_owned(),
            source,
        })?;
        if let Some(parent) = destination.parent() {
            sync_directory(parent)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result.map(|()| recovery_outcome(destination, &manifest, false))
}

fn recovery_outcome(
    destination: &Path,
    manifest: &LegacyRollbackManifest,
    already_recovered: bool,
) -> LegacyRecoveryOutcome {
    LegacyRecoveryOutcome {
        destination: destination.to_owned(),
        files_recovered: manifest.files.len(),
        bytes_recovered: manifest.files.iter().map(|file| file.size_bytes).sum(),
        already_recovered,
    }
}

fn write_rollback_manifest(
    capsule: &Path,
    manifest: &LegacyRollbackManifest,
) -> Result<(), LegacyImportError> {
    let path = capsule.join(ROLLBACK_MANIFEST);
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|error| {
        LegacyImportError::CapsuleManifest {
            path: path.clone(),
            message: error.to_string(),
        }
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|source| LegacyImportError::Preserve {
            path: path.clone(),
            source,
        })?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|source| LegacyImportError::Preserve { path, source })
}

fn read_rollback_manifest(capsule: &Path) -> Result<LegacyRollbackManifest, LegacyImportError> {
    let path = capsule.join(ROLLBACK_MANIFEST);
    let bytes = read_bytes(&path)?;
    let manifest: LegacyRollbackManifest =
        serde_json::from_slice(&bytes).map_err(|error| LegacyImportError::CapsuleManifest {
            path: path.clone(),
            message: error.to_string(),
        })?;
    if manifest.format_version != 1 || !manifest.migration_id.starts_with("mig_") {
        return Err(LegacyImportError::CapsuleManifest {
            path,
            message: "unsupported rollback manifest identity".into(),
        });
    }
    validate_source_manifest(&manifest.files)?;
    Ok(manifest)
}

fn validate_source_manifest(files: &[SourceFile]) -> Result<(), LegacyImportError> {
    if files.is_empty() || files.iter().all(|file| file.relative_path != "repo.json") {
        return invalid("rollback.files", "must contain repo.json");
    }
    let mut seen = BTreeSet::new();
    for file in files {
        if !safe_relative_path(Path::new(&file.relative_path))
            || !seen.insert(file.relative_path.as_str())
        {
            return invalid(
                "rollback.files",
                "contains unsafe or duplicate relative path",
            );
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return invalid("rollback.files.sha256", "must be a SHA-256 digest");
        }
    }
    Ok(())
}

fn verify_file_set(
    root: &Path,
    files: &[SourceFile],
    allow_manifest: bool,
) -> Result<(), LegacyImportError> {
    validate_source_manifest(files)?;
    let expected = files
        .iter()
        .map(|source| source.relative_path.clone())
        .collect::<BTreeSet<_>>();
    let mut actual = recursive_regular_files(root)?;
    if allow_manifest {
        actual.remove(ROLLBACK_MANIFEST);
    }
    if actual != expected {
        return Err(LegacyImportError::PreserveDigest(root.to_owned()));
    }
    for source in files {
        let path = root.join(&source.relative_path);
        reject_non_regular_source(&path)?;
        let bytes = read_bytes(&path)?;
        if bytes.len() as u64 != source.size_bytes || sha256_hex(&bytes) != source.sha256 {
            return Err(LegacyImportError::PreserveDigest(path));
        }
    }
    Ok(())
}

fn recursive_regular_files(root: &Path) -> Result<BTreeSet<String>, LegacyImportError> {
    let mut pending = vec![root.to_owned()];
    let mut result = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        let metadata =
            fs::symlink_metadata(&directory).map_err(|source| LegacyImportError::Read {
                path: directory.clone(),
                source,
            })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return invalid(&directory.display().to_string(), "must be a real directory");
        }
        for path in children(&directory)? {
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| LegacyImportError::Read {
                    path: path.clone(),
                    source,
                })?;
            if metadata.file_type().is_symlink() {
                return invalid(
                    &path.display().to_string(),
                    "legacy source may not be a symlink",
                );
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                return invalid(
                    &path.display().to_string(),
                    "legacy source must be a regular file",
                );
            }
            result.insert(
                path.strip_prefix(root)
                    .expect("walk remains under root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(result)
}

fn copy_verified_file(
    from: &Path,
    to: &Path,
    expected: &SourceFile,
) -> Result<(), LegacyImportError> {
    reject_non_regular_source(from)?;
    let bytes = read_bytes(from)?;
    if bytes.len() as u64 != expected.size_bytes || sha256_hex(&bytes) != expected.sha256 {
        return Err(LegacyImportError::PreserveDigest(from.to_owned()));
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|source| LegacyImportError::Preserve {
            path: parent.to_owned(),
            source,
        })?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|source| LegacyImportError::Preserve {
            path: to.to_owned(),
            source,
        })?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| LegacyImportError::Preserve {
            path: to.to_owned(),
            source,
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(to, fs::Permissions::from_mode(expected.mode)).map_err(|source| {
            LegacyImportError::Preserve {
                path: to.to_owned(),
                source,
            }
        })?;
    }
    Ok(())
}

fn sync_tree(root: &Path, files: &[SourceFile]) -> Result<(), LegacyImportError> {
    let mut directories = BTreeSet::from([root.to_owned()]);
    for source in files {
        if let Some(parent) = root.join(&source.relative_path).parent() {
            directories.insert(parent.to_owned());
        }
    }
    for directory in directories.iter().rev() {
        sync_directory(directory)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), LegacyImportError> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| LegacyImportError::Preserve {
            path: path.to_owned(),
            source,
        })
}

fn staging_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("legacy");
    destination.with_file_name(format!(".{name}.staging"))
}

fn reject_non_regular_source(path: &Path) -> Result<(), LegacyImportError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| LegacyImportError::Read {
        path: path.to_owned(),
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return invalid(
            &path.display().to_string(),
            "legacy source must be a regular non-symlink file",
        );
    }
    Ok(())
}

fn children(directory: &Path) -> Result<Vec<PathBuf>, LegacyImportError> {
    let mut result = fs::read_dir(directory)
        .map_err(|source| LegacyImportError::Read {
            path: directory.to_owned(),
            source,
        })?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| LegacyImportError::Read {
            path: directory.to_owned(),
            source,
        })?;
    result.sort();
    Ok(result)
}

fn regular_children(directory: &Path) -> Result<Vec<PathBuf>, LegacyImportError> {
    let result = children(directory)?;
    for path in &result {
        reject_non_regular_source(path)?;
    }
    Ok(result)
}

fn read_optional_value(path: &Path) -> Result<Value, LegacyImportError> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|source| LegacyImportError::Json {
        path: path.to_owned(),
        source,
    })
}

fn plan_verification_digest(
    plan: &LegacyImportPlan,
    row_counts: &BTreeMap<String, u64>,
) -> Result<String, LegacyImportError> {
    let bytes = serde_json::to_vec(&(
        plan.input_sha256.as_str(),
        &plan.id_map,
        &plan.entities,
        row_counts,
    ))
    .map_err(|error| LegacyImportError::Invalid {
        path: "verification".into(),
        message: error.to_string(),
    })?;
    Ok(sha256_hex(&bytes))
}

fn safe_source_path(root: &Path, raw: &str) -> Result<PathBuf, LegacyImportError> {
    let raw = Path::new(raw);
    let relative = raw.strip_prefix(".jjk").unwrap_or(raw);
    if !safe_relative_path(relative) {
        return invalid(raw.to_string_lossy().as_ref(), "path escapes .jjk");
    }
    let path = root.join(relative);
    if let Ok(canonical) = path.canonicalize() {
        let canonical_root = root
            .canonicalize()
            .map_err(|source| LegacyImportError::Read {
                path: root.to_path_buf(),
                source,
            })?;
        if !canonical.starts_with(canonical_root) {
            return invalid(raw.to_string_lossy().as_ref(), "symlink escapes .jjk");
        }
    }
    Ok(path)
}

fn safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn read_optional_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, LegacyImportError> {
    if !path.is_file() {
        return Ok(None);
    }
    read_json(path).map(Some)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, LegacyImportError> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|source| LegacyImportError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, LegacyImportError> {
    fs::read(path).map_err(|source| LegacyImportError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn timestamp(path: &str, value: &str) -> Result<(), LegacyImportError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|error| LegacyImportError::Invalid {
            path: path.into(),
            message: format!("invalid RFC3339 timestamp: {error}"),
        })
}

fn git_oid(path: &str, oid: &str) -> Result<(), LegacyImportError> {
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        invalid(path, "must be a 40- or 64-hex Git object ID")
    }
}

fn nonempty(path: &str, value: &str) -> Result<(), LegacyImportError> {
    if value.is_empty() {
        invalid(path, "must not be empty")
    } else {
        Ok(())
    }
}

fn invalid<T>(path: &str, message: &str) -> Result<T, LegacyImportError> {
    Err(LegacyImportError::Invalid {
        path: path.into(),
        message: message.into(),
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("<non-utf8>")
        .into()
}
