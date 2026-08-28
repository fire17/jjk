//! Read-only importer for the legacy `.jjk/repo.json` version 1 store.
//!
//! The adapter deliberately owns no SQLite or Git implementation. It parses and
//! verifies legacy bytes, builds a deterministic import plan, and applies that
//! plan through [`LegacyImportSink`] in one caller-owned transaction.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const LEGACY_SCHEMA: &str = "jjk-repo-json-v1";
const TARGET_SCHEMA: &str = "jjk-store/1.0";

#[derive(Debug, thiserror::Error)]
pub(crate) enum LegacyImportError {
    #[error("legacy repo not found at {0}")]
    MissingRepo(PathBuf),
    #[error("cannot read legacy source {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("invalid legacy JSON in {path}: {source}")]
    Json { path: PathBuf, source: serde_json::Error },
    #[error("unsupported legacy schema version {0}; expected 1")]
    UnsupportedVersion(u64),
    #[error("invalid legacy data at {path}: {message}")]
    Invalid { path: String, message: String },
    #[error("legacy state {state_id} references missing Git objects: {oids:?}")]
    MissingGitObjects { state_id: String, oids: Vec<String> },
    #[error("legacy import sink rejected the plan: {0}")]
    Sink(String),
    #[error("cannot preserve legacy source {path}: {source}")]
    Preserve { path: PathBuf, source: std::io::Error },
    #[error("preserved source digest mismatch for {0}")]
    PreserveDigest(PathBuf),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SourceFile {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub mode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyIdMapEntry {
    pub source_id: String,
    pub entity_kind: String,
    pub legacy_key: String,
    pub target_id: String,
    pub source_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct QuarantinedRecord {
    pub source_path: String,
    pub entity_kind: String,
    pub legacy_key: String,
    pub reason: String,
    pub raw_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyMigrationReceipt {
    pub migration_id: String,
    pub input_sha256: String,
    pub row_counts: BTreeMap<String, u64>,
    pub verification_sha256: String,
    pub already_imported: bool,
}

pub(crate) trait GitObjectLookup {
    fn object_exists(&self, oid: &str) -> Result<bool, String>;
}

pub(crate) trait LegacyImportSink {
    fn existing_receipt(
        &mut self,
        migration_id: &str,
    ) -> Result<Option<LegacyMigrationReceipt>, String>;
    fn begin_import(&mut self, plan: &LegacyImportPlan) -> Result<(), String>;
    fn put_id_mapping(&mut self, mapping: &LegacyIdMapEntry) -> Result<(), String>;
    fn put_entity(&mut self, entity: &ImportedEntity) -> Result<(), String>;
    fn commit_import(
        &mut self,
        receipt: &LegacyMigrationReceipt,
    ) -> Result<(), String>;
    fn abort_import(&mut self);
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
        let raw: Value = serde_json::from_slice(&repo_bytes).map_err(|source| {
            LegacyImportError::Json { path: repo_path.clone(), source }
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
        let repo: LegacyRepoV1 = serde_json::from_value(raw.clone()).map_err(|source| {
            LegacyImportError::Json { path: repo_path.clone(), source }
        })?;
        validate_repo(&repo)?;

        let input_sha256 = inventory_digest(&files);
        let source_id = format!("repo-v1:{}:{}", repo.safe_space_id, repo.created_at);
        let migration_id = format!("legacy-v1-{}", &input_sha256[..24]);
        let source_sha = sha256_hex(&repo_bytes);
        let mut id_map = Vec::new();
        let mut entities = Vec::new();
        let mut warnings = Vec::new();
        let mut quarantined = Vec::new();
        let mut required_oids = BTreeSet::new();

        push_entity(&source_id, &source_sha, "repository", &repo.safe_space_id, 0, &repo, &mut id_map, &mut entities)?;

        for (ordinal, state) in repo.states.iter().enumerate() {
            required_oids.insert(state.commit.clone());
            if let Some(parent) = &state.parent_commit {
                required_oids.insert(parent.clone());
            }
            let mut missing = Vec::new();
            for oid in [&state.commit, state.parent_commit.as_ref().unwrap_or(&state.commit)] {
                if !git.object_exists(oid).map_err(LegacyImportError::Sink)? {
                    missing.push(oid.clone());
                }
            }
            missing.sort();
            missing.dedup();
            if !missing.is_empty() {
                return Err(LegacyImportError::MissingGitObjects {
                    state_id: state.id.clone(),
                    oids: missing,
                });
            }
            push_entity(&source_id, &source_sha, "state", &state.id, ordinal as u64, state, &mut id_map, &mut entities)?;
        }

        for (ordinal, (key, lane)) in repo.lanes.iter().enumerate() {
            if lane.name != *key {
                warnings.push(format!("lane key `{key}` differs from stored name `{}`", lane.name));
            }
            push_entity(&source_id, &source_sha, "attempt", key, ordinal as u64, lane, &mut id_map, &mut entities)?;
        }

        let mut branches = BTreeSet::new();
        branches.extend(repo.branch_lane_map.keys().cloned());
        branches.extend(repo.states.iter().map(|state| state.branch.clone()));
        branches.extend(repo.lanes.values().map(|lane| lane.branch.clone()));
        for (ordinal, branch) in branches.into_iter().enumerate() {
            push_entity(&source_id, &source_sha, "branch", &branch, ordinal as u64, &branch, &mut id_map, &mut entities)?;
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
            push_entity(&source_id, &source_sha, "timeshift", &capture.id, ordinal as u64, capture, &mut id_map, &mut entities)?;
        }

        for (ordinal, freeze) in repo.freezes.iter().enumerate() {
            let bundle = safe_source_path(&jjk_root, &freeze.bundle_path);
            let manifest = safe_source_path(&jjk_root, &freeze.manifest_path);
            match (bundle, manifest) {
                (Ok(bundle), Ok(manifest)) if bundle.is_file() && manifest.is_file() => {
                    push_entity(&source_id, &source_sha, "legacy_freeze", &freeze.id, ordinal as u64, freeze, &mut id_map, &mut entities)?;
                }
                _ => quarantine_value(
                    "repo.json",
                    "legacy_freeze",
                    &freeze.id,
                    "freeze pair missing or path escapes .jjk",
                    freeze,
                    &mut quarantined,
                )?,
            }
        }

        if let Some(history) = read_optional_json::<LegacySnapshotHistory>(&jjk_root.join("history.json"))? {
            validate_history(&history)?;
            for (ordinal, snapshot) in history.entries.iter().enumerate() {
                validate_snapshot(snapshot, &format!("history.entries[{ordinal}]"))?;
                push_entity(&source_id, &source_sha, "control_snapshot", &snapshot.id, ordinal as u64, snapshot, &mut id_map, &mut entities)?;
            }
        }

        let backups = jjk_root.join("backups");
        if backups.is_dir() {
            let mut paths = fs::read_dir(&backups)
                .map_err(|source| LegacyImportError::Read { path: backups.clone(), source })?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|v| v.to_str()) == Some("json"))
                .collect::<Vec<_>>();
            paths.sort();
            for (ordinal, path) in paths.iter().enumerate() {
                let snapshot: LegacyWorkspaceSnapshot = read_json(path)?;
                validate_snapshot(&snapshot, &format!("backups/{}", file_name(path)))?;
                push_entity(&source_id, &source_sha, "legacy_backup", &snapshot.id, ordinal as u64, &snapshot, &mut id_map, &mut entities)?;
            }
        }

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

    pub(crate) fn preserve_sources(&self, capsule: &Path) -> Result<(), LegacyImportError> {
        if capsule.exists() {
            verify_capsule(capsule, &self.files)?;
            return Ok(());
        }
        let staging = capsule.with_extension("staging");
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|source| LegacyImportError::Preserve { path: staging.clone(), source })?;
        }
        fs::create_dir_all(&staging).map_err(|source| LegacyImportError::Preserve { path: staging.clone(), source })?;
        for source in &self.files {
            let from = self.source_root.join(&source.relative_path);
            let to = staging.join(&source.relative_path);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|error| LegacyImportError::Preserve { path: parent.to_path_buf(), source: error })?;
            }
            let bytes = read_bytes(&from)?;
            fs::write(&to, bytes).map_err(|error| LegacyImportError::Preserve { path: to.clone(), source: error })?;
        }
        verify_capsule(&staging, &self.files)?;
        if let Some(parent) = capsule.parent() {
            fs::create_dir_all(parent).map_err(|source| LegacyImportError::Preserve { path: parent.to_path_buf(), source })?;
        }
        fs::rename(&staging, capsule).map_err(|source| LegacyImportError::Preserve { path: capsule.to_path_buf(), source })?;
        Ok(())
    }

