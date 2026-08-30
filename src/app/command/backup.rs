//! Checksummed backup/load and portable freeze services.
//!
//! SQLite is captured only through [`BackupStore::online_backup_to`]. Restore
//! is preview-first, re-verifies at apply time, and publishes only a new target.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, thiserror::Error)]
pub(crate) enum BackupError {
    #[error("backup store error: {0}")]
    Store(String),
    #[error("Git bundle error: {0}")]
    Git(String),
    #[error("artifact I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid artifact: {0}")]
    Invalid(String),
    #[error("checksum mismatch for {0}")]
    Checksum(PathBuf),
    #[error("target already exists: {0}; load requires a new target")]
    ExistingTarget(PathBuf),
    #[error("missing required Git objects: {0:?}")]
    MissingObjects(Vec<String>),
    #[error("artifact changed after preview")]
    PreviewChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SchemaIdentity {
    pub format: String,
    pub major: u16,
    pub minor: u16,
    pub migration_set_sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournalHeadManifest {
    pub through_seq: u64,
    pub through_event_hash: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupBoundary {
    pub repository_id: String,
    pub schema: SchemaIdentity,
    pub journal_head: JournalHeadManifest,
    pub operation_boundary: String,
}

pub(crate) trait BackupStore {
    /// Creates a committed database snapshot through SQLite's online backup API.
    fn online_backup_to(&self, destination: &Path) -> Result<BackupBoundary, String>;
    /// Opens and verifies integrity, foreign keys, schema, head, and operations.
    fn verify_backup(&self, database: &Path) -> Result<BackupBoundary, String>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ArtifactDigest {
    pub path: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub required: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupManifestV1 {
    pub format: String,
    pub format_version: u16,
    pub backup_id: String,
    pub repository_id: String,
    pub created_at_utc: String,
    pub created_by_version: String,
    pub schema: SchemaIdentity,
    pub journal_head: JournalHeadManifest,
    pub operation_boundary: String,
    pub git_object_format: String,
    pub privacy: String,
    pub artifacts: Vec<ArtifactDigest>,
    pub required_oids: Vec<String>,
    pub refs_sha256: String,
    pub restore_capabilities: Vec<String>,
    pub source_backup_id: Option<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct BackupCreateRequest<'a> {
    pub backup_id: &'a str,
    pub destination: &'a Path,
    pub created_at_utc: &'a str,
    pub created_by_version: &'a str,
    pub refs_json: &'a [u8],
    pub git_bundle: &'a Path,
    pub git_object_format: &'a str,
    pub required_oids: &'a [String],
    pub source_backup_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BackupVerification {
    pub manifest: BackupManifestV1,
    pub manifest_sha256: String,
    pub total_bytes: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GitBundleVerification {
    pub missing_oids: Vec<String>,
}

pub(crate) trait GitBundleVerifier {
    /// Verifies the bundle and proves every required OID through an isolated import.
    fn verify_bundle(
        &self,
        bundle: &Path,
        required_oids: &[String],
    ) -> Result<GitBundleVerification, String>;
}
pub(crate) trait GitBundleRestorer: GitBundleVerifier {
    /// Initializes/imports Git and restores the checksummed ref image.
    fn restore_bundle(
        &self,
        bundle: &Path,
        refs_json: &[u8],
        target: &Path,
    ) -> Result<PathBuf, String>;
    fn verify_restored_objects(
        &self,
        git_common_dir: &Path,
        required_oids: &[String],
    ) -> Result<Vec<String>, String>;
}

pub(crate) fn create_backup(
    store: &dyn BackupStore,
    git: &dyn GitBundleVerifier,
    request: &BackupCreateRequest<'_>,
) -> Result<BackupVerification, BackupError> {
    timestamp(request.created_at_utc)?;
    validate_object_format(request.git_object_format)?;
    validate_oids(request.git_object_format, request.required_oids)?;
    serde_json::from_slice::<Value>(request.refs_json)
        .map_err(|error| BackupError::Invalid(format!("refs JSON: {error}")))?;
    ensure_new_destination(request.destination)?;
    let staging = sibling_staging(request.destination);
    remove_if_exists(&staging)?;
    private_dir(&staging)?;
    let result = (|| {
        let database = staging.join("metadata/state.sqlite3");
        private_parent(&database)?;
        let boundary = store
            .online_backup_to(&database)
            .map_err(BackupError::Store)?;
        if store.verify_backup(&database).map_err(BackupError::Store)? != boundary {
            return Err(BackupError::Invalid(
                "online backup boundary changed during verification".into(),
            ));
        }
        write_private(&staging.join("git/refs.json"), request.refs_json)?;
        copy_private(request.git_bundle, &staging.join("git/objects.bundle"))?;
        require_closure(
            git.verify_bundle(&staging.join("git/objects.bundle"), request.required_oids)
                .map_err(BackupError::Git)?,
        )?;
        let mut artifacts = vec![
            digest_artifact(
                &staging,
                "metadata/state.sqlite3",
                "application/vnd.sqlite3",
            )?,
            digest_artifact(&staging, "git/refs.json", "application/json")?,
            digest_artifact(&staging, "git/objects.bundle", "application/x-git-bundle")?,
        ];
        artifacts.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest = BackupManifestV1 {
            format: "jjk-backup".into(),
            format_version: 1,
            backup_id: request.backup_id.into(),
            repository_id: boundary.repository_id,
            created_at_utc: request.created_at_utc.into(),
            created_by_version: request.created_by_version.into(),
            schema: boundary.schema,
            journal_head: boundary.journal_head,
            operation_boundary: boundary.operation_boundary,
            git_object_format: request.git_object_format.into(),
            privacy: "project-private+local-sensitive".into(),
            artifacts,
            required_oids: sorted_unique(request.required_oids),
            refs_sha256: sha256_hex(request.refs_json),
            restore_capabilities: vec!["new-target".into()],
            source_backup_id: request.source_backup_id.clone(),
        };
        write_manifest(&staging, &manifest)?;
        let verified = verify_backup_artifact(&staging, store, git)?;
        fs::rename(&staging, request.destination).map_err(|source| BackupError::Io {
            path: request.destination.to_path_buf(),
            source,
        })?;
        Ok(verified)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub(crate) fn verify_backup_artifact(
    root: &Path,
    store: &dyn BackupStore,
    git: &dyn GitBundleVerifier,
) -> Result<BackupVerification, BackupError> {
    verify_backup_artifact_with(root, store, |bundle, required_oids| {
        git.verify_bundle(bundle, required_oids)
    })
}

fn verify_backup_artifact_with(
    root: &Path,
    store: &dyn BackupStore,
    verify_bundle: impl FnOnce(&Path, &[String]) -> Result<GitBundleVerification, String>,
) -> Result<BackupVerification, BackupError> {
    let (manifest_bytes, manifest_sha256) = verified_manifest_bytes(root)?;
    let manifest: BackupManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| BackupError::Invalid(format!("manifest JSON: {error}")))?;
    validate_backup_manifest(&manifest)?;
    verify_artifacts(root, &manifest.artifacts, &backup_paths())?;
    let refs = regular_file_bytes(&root.join("git/refs.json"))?;
    serde_json::from_slice::<Value>(&refs)
        .map_err(|error| BackupError::Invalid(format!("refs JSON: {error}")))?;
    if sha256_hex(&refs) != manifest.refs_sha256 {
        return Err(BackupError::Checksum(root.join("git/refs.json")));
    }
    let boundary = store
        .verify_backup(&root.join("metadata/state.sqlite3"))
        .map_err(BackupError::Store)?;
    if boundary.repository_id != manifest.repository_id
        || boundary.schema != manifest.schema
        || boundary.journal_head != manifest.journal_head
        || boundary.operation_boundary != manifest.operation_boundary
    {
        return Err(BackupError::Invalid(
            "database boundary does not match manifest".into(),
        ));
    }
    require_closure(
        verify_bundle(&root.join("git/objects.bundle"), &manifest.required_oids)
            .map_err(BackupError::Git)?,
    )?;
    let total_bytes = manifest_bytes.len() as u64
        + manifest
            .artifacts
            .iter()
            .map(|item| item.size_bytes)
            .sum::<u64>();
    Ok(BackupVerification {
        manifest,
        manifest_sha256,
        total_bytes,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoadPreview {
    pub manifest: BackupManifestV1,
    pub manifest_sha256: String,
    pub target: PathBuf,
    pub target_exists: bool,
    pub required_bytes: u64,
    pub missing_oids: Vec<String>,
    pub mutation_allowed: bool,
}
pub(crate) fn preview_load(
    artifact: &Path,
    target: &Path,
    store: &dyn BackupStore,
    git: &dyn GitBundleVerifier,
) -> Result<LoadPreview, BackupError> {
    let verified = verify_backup_artifact(artifact, store, git)?;
    let target_exists = target.exists();
    Ok(LoadPreview {
        manifest: verified.manifest,
        manifest_sha256: verified.manifest_sha256,
        target: target.to_owned(),
        target_exists,
        required_bytes: verified.total_bytes,
        missing_oids: Vec::new(),
        mutation_allowed: !target_exists,
    })
}
pub(crate) fn load_into_new_target(
    artifact: &Path,
    preview: &LoadPreview,
    store: &dyn BackupStore,
    git: &dyn GitBundleRestorer,
) -> Result<PathBuf, BackupError> {
    if preview.target.exists() {
        return Err(BackupError::ExistingTarget(preview.target.clone()));
    }
    let verified = verify_backup_artifact_with(artifact, store, |bundle, required_oids| {
        git.verify_bundle(bundle, required_oids)
    })?;
    if verified.manifest_sha256 != preview.manifest_sha256 || verified.manifest != preview.manifest
    {
        return Err(BackupError::PreviewChanged);
    }
    let staging = sibling_staging(&preview.target);
    remove_if_exists(&staging)?;
    private_dir(&staging)?;
    let result = (|| {
        let refs = regular_file_bytes(&artifact.join("git/refs.json"))?;
        let common_dir = git
            .restore_bundle(&artifact.join("git/objects.bundle"), &refs, &staging)
            .map_err(BackupError::Git)?;
        let canonical_staging = staging.canonicalize().map_err(|source| BackupError::Io {
            path: staging.clone(),
            source,
        })?;
        let canonical_common = common_dir
            .canonicalize()
            .map_err(|source| BackupError::Io {
                path: common_dir.clone(),
                source,
            })?;
        if !canonical_common.starts_with(&canonical_staging) {
            return Err(BackupError::Invalid(
                "Git common directory escaped staging".into(),
            ));
        }
        let missing = git
            .verify_restored_objects(&common_dir, &verified.manifest.required_oids)
            .map_err(BackupError::Git)?;
        if !missing.is_empty() {
            return Err(BackupError::MissingObjects(missing));
        }
        let database = common_dir.join("jjk/state.sqlite3");
        copy_private(&artifact.join("metadata/state.sqlite3"), &database)?;
        let installed = store.verify_backup(&database).map_err(BackupError::Store)?;
        if installed.repository_id != verified.manifest.repository_id
            || installed.schema != verified.manifest.schema
            || installed.journal_head != verified.manifest.journal_head
            || installed.operation_boundary != verified.manifest.operation_boundary
        {
            return Err(BackupError::Invalid(
                "installed database boundary differs from backup".into(),
            ));
        }
        fs::rename(&staging, &preview.target).map_err(|source| BackupError::Io {
            path: preview.target.clone(),
            source,
        })?;
        Ok(preview.target.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub(crate) fn create_preload_recovery(
    store: &dyn BackupStore,
    git: &dyn GitBundleVerifier,
    request: &BackupCreateRequest<'_>,
    operation_id: &str,
) -> Result<BackupVerification, BackupError> {
    let backup_id = format!("pre-load-{operation_id}");
    let preload = BackupCreateRequest {
        backup_id: &backup_id,
        destination: request.destination,
        created_at_utc: request.created_at_utc,
        created_by_version: request.created_by_version,
        refs_json: request.refs_json,
        git_bundle: request.git_bundle,
        git_object_format: request.git_object_format,
        required_oids: request.required_oids,
        source_backup_id: request.source_backup_id.clone(),
    };
    create_backup(store, git, &preload)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FreezeManifestV1 {
    pub format: String,
    pub format_version: u16,
    pub freeze_id: String,
    pub origin_repository_id: String,
    pub origin_replica_id: String,
    pub created_at_utc: String,
    pub created_by_version: String,
    pub git_object_format: String,
    pub root_states: Vec<String>,
    pub attempts: Vec<String>,
    pub included_state_ids: Vec<String>,
    pub included_edge_ids: Vec<String>,
    pub included_event_ids: Vec<String>,
    pub boundary_parents: Vec<String>,
    pub required_oids: Vec<String>,
    pub offered_refs: Vec<String>,
    pub artifacts: Vec<ArtifactDigest>,
    pub privacy: String,
    pub required_capabilities: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct FreezeCreateRequest<'a> {
    pub freeze_id: &'a str,
    pub destination: &'a Path,
    pub origin_repository_id: &'a str,
    pub origin_replica_id: &'a str,
    pub created_at_utc: &'a str,
    pub created_by_version: &'a str,
    pub git_object_format: &'a str,
    pub metadata_events: &'a [u8],
    pub metadata_view: &'a Value,
    pub git_bundle: &'a Path,
    pub root_states: &'a [String],
    pub attempts: &'a [String],
    pub included_state_ids: &'a [String],
    pub included_edge_ids: &'a [String],
    pub included_event_ids: &'a [String],
    pub boundary_parents: &'a [String],
    pub required_oids: &'a [String],
}
pub(crate) fn create_freeze(
    request: &FreezeCreateRequest<'_>,
    git: &dyn GitBundleVerifier,
) -> Result<FreezeManifestV1, BackupError> {
    timestamp(request.created_at_utc)?;
    validate_object_format(request.git_object_format)?;
    validate_oids(request.git_object_format, request.required_oids)?;
    ensure_new_destination(request.destination)?;
    let staging = sibling_staging(request.destination);
    remove_if_exists(&staging)?;
    private_dir(&staging)?;
    let result = (|| {
        write_private(
            &staging.join("metadata/events.cbor"),
            request.metadata_events,
        )?;
        write_private(
            &staging.join("metadata/view.json"),
            &canonical_json_bytes(request.metadata_view)?,
        )?;
        copy_private(request.git_bundle, &staging.join("git/objects.bundle"))?;
        require_closure(
            git.verify_bundle(&staging.join("git/objects.bundle"), request.required_oids)
                .map_err(BackupError::Git)?,
        )?;
        let mut artifacts = vec![
            digest_artifact(&staging, "metadata/events.cbor", "application/cbor")?,
            digest_artifact(&staging, "metadata/view.json", "application/json")?,
            digest_artifact(&staging, "git/objects.bundle", "application/x-git-bundle")?,
        ];
        artifacts.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest = FreezeManifestV1 {
            format: "jjk-freeze".into(),
            format_version: 1,
            freeze_id: request.freeze_id.into(),
            origin_repository_id: request.origin_repository_id.into(),
            origin_replica_id: request.origin_replica_id.into(),
            created_at_utc: request.created_at_utc.into(),
            created_by_version: request.created_by_version.into(),
            git_object_format: request.git_object_format.into(),
            root_states: sorted_unique(request.root_states),
            attempts: sorted_unique(request.attempts),
            included_state_ids: sorted_unique(request.included_state_ids),
            included_edge_ids: sorted_unique(request.included_edge_ids),
            included_event_ids: sorted_unique(request.included_event_ids),
            boundary_parents: sorted_unique(request.boundary_parents),
            required_oids: sorted_unique(request.required_oids),
            offered_refs: request
                .root_states
                .iter()
                .map(|state| format!("refs/jjk/imports/{}/{state}", request.freeze_id))
                .collect(),
            artifacts,
            privacy: "project-private; local-sensitive excluded".into(),
            required_capabilities: vec!["git-bundle-v2".into()],
        };
        write_manifest(&staging, &manifest)?;
        inspect_freeze(&staging, git)?;
        fs::rename(&staging, request.destination).map_err(|source| BackupError::Io {
            path: request.destination.to_path_buf(),
            source,
        })?;
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}
pub(crate) fn inspect_freeze(
    root: &Path,
    git: &dyn GitBundleVerifier,
) -> Result<FreezeManifestV1, BackupError> {
    let (bytes, _) = verified_manifest_bytes(root)?;
    let manifest: FreezeManifestV1 = serde_json::from_slice(&bytes)
        .map_err(|error| BackupError::Invalid(format!("freeze manifest JSON: {error}")))?;
    if manifest.format != "jjk-freeze" || manifest.format_version != 1 {
        return Err(BackupError::Invalid("not a JJK freeze v1".into()));
    }
    timestamp(&manifest.created_at_utc)?;
    validate_object_format(&manifest.git_object_format)?;
    validate_oids(&manifest.git_object_format, &manifest.required_oids)?;
    verify_artifacts(root, &manifest.artifacts, &freeze_paths())?;
    require_closure(
        git.verify_bundle(&root.join("git/objects.bundle"), &manifest.required_oids)
            .map_err(BackupError::Git)?,
    )?;
    Ok(manifest)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FreezeImportPreview {
    pub manifest: FreezeManifestV1,
    pub quarantine_refs: Vec<String>,
}
pub(crate) fn preview_freeze_import(
    root: &Path,
    git: &dyn GitBundleVerifier,
) -> Result<FreezeImportPreview, BackupError> {
    let manifest = inspect_freeze(root, git)?;
    let quarantine_refs = manifest.offered_refs.clone();
    Ok(FreezeImportPreview {
        manifest,
        quarantine_refs,
    })
}

fn validate_backup_manifest(manifest: &BackupManifestV1) -> Result<(), BackupError> {
    if manifest.format != "jjk-backup" || manifest.format_version != 1 {
        return Err(BackupError::Invalid("not a JJK backup v1".into()));
    }
    timestamp(&manifest.created_at_utc)?;
    validate_object_format(&manifest.git_object_format)?;
    validate_oids(&manifest.git_object_format, &manifest.required_oids)?;
    if manifest.schema.format != "jjk-store" {
        return Err(BackupError::Invalid("unknown store schema format".into()));
    }
    validate_sha256("migration set", &manifest.schema.migration_set_sha256)?;
    validate_sha256("journal head", &manifest.journal_head.through_event_hash)?;
    validate_sha256("refs", &manifest.refs_sha256)
}
fn write_manifest<T: Serialize>(root: &Path, manifest: &T) -> Result<(), BackupError> {
    let value = serde_json::to_value(manifest).map_err(|e| BackupError::Invalid(e.to_string()))?;
    let bytes = canonical_json_bytes(&value)?;
    write_private(&root.join("manifest.json"), &bytes)?;
    write_private(
        &root.join("manifest.sha256"),
        format!("{}\n", sha256_hex(&bytes)).as_bytes(),
    )
}
fn verified_manifest_bytes(root: &Path) -> Result<(Vec<u8>, String), BackupError> {
    let bytes = regular_file_bytes(&root.join("manifest.json"))?;
    let checksum = String::from_utf8(regular_file_bytes(&root.join("manifest.sha256"))?)
        .map_err(|_| BackupError::Invalid("manifest checksum is not UTF-8".into()))?;
    let expected = checksum.trim();
    validate_sha256("manifest", expected)?;
    let actual = sha256_hex(&bytes);
    if expected != actual {
        return Err(BackupError::Checksum(root.join("manifest.json")));
    }
    Ok((bytes, actual))
}
fn verify_artifacts(
    root: &Path,
    artifacts: &[ArtifactDigest],
    expected: &BTreeSet<String>,
) -> Result<(), BackupError> {
    let mut paths = BTreeSet::new();
    for item in artifacts {
        validated_artifact_path(&item.path)?;
        validate_sha256("artifact", &item.sha256)?;
        if !item.required {
            return Err(BackupError::Invalid(format!(
                "artifact marked optional: {}",
                item.path
            )));
        }
        if !paths.insert(case_fold_path(&item.path)) {
            return Err(BackupError::Invalid(format!(
                "duplicate artifact path `{}`",
                item.path
            )));
        }
        let bytes = regular_file_bytes(&root.join(&item.path))?;
        if bytes.len() as u64 != item.size_bytes || sha256_hex(&bytes) != item.sha256 {
            return Err(BackupError::Checksum(root.join(&item.path)));
        }
    }
    let declared = artifacts
        .iter()
        .map(|item| item.path.clone())
        .collect::<BTreeSet<_>>();
    if &declared != expected {
        return Err(BackupError::Invalid(format!(
            "artifact set mismatch: expected {expected:?}, found {declared:?}"
        )));
    }
    Ok(())
}
fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, BackupError> {
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                let mut out = serde_json::Map::new();
                for key in keys {
                    out.insert(key.clone(), canonical(&map[key]));
                }
                Value::Object(out)
            }
            Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_vec(&canonical(value)).map_err(|e| BackupError::Invalid(e.to_string()))
}
fn digest_artifact(
    root: &Path,
    relative: &str,
    media_type: &str,
) -> Result<ArtifactDigest, BackupError> {
    validated_artifact_path(relative)?;
    let bytes = regular_file_bytes(&root.join(relative))?;
    Ok(ArtifactDigest {
        path: relative.into(),
        media_type: media_type.into(),
        size_bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        required: true,
    })
}
fn regular_file_bytes(path: &Path) -> Result<Vec<u8>, BackupError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| BackupError::Io {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(BackupError::Invalid(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    read(path)
}
fn validated_artifact_path(path: &str) -> Result<&Path, BackupError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(BackupError::Invalid(format!(
            "unsafe artifact path `{}`",
            path.display()
        )));
    }
    Ok(path)
}
fn validate_object_format(value: &str) -> Result<(), BackupError> {
    if matches!(value, "sha1" | "sha256") {
        Ok(())
    } else {
        Err(BackupError::Invalid(format!(
            "unsupported Git object format `{value}`"
        )))
    }
}
fn validate_oids(format: &str, values: &[String]) -> Result<(), BackupError> {
    let length = if format == "sha1" { 40 } else { 64 };
    if values
        .iter()
        .any(|oid| oid.len() != length || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        Err(BackupError::Invalid(format!(
            "required OID does not match {format}"
        )))
    } else {
        Ok(())
    }
}
fn validate_sha256(label: &str, value: &str) -> Result<(), BackupError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(BackupError::Invalid(format!(
            "{label} is not lowercase SHA-256"
        )))
    }
}
fn require_closure(mut result: GitBundleVerification) -> Result<(), BackupError> {
    result.missing_oids.sort();
    result.missing_oids.dedup();
    if result.missing_oids.is_empty() {
        Ok(())
    } else {
        Err(BackupError::MissingObjects(result.missing_oids))
    }
}
fn backup_paths() -> BTreeSet<String> {
    [
        "metadata/state.sqlite3",
        "git/refs.json",
        "git/objects.bundle",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn freeze_paths() -> BTreeSet<String> {
    [
        "metadata/events.cbor",
        "metadata/view.json",
        "git/objects.bundle",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
fn sorted_unique(values: &[String]) -> Vec<String> {
    values
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn timestamp(value: &str) -> Result<(), BackupError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|e| BackupError::Invalid(format!("invalid RFC3339 timestamp: {e}")))
}
fn ensure_new_destination(path: &Path) -> Result<(), BackupError> {
    if path.exists() {
        return Err(BackupError::ExistingTarget(path.to_owned()));
    }
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    Ok(())
}
fn sibling_staging(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        ".{}.staging",
        path.file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("artifact")
    ))
}
fn private_parent(path: &Path) -> Result<(), BackupError> {
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    Ok(())
}
fn private_dir(path: &Path) -> Result<(), BackupError> {
    fs::create_dir_all(path).map_err(|source| BackupError::Io {
        path: path.to_owned(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            BackupError::Io {
                path: path.to_owned(),
                source,
            }
        })?;
    }
    Ok(())
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    private_parent(path)?;
    fs::write(path, bytes).map_err(|source| BackupError::Io {
        path: path.to_owned(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            BackupError::Io {
                path: path.to_owned(),
                source,
            }
        })?;
    }
    Ok(())
}
fn copy_private(from: &Path, to: &Path) -> Result<(), BackupError> {
    write_private(to, &read(from)?)
}
fn read(path: &Path) -> Result<Vec<u8>, BackupError> {
    fs::read(path).map_err(|source| BackupError::Io {
        path: path.to_owned(),
        source,
    })
}
fn remove_if_exists(path: &Path) -> Result<(), BackupError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|source| BackupError::Io {
            path: path.to_owned(),
            source,
        })?;
    }
    Ok(())
}
fn case_fold_path(path: &str) -> String {
    path.to_lowercase()
}
fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[derive(Clone)]
    struct Store(BackupBoundary);
    impl BackupStore for Store {
        fn online_backup_to(&self, path: &Path) -> Result<BackupBoundary, String> {
            fs::write(path, b"sqlite-online-snapshot").map_err(|e| e.to_string())?;
            Ok(self.0.clone())
        }
        fn verify_backup(&self, path: &Path) -> Result<BackupBoundary, String> {
            (fs::read(path).map_err(|e| e.to_string())? == b"sqlite-online-snapshot")
                .then(|| self.0.clone())
                .ok_or_else(|| "invalid database".into())
        }
    }
    #[derive(Default)]
    struct Git(Vec<String>);
    impl GitBundleVerifier for Git {
        fn verify_bundle(&self, _: &Path, _: &[String]) -> Result<GitBundleVerification, String> {
            Ok(GitBundleVerification {
                missing_oids: self.0.clone(),
            })
        }
    }
    impl GitBundleRestorer for Git {
        fn restore_bundle(&self, _: &Path, _: &[u8], target: &Path) -> Result<PathBuf, String> {
            let dir = target.join(".git");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            Ok(dir)
        }
        fn verify_restored_objects(&self, _: &Path, _: &[String]) -> Result<Vec<String>, String> {
            Ok(self.0.clone())
        }
    }
    fn store() -> Store {
        Store(BackupBoundary {
            repository_id: "repo_test".into(),
            schema: SchemaIdentity {
                format: "jjk-store".into(),
                major: 1,
                minor: 0,
                migration_set_sha256: "11".repeat(32),
            },
            journal_head: JournalHeadManifest {
                through_seq: 4,
                through_event_hash: "22".repeat(32),
            },
            operation_boundary: "all-terminal".into(),
        })
    }
    fn create(temp: &Path, git: &Git) -> PathBuf {
        let bundle = temp.join("source.bundle");
        fs::write(&bundle, b"bundle").unwrap();
        let output = temp.join("backup.jjkbak");
        let required = vec!["11".repeat(20)];
        let request = BackupCreateRequest {
            backup_id: "bak_test",
            destination: &output,
            created_at_utc: "2026-08-28T00:00:00Z",
            created_by_version: "0.1.0",
            refs_json: br#"{"HEAD":"refs/heads/main"}"#,
            git_bundle: &bundle,
            git_object_format: "sha1",
            required_oids: &required,
            source_backup_id: None,
        };
        create_backup(&store(), git, &request).unwrap();
        output
    }
    #[test]
    fn corruption_fails_closed_and_existing_target_is_protected() {
        let temp = tempdir().unwrap();
        let git = Git::default();
        let artifact = create(temp.path(), &git);
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"safe").unwrap();
        let preview = preview_load(&artifact, &target, &store(), &git).unwrap();
        assert!(!preview.mutation_allowed);
        assert!(matches!(
            load_into_new_target(&artifact, &preview, &store(), &git),
            Err(BackupError::ExistingTarget(_))
        ));
        fs::write(artifact.join("git/refs.json"), b"tampered").unwrap();
        assert!(verify_backup_artifact(&artifact, &store(), &git).is_err());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"safe");
    }
    #[test]
    fn missing_object_closure_rejects_artifact() {
        let temp = tempdir().unwrap();
        let missing = "22".repeat(20);
        let git = Git(vec![missing.clone()]);
        let bundle = temp.path().join("source.bundle");
        fs::write(&bundle, b"bundle").unwrap();
        let output = temp.path().join("backup.jjkbak");
        let required = vec![missing];
        let request = BackupCreateRequest {
            backup_id: "bak",
            destination: &output,
            created_at_utc: "2026-08-28T00:00:00Z",
            created_by_version: "0.1.0",
            refs_json: b"{}",
            git_bundle: &bundle,
            git_object_format: "sha1",
            required_oids: &required,
            source_backup_id: None,
        };
        assert!(matches!(
            create_backup(&store(), &git, &request),
            Err(BackupError::MissingObjects(_))
        ));
        assert!(!output.exists());
    }
    #[test]
    fn preview_then_load_uses_git_common_control_root() {
        let temp = tempdir().unwrap();
        let git = Git::default();
        let artifact = create(temp.path(), &git);
        let target = temp.path().join("restored");
        let preview = preview_load(&artifact, &target, &store(), &git).unwrap();
        load_into_new_target(&artifact, &preview, &store(), &git).unwrap();
        assert_eq!(
            fs::read(target.join(".git/jjk/state.sqlite3")).unwrap(),
            b"sqlite-online-snapshot"
        );
    }
    #[test]
    fn preload_recovery_is_a_verified_distinct_backup() {
        let temp = tempdir().unwrap();
        let git = Git::default();
        let bundle = temp.path().join("source.bundle");
        fs::write(&bundle, b"bundle").unwrap();
        let output = temp.path().join("preload.jjkbak");
        let required = vec!["11".repeat(20)];
        let request = BackupCreateRequest {
            backup_id: "ignored",
            destination: &output,
            created_at_utc: "2026-08-28T00:00:00Z",
            created_by_version: "0.1.0",
            refs_json: b"{}",
            git_bundle: &bundle,
            git_object_format: "sha1",
            required_oids: &required,
            source_backup_id: Some("bak_source".into()),
        };
        let verified = create_preload_recovery(&store(), &git, &request, "op_test").unwrap();
        assert_eq!(verified.manifest.backup_id, "pre-load-op_test");
        assert_eq!(
            verified.manifest.source_backup_id.as_deref(),
            Some("bak_source")
        );
        assert_eq!(
            verify_backup_artifact(&output, &store(), &git).unwrap(),
            verified
        );
    }
}
