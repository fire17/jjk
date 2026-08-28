//! Checksummed backup/load and portable freeze services.
//!
//! Backup creation accepts a storage implementation only through
//! [`BackupStore`]; the canonical SQLite implementation uses SQLite's online
//! backup API. Loading is preview-first and defaults to a new, absent target.

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
    #[error("artifact I/O failed at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("invalid artifact: {0}")]
    Invalid(String),
    #[error("checksum mismatch for {0}")]
    Checksum(PathBuf),
    #[error("target already exists: {0}; load defaults to a new target")]
    ExistingTarget(PathBuf),
    #[error("missing required Git objects: {0:?}")]
    MissingObjects(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SchemaIdentity {
    pub format: String,
    pub major: u16,
    pub minor: u16,
    pub migration_set_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupBoundary {
    pub repository_id: String,
    pub schema: SchemaIdentity,
    pub journal_head: String,
    pub operation_boundary: String,
    pub git_object_format: String,
}

pub(crate) trait BackupStore {
    /// Creates a transactionally consistent database at `destination` through
    /// SQLite's online backup mechanism (never by copying a WAL-mode file).
    fn online_backup_to(&self, destination: &Path) -> Result<BackupBoundary, String>;
    fn verify_backup(&self, destination: &Path) -> Result<(), String>;
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
    pub journal_head: String,
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
    pub required_oids: &'a [String],
    pub source_backup_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BackupVerification {
    pub manifest: BackupManifestV1,
    pub total_bytes: u64,
}

pub(crate) fn create_backup(
    store: &dyn BackupStore,
    request: &BackupCreateRequest<'_>,
) -> Result<BackupVerification, BackupError> {
    timestamp(request.created_at_utc)?;
    ensure_new_destination(request.destination)?;
    let staging = sibling_staging(request.destination);
    remove_if_exists(&staging)?;
    private_dir(&staging)?;
    let result = (|| {
        let database_path = staging.join("metadata/store.db");
        private_parent(&database_path)?;
        let boundary = store.online_backup_to(&database_path).map_err(BackupError::Store)?;
        store.verify_backup(&database_path).map_err(BackupError::Store)?;

        let refs_path = staging.join("git/refs.json");
        private_parent(&refs_path)?;
        write_private(&refs_path, request.refs_json)?;
        let bundle_path = staging.join("git/objects.bundle");
        private_parent(&bundle_path)?;
        copy_private(request.git_bundle, &bundle_path)?;

        let mut artifacts = vec![
            digest_artifact(&staging, "metadata/store.db", "application/vnd.sqlite3", true)?,
            digest_artifact(&staging, "git/refs.json", "application/json", true)?,
            digest_artifact(&staging, "git/objects.bundle", "application/x-git-bundle", true)?,
        ];
        artifacts.sort_by(|left, right| left.path.cmp(&right.path));
        let refs_sha256 = sha256_hex(request.refs_json);
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
            git_object_format: boundary.git_object_format,
            privacy: "project-private+local-sensitive".into(),
            artifacts,
            required_oids: sorted_unique(request.required_oids),
            refs_sha256,
            restore_capabilities: vec!["new-target".into(), "current-with-preload-backup".into()],
            source_backup_id: request.source_backup_id.clone(),
        };
        write_manifest(&staging, &manifest)?;
        let verified = verify_backup_artifact(&staging)?;
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

pub(crate) fn verify_backup_artifact(root: &Path) -> Result<BackupVerification, BackupError> {
    let manifest_bytes = read(&root.join("manifest.json"))?;
    let expected = String::from_utf8(read(&root.join("manifest.sha256"))?)
        .map_err(|_| BackupError::Invalid("manifest.sha256 is not UTF-8".into()))?;
    if expected.trim() != sha256_hex(&manifest_bytes) {
        return Err(BackupError::Checksum(root.join("manifest.json")));
    }
    let manifest: BackupManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| BackupError::Invalid(format!("manifest JSON: {error}")))?;
    if manifest.format != "jjk-backup" || manifest.format_version != 1 {
        return Err(BackupError::Invalid("not a JJK backup v1".into()));
    }
    timestamp(&manifest.created_at_utc)?;
    let mut paths = BTreeSet::new();
    let mut total = manifest_bytes.len() as u64;
    for artifact in &manifest.artifacts {
        let relative = validated_artifact_path(&artifact.path)?;
        if !paths.insert(case_fold_path(&artifact.path)) {
            return Err(BackupError::Invalid(format!("duplicate artifact path `{}`", artifact.path)));
        }
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path).map_err(|source| BackupError::Io { path: path.clone(), source })?;
        if !metadata.file_type().is_file() {
            return Err(BackupError::Invalid(format!("artifact is not a regular file: {}", artifact.path)));
        }
        let bytes = read(&path)?;
        if bytes.len() as u64 != artifact.size_bytes || sha256_hex(&bytes) != artifact.sha256 {
            return Err(BackupError::Checksum(path));
        }
        total += bytes.len() as u64;
    }
    Ok(BackupVerification { manifest, total_bytes: total })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoadPreview {
    pub manifest: BackupManifestV1,
    pub target: PathBuf,
    pub target_exists: bool,
    pub required_bytes: u64,
    pub missing_oids: Vec<String>,
    pub mutation_allowed: bool,
}

pub(crate) trait GitBundleVerifier {
    fn verify_bundle(&self, bundle: &Path) -> Result<Vec<String>, String>;
}

pub(crate) fn preview_load(
    artifact: &Path,
    target: &Path,
    git: &dyn GitBundleVerifier,
) -> Result<LoadPreview, BackupError> {
    let verification = verify_backup_artifact(artifact)?;
    let advertised = git
        .verify_bundle(&artifact.join("git/objects.bundle"))
        .map_err(BackupError::Store)?;
    let advertised = advertised.into_iter().collect::<BTreeSet<_>>();
    let missing_oids = verification
        .manifest
        .required_oids
        .iter()
        .filter(|oid| !advertised.contains(*oid))
        .cloned()
        .collect::<Vec<_>>();
    let target_exists = target.exists();
    Ok(LoadPreview {
        manifest: verification.manifest,
        target: target.to_path_buf(),
        target_exists,
        required_bytes: verification.total_bytes,
        mutation_allowed: !target_exists && missing_oids.is_empty(),
        missing_oids,
    })
}

pub(crate) fn load_into_new_target(
    artifact: &Path,
    preview: &LoadPreview,
) -> Result<PathBuf, BackupError> {
    if preview.target.exists() {
        return Err(BackupError::ExistingTarget(preview.target.clone()));
    }
    if !preview.missing_oids.is_empty() {
        return Err(BackupError::MissingObjects(preview.missing_oids.clone()));
    }
    let staging = sibling_staging(&preview.target);
    remove_if_exists(&staging)?;
    private_dir(&staging)?;
    let result = (|| {
        let database = artifact.join("metadata/store.db");
        let destination = staging.join(".jjk/store.db");
        private_parent(&destination)?;
        copy_private(&database, &destination)?;
        let bundle = staging.join(".jjk/restore/objects.bundle");
        private_parent(&bundle)?;
        copy_private(&artifact.join("git/objects.bundle"), &bundle)?;
        let refs = staging.join(".jjk/restore/refs.json");
        private_parent(&refs)?;
        copy_private(&artifact.join("git/refs.json"), &refs)?;
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
        required_oids: request.required_oids,
        source_backup_id: request.source_backup_id.clone(),
    };
    create_backup(store, &preload)
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

pub(crate) fn create_freeze(request: &FreezeCreateRequest<'_>) -> Result<FreezeManifestV1, BackupError> {
    timestamp(request.created_at_utc)?;
    ensure_new_destination(request.destination)?;
    let staging = sibling_staging(request.destination);
    remove_if_exists(&staging)?;
    private_dir(&staging)?;
    let result = (|| {
        write_private(&staging.join("metadata/events.cbor"), request.metadata_events)?;
        let view = canonical_json_bytes(request.metadata_view)?;
        write_private(&staging.join("metadata/view.json"), &view)?;
        copy_private(request.git_bundle, &staging.join("git/objects.bundle"))?;
        let mut artifacts = vec![
            digest_artifact(&staging, "metadata/events.cbor", "application/cbor", true)?,
            digest_artifact(&staging, "metadata/view.json", "application/json", true)?,
            digest_artifact(&staging, "git/objects.bundle", "application/x-git-bundle", true)?,
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
            root_states: sorted_unique(request.root_states),
            attempts: sorted_unique(request.attempts),
            included_state_ids: sorted_unique(request.included_state_ids),
            included_edge_ids: sorted_unique(request.included_edge_ids),
            included_event_ids: sorted_unique(request.included_event_ids),
            boundary_parents: sorted_unique(request.boundary_parents),
            required_oids: sorted_unique(request.required_oids),
            offered_refs: request.root_states.iter().map(|state| format!("refs/jjk/imports/{}/{state}", request.freeze_id)).collect(),
            artifacts,
            privacy: "project-private; local-sensitive excluded".into(),
            required_capabilities: vec!["git-bundle-v2".into()],
        };
        write_manifest(&staging, &manifest)?;
        inspect_freeze(&staging)?;
        fs::rename(&staging, request.destination).map_err(|source| BackupError::Io {
            path: request.destination.to_path_buf(),
            source,
        })?;
        Ok(manifest)
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&staging); }
    result
}

pub(crate) fn inspect_freeze(root: &Path) -> Result<FreezeManifestV1, BackupError> {
    let manifest_bytes = read(&root.join("manifest.json"))?;
    let checksum = String::from_utf8(read(&root.join("manifest.sha256"))?)
        .map_err(|_| BackupError::Invalid("manifest checksum is not UTF-8".into()))?;
    if checksum.trim() != sha256_hex(&manifest_bytes) {
        return Err(BackupError::Checksum(root.join("manifest.json")));
    }
    let manifest: FreezeManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| BackupError::Invalid(format!("freeze manifest JSON: {error}")))?;
    if manifest.format != "jjk-freeze" || manifest.format_version != 1 {
        return Err(BackupError::Invalid("not a JJK freeze v1".into()));
    }
    for artifact in &manifest.artifacts {
        let relative = validated_artifact_path(&artifact.path)?;
        let path = root.join(relative);
        let bytes = read(&path)?;
        if bytes.len() as u64 != artifact.size_bytes || sha256_hex(&bytes) != artifact.sha256 {
            return Err(BackupError::Checksum(path));
        }
    }
    Ok(manifest)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FreezeImportPreview {
    pub manifest: FreezeManifestV1,
    pub missing_oids: Vec<String>,
    pub quarantine_refs: Vec<String>,
}

pub(crate) fn preview_freeze_import(
    root: &Path,
    git: &dyn GitBundleVerifier,
) -> Result<FreezeImportPreview, BackupError> {
    let manifest = inspect_freeze(root)?;
    let advertised = git.verify_bundle(&root.join("git/objects.bundle")).map_err(BackupError::Store)?
        .into_iter().collect::<BTreeSet<_>>();
    let missing_oids = manifest.required_oids.iter().filter(|oid| !advertised.contains(*oid)).cloned().collect();
    let quarantine_refs = manifest.offered_refs.clone();
    Ok(FreezeImportPreview { manifest, missing_oids, quarantine_refs })
}

fn write_manifest<T: Serialize>(root: &Path, manifest: &T) -> Result<(), BackupError> {
    let value = serde_json::to_value(manifest).map_err(|error| BackupError::Invalid(error.to_string()))?;
    let bytes = canonical_json_bytes(&value)?;
    write_private(&root.join("manifest.json"), &bytes)?;
    write_private(&root.join("manifest.sha256"), format!("{}\n", sha256_hex(&bytes)).as_bytes())
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, BackupError> {
    fn canonicalize(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                let mut canonical = serde_json::Map::new();
                for key in keys {
                    canonical.insert(key.clone(), canonicalize(&map[key]));
                }
                Value::Object(canonical)
            }
            Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_vec(&canonicalize(value)).map_err(|error| BackupError::Invalid(error.to_string()))
}

fn digest_artifact(root: &Path, relative: &str, media_type: &str, required: bool) -> Result<ArtifactDigest, BackupError> {
    validated_artifact_path(relative)?;
    let bytes = read(&root.join(relative))?;
    Ok(ArtifactDigest {
        path: relative.into(), media_type: media_type.into(), size_bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes), required,
    })
}

fn validated_artifact_path(path: &str) -> Result<&Path, BackupError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() || path.is_absolute() || !path.components().all(|component| matches!(component, Component::Normal(_))) {
        return Err(BackupError::Invalid(format!("unsafe artifact path `{}`", path.display())));
    }
    Ok(path)
}

fn sorted_unique(values: &[String]) -> Vec<String> {
    values.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect()
}

fn timestamp(value: &str) -> Result<(), BackupError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|error| BackupError::Invalid(format!("invalid RFC3339 timestamp: {error}")))
}

fn ensure_new_destination(path: &Path) -> Result<(), BackupError> {
    if path.exists() { return Err(BackupError::ExistingTarget(path.to_path_buf())); }
    if let Some(parent) = path.parent() { private_dir(parent)?; }
    Ok(())
}

fn sibling_staging(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("artifact");
    path.with_file_name(format!(".{name}.staging"))
}

fn private_parent(path: &Path) -> Result<(), BackupError> {
    if let Some(parent) = path.parent() { private_dir(parent)?; }
    Ok(())
}

fn private_dir(path: &Path) -> Result<(), BackupError> {
    fs::create_dir_all(path).map_err(|source| BackupError::Io { path: path.to_path_buf(), source })?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|source| BackupError::Io { path: path.to_path_buf(), source })?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    private_parent(path)?;
    fs::write(path, bytes).map_err(|source| BackupError::Io { path: path.to_path_buf(), source })?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|source| BackupError::Io { path: path.to_path_buf(), source })?;
    }
    Ok(())
}

fn copy_private(from: &Path, to: &Path) -> Result<(), BackupError> {
    let bytes = read(from)?;
    write_private(to, &bytes)
}

fn read(path: &Path) -> Result<Vec<u8>, BackupError> {
    fs::read(path).map_err(|source| BackupError::Io { path: path.to_path_buf(), source })
}

fn remove_if_exists(path: &Path) -> Result<(), BackupError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|source| BackupError::Io { path: path.to_path_buf(), source })?;
    }
    Ok(())
}

fn case_fold_path(path: &str) -> String { path.to_lowercase() }
fn sha256_hex(bytes: &[u8]) -> String { hex::encode(Sha256::digest(bytes)) }