    pub(crate) fn apply(
        &self,
        sink: &mut dyn LegacyImportSink,
    ) -> Result<LegacyMigrationReceipt, LegacyImportError> {
        if let Some(mut receipt) = sink.existing_receipt(&self.migration_id).map_err(LegacyImportError::Sink)? {
            if receipt.input_sha256 != self.input_sha256 {
                return Err(LegacyImportError::Invalid {
                    path: "migration receipt".into(),
                    message: "migration ID already exists for different source bytes".into(),
                });
            }
            receipt.already_imported = true;
            return Ok(receipt);
        }
        sink.begin_import(self).map_err(LegacyImportError::Sink)?;
        let result = (|| {
            for mapping in &self.id_map {
                sink.put_id_mapping(mapping).map_err(LegacyImportError::Sink)?;
            }
            for entity in &self.entities {
                sink.put_entity(entity).map_err(LegacyImportError::Sink)?;
            }
            let mut row_counts = BTreeMap::new();
            for entity in &self.entities {
                *row_counts.entry(entity.entity_kind.clone()).or_insert(0) += 1;
            }
            let verification_sha256 = plan_verification_digest(self, &row_counts)?;
            let receipt = LegacyMigrationReceipt {
                migration_id: self.migration_id.clone(),
                input_sha256: self.input_sha256.clone(),
                row_counts,
                verification_sha256,
                already_imported: false,
            };
            sink.commit_import(&receipt).map_err(LegacyImportError::Sink)?;
            Ok(receipt)
        })();
        if result.is_err() {
            sink.abort_import();
        }
        result
    }
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
    let state_ids = repo.states.iter().map(|state| state.id.as_str()).collect::<BTreeSet<_>>();
    if state_ids.len() != repo.states.len() {
        return invalid("states", "contains duplicate state IDs");
    }
    for (index, state) in repo.states.iter().enumerate() {
        validate_state(state, index, &state_ids)?;
    }
    for (key, lane) in &repo.lanes {
        timestamp(&format!("lanes.{key}.createdAt"), &lane.created_at)?;
        timestamp(&format!("lanes.{key}.updatedAt"), &lane.updated_at)?;
        if let Some(tip) = &lane.current_state_id {
            if !state_ids.contains(tip.as_str()) {
                return invalid(&format!("lanes.{key}.currentStateId"), "references unknown state");
            }
        }
    }
    for (branch, lane) in &repo.branch_lane_map {
        if !repo.lanes.contains_key(lane) {
            return invalid(&format!("branchLaneMap.{branch}"), "references unknown lane");
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
    Ok(())
}

fn validate_state(
    state: &LegacyState,
    index: usize,
    state_ids: &BTreeSet<&str>,
) -> Result<(), LegacyImportError> {
    let root = format!("states[{index}]");
    nonempty(&format!("{root}.id"), &state.id)?;
    if !matches!(state.kind.as_str(), "new" | "git" | "save" | "stash" | "cherry" | "step" | "nice" | "star" | "auto") {
        return invalid(&format!("{root}.kind"), "unknown legacy state kind");
    }
    timestamp(&format!("{root}.createdAt"), &state.created_at)?;
    git_oid(&format!("{root}.commit"), &state.commit)?;
    if let Some(parent) = &state.parent_commit {
        git_oid(&format!("{root}.parentCommit"), parent)?;
    }
    if let Some(parent) = &state.parent_state_id {
        if !state_ids.contains(parent.as_str()) {
            return invalid(&format!("{root}.parentStateId"), "references unknown state");
        }
    }
    if state.stats.changed_files < 0 || state.stats.inserted_lines.is_some_and(|v| v < 0) || state.stats.deleted_lines.is_some_and(|v| v < 0) {
        return invalid(&format!("{root}.stats"), "counts cannot be negative");
    }
    if let Some(metadata) = &state.metadata {
        if let Some(oid) = &metadata.git_commit {
            git_oid(&format!("{root}.metadata.gitCommit"), oid)?;
            if oid != &state.commit {
                return invalid(&format!("{root}.metadata.gitCommit"), "does not equal state commit");
            }
        }
        if let Some(deleted_at) = &metadata.deleted_at {
            timestamp(&format!("{root}.metadata.deletedAt"), deleted_at)?;
        }
        for (ordinal, context) in metadata.prior_contexts.iter().enumerate() {
            timestamp(&format!("{root}.metadata.priorContexts[{ordinal}].updatedAt"), &context.updated_at)?;
        }
    }
    Ok(())
}

fn validate_navigation(history: &LegacyNavigation, states: &BTreeSet<&str>) -> Result<(), LegacyImportError> {
    if history.entries.is_empty() {
        if history.index != -1 {
            return invalid("currentStateHistory.index", "must be -1 for empty history");
        }
    } else if history.index < 0 || history.index as usize >= history.entries.len() {
        return invalid("currentStateHistory.index", "outside entries bounds");
    }
    if history.entries.iter().any(|id| !states.contains(id.as_str())) {
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
    if !history.entries.is_empty() && (history.index < 0 || history.index as usize >= history.entries.len()) {
        return invalid("history.index", "outside entries bounds");
    }
    Ok(())
}

fn validate_snapshot(snapshot: &LegacyWorkspaceSnapshot, root: &str) -> Result<(), LegacyImportError> {
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
    let bytes = serde_json::to_vec(value).map_err(|error| LegacyImportError::Invalid {
        path: format!("{kind}.{key}"),
        message: error.to_string(),
    })?;
    quarantined.push(QuarantinedRecord {
        source_path: source_path.into(),
        entity_kind: kind.into(),
        legacy_key: key.into(),
        reason: reason.into(),
        raw_sha256: sha256_hex(&bytes),
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
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let prefix = match kind {
        "repository" => "repo",
        "state" => "st",
        "attempt" => "at",
        "branch" => "br",
        "timeshift" => "tsh",
        "legacy_freeze" => "frz",
        "legacy_backup" => "bkp",
        "control_snapshot" => "snap",
        _ => "id",
    };
    format!("{prefix}_{}", Uuid::from_bytes(bytes))
}

fn inventory(jjk_root: &Path) -> Result<Vec<SourceFile>, LegacyImportError> {
    let mut paths = vec![jjk_root.join("repo.json")];
    for optional in [jjk_root.join("history.json")] {
        if optional.is_file() {
            paths.push(optional);
        }
    }
    for directory in [jjk_root.join("backups"), jjk_root.join("freezes")] {
        if directory.is_dir() {
            let mut children = fs::read_dir(&directory)
                .map_err(|source| LegacyImportError::Read { path: directory.clone(), source })?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            children.sort();
            paths.extend(children);
        }
    }
    paths.sort();
    let mut result = Vec::new();
    for path in paths {
        let metadata = fs::symlink_metadata(&path).map_err(|source| LegacyImportError::Read { path: path.clone(), source })?;
        if metadata.file_type().is_symlink() {
            return invalid(&path.display().to_string(), "legacy source may not be a symlink");
        }
        let bytes = read_bytes(&path)?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        };
        #[cfg(not(unix))]
        let mode = 0;
        result.push(SourceFile {
            relative_path: path.strip_prefix(jjk_root).expect("inventory rooted beneath .jjk").to_string_lossy().replace('\\', "/"),
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

fn verify_capsule(capsule: &Path, files: &[SourceFile]) -> Result<(), LegacyImportError> {
    for source in files {
        let path = capsule.join(&source.relative_path);
        let bytes = read_bytes(&path)?;
        if bytes.len() as u64 != source.size_bytes || sha256_hex(&bytes) != source.sha256 {
            return Err(LegacyImportError::PreserveDigest(path));
        }
    }
    Ok(())
}

fn plan_verification_digest(
    plan: &LegacyImportPlan,
    row_counts: &BTreeMap<String, u64>,
) -> Result<String, LegacyImportError> {
    let bytes = serde_json::to_vec(&(plan.input_sha256.as_str(), &plan.id_map, &plan.entities, row_counts))
        .map_err(|error| LegacyImportError::Invalid { path: "verification".into(), message: error.to_string() })?;
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
        let canonical_root = root.canonicalize().map_err(|source| LegacyImportError::Read { path: root.to_path_buf(), source })?;
        if !canonical.starts_with(canonical_root) {
            return invalid(raw.to_string_lossy().as_ref(), "symlink escapes .jjk");
        }
    }
    Ok(path)
}

fn safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path.components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn read_optional_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, LegacyImportError> {
    if !path.is_file() {
        return Ok(None);
    }
    read_json(path).map(Some)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, LegacyImportError> {
    let bytes = read_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|source| LegacyImportError::Json { path: path.to_path_buf(), source })
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, LegacyImportError> {
    fs::read(path).map_err(|source| LegacyImportError::Read { path: path.to_path_buf(), source })
}

fn timestamp(path: &str, value: &str) -> Result<(), LegacyImportError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|error| LegacyImportError::Invalid { path: path.into(), message: format!("invalid RFC3339 timestamp: {error}") })
}

fn git_oid(path: &str, oid: &str) -> Result<(), LegacyImportError> {
    if matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        invalid(path, "must be a 40- or 64-hex Git object ID")
    }
}

fn nonempty(path: &str, value: &str) -> Result<(), LegacyImportError> {
    if value.is_empty() { invalid(path, "must not be empty") } else { Ok(()) }
}

fn invalid<T>(path: &str, message: &str) -> Result<T, LegacyImportError> {
    Err(LegacyImportError::Invalid { path: path.into(), message: message.into() })
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_name(path: &Path) -> String {
    path.file_name().and_then(|value| value.to_str()).unwrap_or("<non-utf8>").into()
}
