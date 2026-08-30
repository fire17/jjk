//! Executable runtime for the first complete native command slice.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::adapters::git::{GitCli, GitError};
use crate::adapters::os::{process::OsProcess, safe_path::SafeDestination};
use crate::adapters::sqlite::{
    RuntimeGitRef, RuntimeGitSnapshot, RuntimeNavigation, RuntimeProjection, RuntimeStateInsert,
    RuntimeStateRow, RuntimeWorktreeEntry, SqliteStore, StoreOpenOptions,
};
use crate::app::runtime_mutation::{RuntimeFact, RuntimeMutationCommit, RuntimeMutationRequest};
use crate::app::transaction::{CoordinationError, EffectFailure, PreparedOperation};
use crate::domain::{AttemptId, HandoffId, ProvenanceId, RepoId, StateId, ValidationId};
use crate::ports::journal::{ActorKind, EventRecord, Journal, PayloadCodec};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("{0}")]
    InvalidArguments(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Internal(String),
}

impl RuntimeError {
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidArguments(_) => 2,
            Self::Unavailable(_) => 3,
            Self::Internal(_) => 70,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Format {
    Human(usize),
    Json,
}

pub fn dispatch_native(name: &str, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    match name {
        "setup" => setup(args, cwd),
        "save" | "step" | "nice" => capture(name, args, cwd),
        "star" => star(true, args, cwd),
        "unstar" => star(false, args, cwd),
        "current" => current(args, cwd),
        "status" => status(args, cwd),
        "see" | "story" => list(name, args, cwd),
        "return" => restore(args, cwd),
        "back" | "forward" => history_navigation(name, args, cwd),
        "up" | "down" => traverse(name, args, cwd),
        "pick" => pick(args, cwd),
        "fork" => fork(args, cwd),
        "freeze" => freeze(args, cwd),
        "archive" => visibility(true, args, cwd),
        "recover" => recover(args, cwd),
        "undo" | "redo" => control_history(name, args, cwd),
        "backup" => backup(args, cwd),
        "load" => load(args, cwd),
        "handoff" => handoff(args, cwd),
        "validate" => validate(args, cwd),
        "doctor" => doctor(args, cwd),
        "completion" => completion(args),
        _ => Err(RuntimeError::Internal(format!(
            "registered command `{name}` has no runtime implementation"
        ))),
    }
}

fn setup(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let mut migration = None;
    let mut presentation_args = Vec::new();
    for argument in args {
        if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--migration="))
        {
            if migration.replace(value.to_owned()).is_some() {
                return Err(RuntimeError::InvalidArguments(
                    "setup accepts one --migration action".into(),
                ));
            }
        } else {
            presentation_args.push(argument.clone());
        }
    }
    let format = presentation(&presentation_args)?;
    let git = GitCli::new("git", OsProcess);
    let discovery = match git.discover(cwd) {
        Ok(discovery) => discovery,
        Err(_) => {
            let nonempty = fs::read_dir(cwd)
                .map_err(internal)?
                .next()
                .transpose()
                .map_err(internal)?
                .is_some();
            if nonempty {
                return Err(RuntimeError::Unavailable("setup initializes Git only in an empty directory; initialize non-empty directories explicitly".into()));
            }
            required(&git, cwd, ["init", "-q"])?;
            git.discover(cwd).map_err(git_error)?
        }
    };
    let root = discovery.worktree_root.as_ref().ok_or_else(|| {
        RuntimeError::Unavailable("`setup` requires a non-bare Git worktree".into())
    })?;
    let legacy = crate::app::command::init::detect_legacy_metadata(root);
    if legacy && migration.is_none() {
        return Err(RuntimeError::Unavailable("legacy JJK metadata detected; run `jjk setup --migration=check` or `--migration=apply`".into()));
    }
    let root_token = repository_root_token(&discovery.common_dir)?;
    if let Some(action) = migration.as_deref() {
        if !legacy && action != "rollback" {
            return Err(RuntimeError::Unavailable(
                "no legacy JJK metadata was detected".into(),
            ));
        }
        if action == "rollback" {
            return setup_legacy_rollback(format, root, &discovery.common_dir);
        }
        let lookup = crate::adapters::legacy::sqlite::GitLegacyLookup::new(&git, root);
        let prepared = crate::app::command::init::preview_legacy_migration(
            root,
            &discovery.common_dir.join("jjk"),
            &lookup,
        )
        .map_err(internal)?;
        if action == "check" {
            return emit_legacy_preview(format, root, &prepared, "check", None, false);
        }
        if action != "apply" {
            return Err(RuntimeError::InvalidArguments(
                "--migration must be check, apply, or rollback".into(),
            ));
        }
        let created_at = now_utc()?;
        let database = SqliteStore::database_path(&discovery.common_dir);
        let created = !database.exists();
        let mut store = if created {
            SqliteStore::open(
                &discovery.common_dir,
                RepoId::new_v7().as_uuid(),
                &root_token,
                &created_at,
                StoreOpenOptions::default(),
            )
        } else {
            SqliteStore::open_existing(
                &discovery.common_dir,
                &root_token,
                StoreOpenOptions::default(),
            )
        }
        .map_err(internal)?;
        let existing = crate::adapters::legacy::sqlite::existing_legacy_receipt(
            &store,
            &prepared.preview().migration_id,
        )
        .map_err(internal)?;
        if let crate::app::command::init::LegacyMigrationStatus::AlreadyApplied(receipt) =
            crate::app::command::init::inspect_legacy_migration_receipt(&prepared, existing)
                .map_err(internal)?
        {
            return emit_legacy_preview(
                format,
                root,
                &prepared,
                "apply",
                Some(receipt.migration_id),
                true,
            );
        }
        let imported =
            crate::app::command::init::prepare_legacy_import(&prepared).map_err(internal)?;
        let projection = crate::adapters::legacy::sqlite::legacy_import_projection(&imported)
            .map_err(internal)?;
        let common_dir = discovery.common_dir.clone();
        let request = RuntimeMutationRequest { operation_id:Uuid::now_v7(), repo_id:store.repository_uuid().map_err(internal)?, actor_id:Uuid::now_v7(), actor_kind:ActorKind::Import, command_kind:"setup-migration".into(), recorded_at_utc:now_utc()?, repository_fingerprint:repository_fingerprint(&git,cwd,store.repository_uuid().map_err(internal)?,&root_token)?, request:serde_json::to_vec(&serde_json::json!({"migration_id":prepared.preview().migration_id,"input_sha256":prepared.preview().input_sha256})).map_err(internal)?, expected_effects:serde_json::to_vec(&serde_json::json!({"rollback_capsule":prepared.preview().rollback_capsule,"projection":"legacy-migration"})).map_err(internal)?, recovery_artifact:Some(serde_json::to_vec(&serde_json::json!({"rollback_capsule":prepared.preview().rollback_capsule})).map_err(internal)?), provenance:b"legacy-import-v1".to_vec(), lock_timeout:std::time::Duration::from_secs(5) };
        let effect = |_proof: &PreparedOperation<'_>| {
            crate::app::command::init::preserve_legacy_sources(&prepared).map_err(|error| {
                EffectFailure::Indeterminate(RuntimeError::Internal(error.to_string()))
            })
        };
        let capsule = prepared.preview().rollback_capsule.clone();
        let verify =
            move |_manifest: &crate::adapters::legacy::repo_json::LegacyRollbackManifest| {
                Ok::<bool, RuntimeError>(capsule.is_dir())
            };
        let payload = serde_json::to_vec(&imported).map_err(internal)?;
        let migration_id = prepared.preview().migration_id.clone();
        let commit =
            move |_manifest: &crate::adapters::legacy::repo_json::LegacyRollbackManifest| {
                let mut fact = runtime_fact("LegacyMigrationImported", payload.clone());
                fact.dedup_key = Some(migration_id.clone());
                Ok::<RuntimeMutationCommit<RuntimeProjection>, RuntimeError>(
                    RuntimeMutationCommit {
                        facts: vec![fact],
                        projections: vec![projection],
                        result: payload,
                    },
                )
            };
        crate::app::runtime_mutation::execute(
            &common_dir,
            &mut store,
            request,
            effect,
            verify,
            commit,
        )
        .map_err(transaction_error)?;
        return emit_legacy_preview(
            format,
            root,
            &prepared,
            "apply",
            Some(prepared.preview().migration_id.clone()),
            false,
        );
    }
    let database = SqliteStore::database_path(&discovery.common_dir);
    let created = !database.exists();
    let created_at = now_utc()?;
    let mut store = if created {
        SqliteStore::open(
            &discovery.common_dir,
            RepoId::new_v7().as_uuid(),
            &root_token,
            &created_at,
            StoreOpenOptions::default(),
        )
    } else {
        SqliteStore::open_existing(
            &discovery.common_dir,
            &root_token,
            StoreOpenOptions::default(),
        )
    }
    .map_err(internal)?;
    import_reachable_history(&git, cwd, &discovery, &mut store)?;
    ensure_local_git_exclude(&discovery.common_dir, ".worktrees/")?;
    #[derive(Serialize)]
    struct SetupResult {
        command: &'static str,
        repository: String,
        store: String,
        repository_id: String,
        created: bool,
    }
    emit(
        format,
        &SetupResult {
            command: "setup",
            repository: root.display().to_string(),
            store: store.path().display().to_string(),
            repository_id: RepoId::from_uuid(store.repository_uuid().map_err(internal)?)
                .map_err(internal)?
                .to_string(),
            created,
        },
    )?;
    Ok(0)
}

fn import_reachable_history(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    discovery: &crate::adapters::git::RepositoryDiscovery,
    store: &mut SqliteStore,
) -> Result<(), RuntimeError> {
    let mut revisions = required(git, cwd, ["rev-list", "--topo-order", "--reverse", "--all"])?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if let Some(head) = observation_optional(git, cwd, ["rev-parse", "--verify", "HEAD"])? {
        if !revisions.iter().any(|oid| oid == &head) {
            let detached = required_os(
                git,
                cwd,
                [
                    OsString::from("rev-list"),
                    OsString::from("--topo-order"),
                    OsString::from("--reverse"),
                    OsString::from(&head),
                ],
            )?;
            for oid in detached.lines() {
                if !revisions.iter().any(|known| known == oid) {
                    revisions.push(oid.to_owned());
                }
            }
        }
    }
    let existing = store
        .state_rows()
        .map_err(internal)?
        .into_iter()
        .map(|state| state.git_oid)
        .collect::<std::collections::BTreeSet<_>>();
    revisions.retain(|oid| !existing.contains(oid));
    if revisions.is_empty() {
        return Ok(());
    }
    let algorithm = match &discovery.object_format {
        crate::ports::repository::ObjectFormat::Sha1 => "sha1".to_owned(),
        crate::ports::repository::ObjectFormat::Sha256 => "sha256".to_owned(),
        crate::ports::repository::ObjectFormat::Other(value) => {
            value.to_string_lossy().into_owned()
        }
    };
    let repo_id = store.repository_uuid().map_err(internal)?;
    let root_token = repository_root_token(&discovery.common_dir)?;
    let fingerprint = repository_fingerprint(git, cwd, repo_id, &root_token)?;
    let recorded = now_utc()?;
    let mut states = Vec::with_capacity(revisions.len());
    let mut events = Vec::with_capacity(revisions.len());
    let mut head = store.head().map_err(internal)?;
    for oid in revisions {
        let state_id =
            deterministic_v7_uuid(format!("jjk-import-state-v1\0{algorithm}\0{oid}").as_bytes());
        let attempt_id =
            deterministic_v7_uuid(format!("jjk-import-attempt-v1\0{algorithm}\0{oid}").as_bytes());
        let subject = required_os(
            git,
            cwd,
            [
                OsString::from("show"),
                OsString::from("-s"),
                OsString::from("--format=%s"),
                OsString::from(&oid),
            ],
        )?;
        let label = format!("import-{}", &oid[..oid.len().min(12)]);
        let state = RuntimeStateInsert {
            state_id,
            attempt_id,
            logical_parent: None,
            workspace_id: Uuid::nil(),
            git_algorithm: algorithm.clone(),
            git_oid: oid.clone(),
            head_oid: None,
            kind: "imported".into(),
            label,
            message: Some(subject),
            relative_locator: Vec::new(),
        };
        let mut event=runtime_event_for_head(store,&head,"GitCommitImported",serde_json::to_vec(&serde_json::json!({"state_id":display_state_id(&hex::encode_upper(state_id.as_bytes()))?,"git_oid":oid})).map_err(internal)?,ActorKind::Import,&recorded,&fingerprint)?;
        event.dedup_key = Some(format!("git-commit:{algorithm}:{}", state.git_oid));
        head.event_hash = event.event_hash;
        events.push(event);
        states.push(state);
    }
    store
        .import_runtime_history(&events, &states)
        .map_err(internal)?;
    Ok(())
}
fn emit_legacy_preview(
    format: Format,
    root: &Path,
    prepared: &crate::app::command::init::PreparedLegacyMigration,
    action: &str,
    migration_id: Option<String>,
    already_imported: bool,
) -> Result<i32, RuntimeError> {
    let preview = prepared.preview();
    let rollback_capsule =
        (action != "check").then(|| preview.rollback_capsule.display().to_string());
    let value = serde_json::json!({"command":"setup","repository":root.display().to_string(),"migration":{"action":action,"migration_id":migration_id,"source_id":preview.source_id,"input_sha256":preview.input_sha256,"entities":preview.entity_counts,"quarantined":preview.quarantined,"warnings":preview.warnings,"rollback_capsule":rollback_capsule,"already_imported":already_imported}});
    emit(format, &value)?;
    Ok(0)
}

fn setup_legacy_rollback(
    format: Format,
    root: &Path,
    common_dir: &Path,
) -> Result<i32, RuntimeError> {
    let migration_root = common_dir.join("jjk").join("migrations").join("legacy-v1");
    let mut capsules = fs::read_dir(&migration_root)
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    capsules.sort_by_key(std::fs::DirEntry::file_name);
    let capsule = capsules
        .last()
        .ok_or_else(|| RuntimeError::Unavailable("no legacy rollback capsule exists".into()))?
        .path();
    let outcome = crate::app::command::init::recover_legacy_assets(&capsule, &root.join(".jjk"))
        .map_err(internal)?;
    let migration_id = capsule
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| RuntimeError::Internal("invalid rollback capsule name".into()))?;
    emit(
        format,
        &serde_json::json!({"command":"setup","repository":root.display().to_string(),"migration":{"action":"rollback","migration_id":migration_id,"rollback_capsule":capsule.display().to_string(),"already_recovered":outcome.already_recovered,"files_recovered":outcome.files_recovered,"bytes_recovered":outcome.bytes_recovered}}),
    )?;
    Ok(0)
}

fn capture(name: &str, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let (format, message) = capture_arguments(args)?;
    let mut context = context(cwd)?;
    let head_commit = observation_optional(&context.git, cwd, ["rev-parse", "--verify", "HEAD"])?;
    let parent_state = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?;
    if let Some(parent) = &parent_state {
        let parent_tree = observation_required_os(
            &context.git,
            cwd,
            [
                OsString::from("rev-parse"),
                OsString::from(format!("{}^{{tree}}", parent.git_oid)),
            ],
        )?;
        let live_tree = observation_required(&context.git, cwd, ["write-tree"])?;
        let unstaged = context
            .git
            .run(
                cwd,
                observation_args([OsString::from("diff"), OsString::from("--quiet")]),
            )
            .map_err(git_error)?;
        let untracked = observation_required(
            &context.git,
            cwd,
            ["ls-files", "--others", "--exclude-standard"],
        )?;
        if parent_tree == live_tree && unstaged.exit_code == 0 && untracked.is_empty() {
            #[derive(Serialize)]
            struct NoopResult {
                command: String,
                state_id: String,
                state_ref: String,
                commit: String,
                attempt_id: String,
                label: String,
                created: bool,
            }
            let state_id = display_state_id(&parent.state_id)?;
            emit(
                format,
                &NoopResult {
                    command: name.to_owned(),
                    state_ref: format!("refs/jjk/states/{state_id}"),
                    state_id,
                    commit: parent.git_oid.clone(),
                    attempt_id: display_attempt_id(&parent.attempt_id)?,
                    label: parent.label.clone(),
                    created: false,
                },
            )?;
            return Ok(0);
        }
    }
    let divergent = parent_state
        .as_ref()
        .map(|parent| {
            context
                .store
                .logical_children(parent)
                .map(|children| !children.is_empty())
        })
        .transpose()
        .map_err(internal)?
        .unwrap_or(false);
    let attempt_id = if divergent {
        AttemptId::new_v7().as_uuid()
    } else {
        context
            .store
            .current_attempt_id(context.workspace_id)
            .map_err(internal)?
            .or_else(|| {
                parent_state
                    .as_ref()
                    .and_then(|parent| parse_attempt_id(&parent.attempt_id).ok())
            })
            .unwrap_or_else(|| AttemptId::new_v7().as_uuid())
    };
    let state_id = StateId::new_v7();
    let label = unique_state_label(&context.store, name, &message)?;
    let mut navigation = context
        .store
        .runtime_navigation(context.workspace_id)
        .map_err(internal)?;
    if let Some(parent) = &parent_state {
        append_navigation(
            &mut navigation,
            &parent.state_id,
            &hex::encode_upper(state_id.into_bytes()),
        );
    } else {
        navigation.entries = vec![hex::encode_upper(state_id.into_bytes())];
        navigation.cursor = Some(0);
    }
    let logical_parent = parent_state
        .as_ref()
        .map(|parent| parse_state_id(&parent.state_id))
        .transpose()?;
    let semantic_parent = parent_state
        .as_ref()
        .map(|state| state.git_oid.clone())
        .or_else(|| head_commit.clone());
    let state_ref = format!("refs/jjk/states/{state_id}");
    let (request, common_dir) = mutation_request(
        &context,
        cwd,
        name,
        serde_json::json!({"message":message,"workspace_id":context.workspace_id,"parent_state":parent_state.as_ref().map(|state|state.state_id.clone())}),
        serde_json::json!({"private_index":true,"commit_parent":semantic_parent,"state_ref":state_ref}),
        Some(
            serde_json::json!({"state_ref":state_ref,"expected_pre":null,"post_commit":"computed-by-effect"}),
        ),
    )?;
    let private_index = context
        .store
        .path()
        .parent()
        .expect("store parent")
        .join(format!("capture-{}.index", request.operation_id.simple()));
    let workspace_id = context.workspace_id;
    let relative_locator = context.relative_locator.clone();
    let object_format = context.object_format.clone();
    let git = &context.git;
    let before_git = capture_runtime_git_snapshot(git, cwd)?;
    let store = &mut context.store;
    let message_for_effect = message.clone();
    let state_ref_for_effect = state_ref.clone();
    let semantic_parent_for_effect = semantic_parent.clone();
    let head_for_effect = head_commit.clone();
    let effect = move |_prepared: &PreparedOperation<'_>| {
        (|| -> Result<(String, String, RuntimeGitSnapshot), RuntimeError> {
            let mut env = BTreeMap::new();
            env.insert(
                OsString::from("GIT_INDEX_FILE"),
                Some(private_index.as_os_str().to_owned()),
            );
            if let Some(head) = &head_for_effect {
                required_output(
                    git.run_with_env(cwd, ["read-tree", head], env.clone())
                        .map_err(git_error)?,
                )?;
            }
            add_all_excluding_nested(git, cwd, env.clone())?;
            let tree = required_output(
                git.run_with_env(cwd, ["write-tree"], env)
                    .map_err(git_error)?,
            )?;
            let _ = fs::remove_file(&private_index);
            let commit = create_commit(
                git,
                cwd,
                &tree,
                semantic_parent_for_effect.as_deref(),
                &message_for_effect,
            )?;
            required_os(
                git,
                cwd,
                [
                    OsString::from("update-ref"),
                    OsString::from(&state_ref_for_effect),
                    OsString::from(&commit),
                    OsString::from("0000000000000000000000000000000000000000"),
                ],
            )?;
            let after = capture_runtime_git_snapshot(git, cwd)?;
            Ok((tree, commit, after))
        })()
        .map_err(EffectFailure::Indeterminate)
    };
    let state_ref_for_verify = state_ref.clone();
    let verify =
        move |effect: &(String, String, RuntimeGitSnapshot)| -> Result<bool, RuntimeError> {
            let ref_oid = observation_required_os(
                git,
                cwd,
                [
                    OsString::from("rev-parse"),
                    OsString::from(&state_ref_for_verify),
                ],
            )?;
            let commit_tree = observation_required_os(
                git,
                cwd,
                [
                    OsString::from("rev-parse"),
                    OsString::from(format!("{}^{{tree}}", effect.1)),
                ],
            )?;
            Ok(ref_oid == effect.1
                && commit_tree == effect.0
                && capture_runtime_git_snapshot(git, cwd)? == effect.2)
        };
    let payload_message = message.clone();
    let label_for_commit = label.clone();
    let head_for_commit = head_commit.clone();
    let result_state_ref = state_ref.clone();
    let parent_for_attempt = parent_state.clone();
    let attempt_objective = message.clone();
    let commit = move |effect: &(String, String, RuntimeGitSnapshot)| -> Result<RuntimeMutationCommit<RuntimeProjection>, RuntimeError> {
        let payload = serde_json::to_vec(&serde_json::json!({"state_id":state_id.to_string(),"attempt_id":AttemptId::from_uuid(attempt_id).map_err(internal)?.to_string(),"git_oid":effect.1,"kind":name,"label":label_for_commit,"message":payload_message})).map_err(internal)?;
        let result = serde_json::to_vec(&serde_json::json!({"state_id":state_id.to_string(),"state_ref":result_state_ref,"commit":effect.1})).map_err(internal)?;
        let state = RuntimeProjection::State { state: RuntimeStateInsert {
            state_id: state_id.as_uuid(), attempt_id, logical_parent, workspace_id,
            git_algorithm: object_format, git_oid: effect.1.clone(), head_oid: head_for_commit,
            kind: name.to_owned(), label: label_for_commit, message: Some(payload_message), relative_locator,
        }};
        let mut projections = Vec::new();
        if divergent { projections.push(RuntimeProjection::Fork { source: parent_for_attempt.clone().expect("divergent capture has parent"), attempt_id, objective: attempt_objective.clone(), workspace_id: None, relative_locator: None, head_oid: None }); }
        projections.push(state);
        projections.push(RuntimeProjection::Raw(crate::ports::projection::ProjectionUpdate { projection_name: "runtime-navigation-v1".to_owned(), reducer_version: 1, key: workspace_id.as_bytes().to_vec(), value: serde_json::to_vec(&navigation).expect("runtime navigation serializes"), event_index: 0 }));
        projections.push(RuntimeProjection::ControlGit { before: before_git.clone(), after: effect.2.clone() });
        Ok(RuntimeMutationCommit { facts: vec![runtime_fact("StateCaptured", payload)], projections, result })
    };
    crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
        .map_err(transaction_error)?;
    let committed = observation_required_os(
        git,
        cwd,
        [OsString::from("rev-parse"), OsString::from(&state_ref)],
    )?;
    #[derive(Serialize)]
    struct CaptureResult<'a> {
        command: &'a str,
        state_id: String,
        state_ref: String,
        commit: String,
        attempt_id: String,
        workspace_id: String,
        logical_parent: Option<String>,
        label: String,
    }
    emit(
        format,
        &CaptureResult {
            command: name,
            state_id: state_id.to_string(),
            state_ref,
            commit: committed,
            attempt_id: AttemptId::from_uuid(attempt_id)
                .map_err(internal)?
                .to_string(),
            workspace_id: crate::domain::WorkspaceId::from_uuid(workspace_id)
                .map_err(internal)?
                .to_string(),
            logical_parent: logical_parent
                .map(|parent| {
                    StateId::from_uuid(parent)
                        .map(|id| id.to_string())
                        .map_err(internal)
                })
                .transpose()?,
            label,
        },
    )?;
    Ok(0)
}
fn current(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let context = context(cwd)?;
    let state = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .ok_or_else(|| RuntimeError::Unavailable("no JJK state exists".into()))?;
    let projection_version = state.created_seq.saturating_add(1);
    let starred = context
        .store
        .state_is_starred(&state.state_id)
        .map_err(internal)?;
    let mut state = state_view("current", state, starred)?;
    if let Some(attempt_id) = context
        .store
        .current_attempt_id(context.workspace_id)
        .map_err(internal)?
    {
        state.attempt_id = AttemptId::from_uuid(attempt_id)
            .map_err(internal)?
            .to_string();
    }
    #[derive(Serialize)]
    struct Result {
        projection_version: u64,
        workspace_id: String,
        #[serde(flatten)]
        state: StateView,
    }
    emit(
        format,
        &Result {
            projection_version,
            workspace_id: crate::domain::WorkspaceId::from_uuid(context.workspace_id)
                .map_err(internal)?
                .to_string(),
            state,
        },
    )?;
    Ok(0)
}

fn list(name: &str, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let context = context(cwd)?;
    let current_state = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .map(|state| display_state_id(&state.state_id))
        .transpose()?;
    let states = context
        .store
        .state_rows()
        .map_err(internal)?
        .into_iter()
        .rev()
        .filter(|row| !row.archived)
        .filter_map(|row| {
            let starred = context
                .store
                .state_is_starred(&row.state_id)
                .map_err(internal);
            match starred {
                Ok(starred) if name != "story" || row.kind == "nice" || starred => {
                    Some(state_view(name, row, starred))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let projection_version = context.store.head().map_err(internal)?.local_seq;
    #[derive(Serialize)]
    struct Result<'a> {
        command: &'a str,
        projection_version: u64,
        current_state: Option<String>,
        states: Vec<StateView>,
    }
    emit(
        format,
        &Result {
            command: name,
            projection_version,
            current_state,
            states,
        },
    )?;
    Ok(0)
}
fn star(enabled: bool, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let command = if enabled { "star" } else { "unstar" };
    let (format, target) = optional_state_argument(args, command)?;
    let mut context = context(cwd)?;
    let state = match target {
        Some(target) => context
            .store
            .resolve_state_row(&target)
            .map_err(unavailable_store)?,
        None => context
            .store
            .current_state_row(context.workspace_id)
            .map_err(unavailable_store)?
            .ok_or_else(|| {
                RuntimeError::Unavailable(format!(
                    "{command} requires a current JJK state or explicit state"
                ))
            })?,
    };
    if state.archived {
        return Err(RuntimeError::Unavailable(format!(
            "cannot {command} archived state {}; recover it first",
            display_state_id(&state.state_id)?
        )));
    }
    let state_id = display_state_id(&state.state_id)?;
    let changed = context
        .store
        .state_is_starred(&state.state_id)
        .map_err(internal)?
        != enabled;
    if changed {
        let (request, common_dir) = mutation_request(
            &context,
            cwd,
            command,
            serde_json::json!({"state_id":state_id,"starred":enabled}),
            serde_json::json!({"projection":"state-annotation","kind":"star","enabled":enabled}),
            None,
        )?;
        let store = &mut context.store;
        let effect = |_prepared: &PreparedOperation<'_>| Ok::<(), EffectFailure<RuntimeError>>(());
        let verify = |_effect: &()| Ok::<bool, RuntimeError>(true);
        let projection_state = state.clone();
        let state_for_result = state_id.clone();
        let commit =
            move |_effect: &()| -> Result<RuntimeMutationCommit<RuntimeProjection>, RuntimeError> {
                let payload = serde_json::to_vec(&serde_json::json!({"state_id":state_for_result,"kind":"star","enabled":enabled})).map_err(internal)?;
                Ok(fact_commit(
                "StateAnnotated",
                payload,
                RuntimeProjection::Star { state: projection_state, enabled },
                serde_json::to_vec(&serde_json::json!({"state_id":state_for_result,"starred":enabled,"changed":true})).map_err(internal)?,
            ))
            };
        crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
            .map_err(transaction_error)?;
    }
    #[derive(Serialize)]
    struct StarResult {
        command: &'static str,
        state_id: String,
        starred: bool,
        changed: bool,
    }
    emit(
        format,
        &StarResult {
            command,
            state_id,
            starred: enabled,
            changed,
        },
    )?;
    Ok(0)
}

fn restore(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let (format, target) = state_argument(args, "return")?;
    let mut context = context(cwd)?;
    let current = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?;
    let captured = captured_snapshot(&context, current.as_ref())?;
    if !workspace_matches_state(&context.git, cwd, current.as_ref(), captured.as_ref())? {
        return Err(RuntimeError::Unavailable(
            "return unavailable because the workspace or index differs from the current JJK state"
                .into(),
        ));
    }
    let state = context
        .store
        .resolve_state_row(&target)
        .map_err(unavailable_store)?;
    let mut saved_git = context
        .store
        .runtime_git_snapshot_for_state(context.workspace_id, &state.state_id)
        .map_err(internal)?;
    if let Some(snapshot) = &mut saved_git {
        let retained = capture_git_refs(&context.git, cwd)?;
        for reference in retained
            .into_iter()
            .filter(|reference| reference.name.starts_with(b"refs/jjk/states/"))
        {
            if !snapshot
                .refs
                .iter()
                .any(|saved| saved.name == reference.name)
            {
                snapshot.refs.push(reference);
            }
        }
        snapshot
            .refs
            .sort_by(|left, right| left.name.cmp(&right.name));
    }
    execute_activation("return", &mut context, cwd, &state, None, saved_git)?;
    #[derive(Serialize)]
    struct RestoreResult {
        command: &'static str,
        state_id: String,
        commit: String,
    }
    emit(
        format,
        &RestoreResult {
            command: "return",
            state_id: display_state_id(&state.state_id)?,
            commit: state.git_oid,
        },
    )?;
    Ok(0)
}

fn traverse(name: &str, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let mut context = context(cwd)?;
    let current = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .ok_or_else(|| RuntimeError::Unavailable("no current JJK state exists".into()))?;
    let captured = captured_snapshot(&context, Some(&current))?;
    if !workspace_matches_state(&context.git, cwd, Some(&current), captured.as_ref())? {
        return Err(RuntimeError::Unavailable(format!(
            "refusing {name} because the worktree or index differs from the current JJK state"
        )));
    }
    let target = if name == "up" {
        let parent = current.logical_parent.as_deref().ok_or_else(|| {
            RuntimeError::Unavailable("current state has no logical parent".into())
        })?;
        context
            .store
            .resolve_state_row(parent)
            .map_err(unavailable_store)?
    } else {
        let children = context.store.logical_children(&current).map_err(internal)?;
        match children.as_slice() {
            [child] => child.clone(),
            [] => {
                return Err(RuntimeError::Unavailable(
                    "current state has no logical child".into(),
                ));
            }
            _ => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "ambiguous navigation: current state has {} logical children; use `jjk return <state>` with one of {}",
                    children.len(),
                    children
                        .iter()
                        .map(|child| display_state_id(&child.state_id)
                            .unwrap_or_else(|_| child.state_id.clone()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    };
    activate_state(name, format, &mut context, cwd, target)
}

fn history_navigation(name: &str, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let mut context = context(cwd)?;
    let current = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .ok_or_else(|| RuntimeError::Unavailable("no current JJK state exists".into()))?;
    let captured = captured_snapshot(&context, Some(&current))?;
    if !workspace_matches_state(&context.git, cwd, Some(&current), captured.as_ref())? {
        return Err(RuntimeError::Unavailable(format!(
            "refusing {name} because the worktree or index differs from the current JJK state"
        )));
    }
    let before = context
        .store
        .runtime_navigation(context.workspace_id)
        .map_err(internal)?;
    let from = before.cursor.ok_or_else(|| {
        RuntimeError::Unavailable("navigation history has no current position".into())
    })?;
    let to = if name == "back" {
        from.checked_sub(1)
    } else {
        from.checked_add(1)
            .filter(|index| *index < before.entries.len())
    }
    .ok_or_else(|| {
        RuntimeError::Unavailable(
            if name == "back" {
                "no earlier navigation state"
            } else {
                "no later navigation state"
            }
            .into(),
        )
    })?;
    let target = context
        .store
        .resolve_state_row(&before.entries[to])
        .map_err(unavailable_store)?;
    let mut saved_git = context
        .store
        .runtime_git_snapshot_for_state(context.workspace_id, &target.state_id)
        .map_err(internal)?;
    if let Some(snapshot) = &mut saved_git {
        let retained = capture_git_refs(&context.git, cwd)?;
        for reference in retained
            .into_iter()
            .filter(|reference| reference.name.starts_with(b"refs/jjk/states/"))
        {
            if !snapshot
                .refs
                .iter()
                .any(|saved| saved.name == reference.name)
            {
                snapshot.refs.push(reference);
            }
        }
        snapshot
            .refs
            .sort_by(|left, right| left.name.cmp(&right.name));
    }
    let mut navigation = before.clone();
    navigation.cursor = Some(to);
    execute_activation(
        name,
        &mut context,
        cwd,
        &target,
        Some(navigation),
        saved_git,
    )?;
    #[derive(Serialize)]
    struct NavigationResult<'a> {
        command: &'a str,
        state_id: String,
        commit: String,
        history_position: usize,
        history_length: usize,
    }
    emit(
        format,
        &NavigationResult {
            command: name,
            state_id: display_state_id(&target.state_id)?,
            commit: target.git_oid,
            history_position: to + 1,
            history_length: before.entries.len(),
        },
    )?;
    Ok(0)
}
fn pick(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let (format, query) = state_argument(args, "pick")?;
    let mut context = context(cwd)?;
    let target = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .ok_or_else(|| RuntimeError::Unavailable("pick requires a current target state".into()))?;
    let source = context
        .store
        .resolve_state_row(&query)
        .map_err(unavailable_store)?;
    let parent =
        context
            .store
            .resolve_state_row(source.logical_parent.as_deref().ok_or_else(|| {
                RuntimeError::Unavailable("pick source has no logical parent".into())
            })?)
            .map_err(unavailable_store)?;
    let diff = context
        .git
        .run(
            cwd,
            [
                "diff",
                "--binary",
                "--full-index",
                "--no-ext-diff",
                &parent.git_oid,
                &source.git_oid,
                "--",
            ],
        )
        .map_err(git_error)?;
    if diff.exit_code != 0 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&diff.stderr).into(),
        ));
    }
    let patch_id = stable_patch_id(cwd, &diff.stdout)?;
    let before = capture_runtime_git_snapshot(&context.git, cwd)?;
    let preimage_json = git_control_receipt_preimage(&context.git, cwd, &before)?;
    let preimage_bytes = serde_json::to_vec(&before).map_err(internal)?;
    let result_id = StateId::new_v7();
    let provenance_id = ProvenanceId::new_v7();
    let composition_id = crate::domain::CompositionId::new_v7();
    let message = format!("pick {} onto {}", source.label, target.label);
    let label = unique_state_label(&context.store, "cherry", &message)?;
    let source_state = display_state_id(&source.state_id)?;
    let source_parent = display_state_id(&parent.state_id)?;
    let target_base = display_state_id(&target.state_id)?;
    let (request, common_dir) = mutation_request(
        &context,
        cwd,
        "pick",
        serde_json::json!({"source_state":source_state,"source_parent":source_parent,"target_base":target_base,"patch_id":patch_id,"composition_id":composition_id.to_string()}),
        serde_json::json!({"isolated_index":true,"source_delta":patch_id,"result_ref":format!("refs/jjk/states/{result_id}")}),
        Some(serde_json::from_slice(&preimage_bytes).map_err(internal)?),
    )?;
    let operation_id = request.operation_id;
    let artifact_id = format!("conflict-{}", operation_id.simple());
    let recovery_dir = common_dir.join("jjk/recovery");
    let conflict_dir = common_dir.join("jjk/conflicts");
    let preimage_artifact = recovery_dir.join(format!("{}.preimage.json", operation_id.simple()));
    let conflict_artifact = conflict_dir.join(format!("{artifact_id}.json"));
    let patch_artifact = conflict_dir.join(format!("{artifact_id}.patch"));
    let temporary_index = common_dir
        .join("jjk/tmp")
        .join(format!("pick-{}.index", operation_id.simple()));
    let artifact_sha256 = hex::encode(Sha256::digest(&preimage_bytes));
    let next_action = format!("jjk recover {operation_id} --abort");
    let git = &context.git;
    let target_oid = target.git_oid.clone();
    let target_parent = target.git_oid.clone();
    let result_ref = format!("refs/jjk/states/{result_id}");
    let result_ref_for_effect = result_ref.clone();
    let message_for_effect = message.clone();
    let diff_for_effect = diff.stdout;
    let receipt_template = PickConflictReceipt {
        command: "pick".into(),
        operation_id: operation_id.to_string(),
        status: "awaiting_resolution".into(),
        source_state: source_state.clone(),
        source_parent: source_parent.clone(),
        target_base: target_base.clone(),
        patch_id: patch_id.clone(),
        composition_id: composition_id.to_string(),
        artifact_id,
        conflict_artifact: conflict_artifact.display().to_string(),
        artifact_sha256,
        preimage_artifact: preimage_artifact.display().to_string(),
        next_action,
        conflicting_paths: Vec::new(),
        preimage: preimage_json,
    };
    let before_for_effect = before.clone();
    let effect = move |_prepared: &PreparedOperation<'_>| -> Result<PickEffect, EffectFailure<RuntimeError>> {
        (|| -> Result<PickEffect, EffectFailure<RuntimeError>> {
            fs::create_dir_all(&recovery_dir).map_err(|error| EffectFailure::Indeterminate(internal(error)))?;
            fs::create_dir_all(&conflict_dir).map_err(|error| EffectFailure::Indeterminate(internal(error)))?;
            if let Some(parent) = temporary_index.parent() { fs::create_dir_all(parent).map_err(|error| EffectFailure::Indeterminate(internal(error)))?; }
            atomic_bytes(&preimage_artifact, &preimage_bytes).map_err(EffectFailure::Indeterminate)?;
            atomic_bytes(&patch_artifact, &diff_for_effect).map_err(EffectFailure::Indeterminate)?;
            let mut env = BTreeMap::new();
            env.insert(OsString::from("GIT_INDEX_FILE"), Some(temporary_index.as_os_str().to_owned()));
            required_output(git.run_with_env(cwd, ["read-tree", target_oid.as_str()], env.clone()).map_err(git_error).map_err(EffectFailure::Indeterminate)?)
                .map_err(EffectFailure::Indeterminate)?;
            let isolated = git.run_with_env(cwd, [OsString::from("apply"), OsString::from("--3way"), OsString::from("--cached"), patch_artifact.as_os_str().to_owned()], env.clone())
                .map_err(git_error).map_err(EffectFailure::Indeterminate)?;
            if isolated.exit_code != 0 {
                let paths = conflict_paths(git, cwd, env).unwrap_or_default();
                let _ = fs::remove_file(&temporary_index);
                let mut receipt = receipt_template.clone(); receipt.conflicting_paths = paths;
                let receipt_bytes = serde_json::to_vec(&receipt).map_err(internal).map_err(EffectFailure::Indeterminate)?;
                atomic_bytes(&conflict_artifact, &receipt_bytes).map_err(EffectFailure::Indeterminate)?;
                return Err(EffectFailure::ConflictPaused { source: RuntimeError::Unavailable("pick conflict awaits explicit resolution".into()), result: receipt_bytes });
            }
            let tree = required_output(git.run_with_env(cwd, ["write-tree"], env).map_err(git_error).map_err(EffectFailure::Indeterminate)?)
                .map_err(EffectFailure::Indeterminate)?;
            let live = git.run(cwd, [OsString::from("apply"), OsString::from("--3way"), OsString::from("--index"), patch_artifact.as_os_str().to_owned()])
                .map_err(git_error).map_err(EffectFailure::Indeterminate)?;
            if live.exit_code != 0 {
                restore_runtime_git_snapshot(git, cwd, &before_for_effect, None).map_err(EffectFailure::Indeterminate)?;
                let mut receipt = receipt_template.clone(); receipt.conflicting_paths = conflict_paths(git, cwd, BTreeMap::new()).unwrap_or_default();
                let receipt_bytes = serde_json::to_vec(&receipt).map_err(internal).map_err(EffectFailure::Indeterminate)?;
                atomic_bytes(&conflict_artifact, &receipt_bytes).map_err(EffectFailure::Indeterminate)?;
                let _ = fs::remove_file(&temporary_index);
                return Err(EffectFailure::ConflictPaused { source: RuntimeError::Unavailable("pick conflict awaits explicit resolution".into()), result: receipt_bytes });
            }
            let commit = create_commit(git, cwd, &tree, Some(&target_parent), &message_for_effect).map_err(EffectFailure::Indeterminate)?;
            required_os(git, cwd, [OsString::from("update-ref"), OsString::from(&result_ref_for_effect), OsString::from(&commit), OsString::from("0000000000000000000000000000000000000000")]).map_err(EffectFailure::Indeterminate)?;
            let after = capture_runtime_git_snapshot(git, cwd).map_err(EffectFailure::Indeterminate)?;
            let _ = fs::remove_file(&temporary_index);
            Ok(PickEffect { tree, commit, after })
        })()
    };
    let result_ref_for_verify = result_ref.clone();
    let verify = move |effect: &PickEffect| -> Result<bool, RuntimeError> {
        let ref_oid = observation_required_os(
            git,
            cwd,
            [
                OsString::from("rev-parse"),
                OsString::from(&result_ref_for_verify),
            ],
        )?;
        let commit_tree = observation_required_os(
            git,
            cwd,
            [
                OsString::from("rev-parse"),
                OsString::from(format!("{}^{{tree}}", effect.commit)),
            ],
        )?;
        Ok(ref_oid == effect.commit && commit_tree == effect.tree)
    };
    let mut navigation = context
        .store
        .runtime_navigation(context.workspace_id)
        .map_err(internal)?;
    append_navigation(
        &mut navigation,
        &target.state_id,
        &hex::encode_upper(result_id.into_bytes()),
    );
    let state = RuntimeStateInsert {
        state_id: result_id.as_uuid(),
        attempt_id: parse_attempt_id(&target.attempt_id)?,
        logical_parent: Some(parse_state_id(&target.state_id)?),
        workspace_id: context.workspace_id,
        git_algorithm: context.object_format.clone(),
        git_oid: String::new(),
        head_oid: optional(&context.git, cwd, ["rev-parse", "--verify", "HEAD"])?,
        kind: "cherry".into(),
        label,
        message: Some(message),
        relative_locator: context.relative_locator.clone(),
    };
    let source_uuid = parse_state_id(&source.state_id)?;
    let before_for_commit = before;
    let attempt_id = display_attempt_id(&target.attempt_id)?;
    let commit = move |effect: &PickEffect| -> Result<RuntimeMutationCommit<RuntimeProjection>, RuntimeError> {
        let mut state = state.clone(); state.git_oid = effect.commit.clone();
        let result = serde_json::to_vec(&serde_json::json!({"command":"pick","state_id":result_id.to_string(),"commit":effect.commit,"kind":"cherry","source_state":source_state,"source_parent":source_parent,"target_base":target_base,"attempt_id":attempt_id,"patch_id":patch_id,"provenance_id":provenance_id.to_string(),"composition_id":composition_id.to_string(),"conflicted":false,"conflict_resolution":null})).map_err(internal)?;
        let payload = serde_json::to_vec(&serde_json::json!({"source_state":source_state,"source_parent":source_parent,"target_base":target_base,"patch_id":patch_id,"result_state":result_id.to_string()})).map_err(internal)?;
        Ok(RuntimeMutationCommit { facts: vec![runtime_fact("DeltaApplied", payload)], projections: vec![RuntimeProjection::PickedState { state, source_state: source_uuid, provenance_id: provenance_id.as_uuid(), navigation: navigation.clone() }, RuntimeProjection::ControlGit { before: before_for_commit.clone(), after: effect.after.clone() }], result })
    };
    let store = &mut context.store;
    match crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
    {
        Ok(operation) => {
            let result = serde_json::from_slice::<serde_json::Value>(
                operation
                    .result
                    .as_deref()
                    .ok_or_else(|| RuntimeError::Internal("committed pick has no result".into()))?,
            )
            .map_err(internal)?;
            emit(format, &result)?;
            Ok(0)
        }
        Err(CoordinationError::ConflictPaused { operation, .. }) => {
            let receipt = conflict_receipt(&operation)?;
            emit(format, &receipt)?;
            Ok(crate::cli::exit::ExitCode::Conflict.get())
        }
        Err(error) => Err(transaction_error(error)),
    }
}

#[derive(Clone, Debug)]
struct PickEffect {
    tree: String,
    commit: String,
    after: RuntimeGitSnapshot,
}

#[derive(Clone, Debug, serde::Deserialize, Serialize)]
struct PickConflictReceipt {
    command: String,
    operation_id: String,
    status: String,
    source_state: String,
    source_parent: String,
    target_base: String,
    patch_id: String,
    composition_id: String,
    artifact_id: String,
    conflict_artifact: String,
    artifact_sha256: String,
    preimage_artifact: String,
    next_action: String,
    conflicting_paths: Vec<String>,
    preimage: serde_json::Value,
}
fn fork(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let worktree_index = args
        .iter()
        .position(|value| value == OsStr::new("--worktree"));
    let materialize = worktree_index.is_some();
    let mut filtered = args.to_vec();
    if let Some(index) = worktree_index {
        filtered.remove(index);
    }
    let (format, objective) = capture_arguments(&filtered)?;
    let mut context = context(cwd)?;
    let source = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .ok_or_else(|| RuntimeError::Unavailable("fork requires a current JJK state".into()))?;
    let attempt_id = AttemptId::new_v7();
    let root = materialize
        .then(|| lexical_repository_root(&context.git, cwd))
        .transpose()?;
    let slug = slug(&objective);
    let prefix = attempt_id
        .to_string()
        .chars()
        .skip(3)
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    let branch = materialize.then(|| format!("jjk/{slug}-{prefix}"));
    let worktree_path = root.as_ref().map(|root| {
        Path::new(root)
            .join(".worktrees")
            .join(format!("{slug}--{prefix}"))
    });
    let source_ref = format!("refs/jjk/states/{}", display_state_id(&source.state_id)?);
    let (request, common_dir) = mutation_request(
        &context,
        cwd,
        "fork",
        serde_json::json!({"attempt_id":attempt_id.to_string(),"from_state":display_state_id(&source.state_id)?,"objective":objective,"worktree":materialize}),
        serde_json::json!({"branch":branch,"worktree":worktree_path,"source_ref":source_ref}),
        worktree_path
            .as_ref()
            .map(|path| serde_json::json!({"branch":branch,"worktree":path,"pre_ref":"absent"})),
    )?;
    let fork_locator = worktree_path
        .as_ref()
        .map(|path| workspace_locator(path, &common_dir));
    let fork_workspace_id = fork_locator.as_ref().map(|locator| {
        let mut seed = b"jjk-workspace-v1\0".to_vec();
        seed.extend(&context.root_token);
        seed.push(0);
        seed.extend(locator);
        deterministic_v7_uuid(&seed)
    });
    let git = &context.git;
    let store = &mut context.store;
    let branch_for_effect = branch.clone();
    let path_for_effect = worktree_path.clone();
    let source_oid = source.git_oid.clone();
    let effect =
        move |_prepared: &PreparedOperation<'_>| -> Result<(), EffectFailure<RuntimeError>> {
            if let (Some(branch), Some(path)) = (&branch_for_effect, &path_for_effect) {
                required_os(
                    git,
                    cwd,
                    [
                        OsString::from("branch"),
                        OsString::from(branch),
                        OsString::from(&source_oid),
                    ],
                )
                .map_err(EffectFailure::Indeterminate)?;
                required_os(
                    git,
                    cwd,
                    [
                        OsString::from("worktree"),
                        OsString::from("add"),
                        path.as_os_str().to_owned(),
                        OsString::from(branch),
                    ],
                )
                .map_err(EffectFailure::Indeterminate)?;
            }
            Ok(())
        };
    let branch_for_verify = branch.clone();
    let path_for_verify = worktree_path.clone();
    let source_oid = source.git_oid.clone();
    let verify = move |_effect: &()| -> Result<bool, RuntimeError> {
        let Some(branch) = &branch_for_verify else {
            return Ok(true);
        };
        let branch_oid = observation_required_os(
            git,
            cwd,
            [
                OsString::from("rev-parse"),
                OsString::from(format!("refs/heads/{branch}")),
            ],
        )?;
        Ok(branch_oid == source_oid && path_for_verify.as_ref().is_some_and(|path| path.exists()))
    };
    let projection_source = source.clone();
    let objective_for_commit = objective.clone();
    let workspace_for_commit = fork_workspace_id;
    let locator_for_commit = fork_locator.clone();
    let head_for_commit = materialize.then(|| source.git_oid.clone());
    let payload = serde_json::to_vec(&serde_json::json!({"attempt_id":attempt_id.to_string(),"from_state":display_state_id(&source.state_id)?,"objective":objective,"worktree":materialize})).map_err(internal)?;
    let result_payload = payload.clone();
    let commit = move |_effect: &()| {
        Ok::<RuntimeMutationCommit<RuntimeProjection>, RuntimeError>(fact_commit(
            "AttemptForked",
            payload,
            RuntimeProjection::Fork {
                source: projection_source,
                attempt_id: attempt_id.as_uuid(),
                objective: objective_for_commit,
                workspace_id: workspace_for_commit,
                relative_locator: locator_for_commit,
                head_oid: head_for_commit,
            },
            result_payload,
        ))
    };
    crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
        .map_err(transaction_error)?;
    let workspace_id = fork_workspace_id
        .map(|id| {
            crate::domain::WorkspaceId::from_uuid(id)
                .map(|id| id.to_string())
                .map_err(internal)
        })
        .transpose()?;
    #[derive(Serialize)]
    struct ForkResult {
        command: &'static str,
        attempt_id: String,
        workspace_id: Option<String>,
        from_state: String,
        objective: String,
        source_checkout_mutated: bool,
        branch: Option<String>,
        worktree: Option<String>,
    }
    emit(
        format,
        &ForkResult {
            command: "fork",
            attempt_id: attempt_id.to_string(),
            workspace_id,
            from_state: display_state_id(&source.state_id)?,
            objective,
            source_checkout_mutated: false,
            branch,
            worktree: worktree_path.map(|path| path.display().to_string()),
        },
    )?;
    Ok(0)
}

fn control_history(name: &str, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let mut context = context(cwd)?;
    let current = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?;
    let captured = captured_snapshot(&context, current.as_ref())?;
    if !workspace_matches_state(&context.git, cwd, current.as_ref(), captured.as_ref())? {
        return Err(RuntimeError::Unavailable(format!(
            "refusing {name} because the worktree or index differs from current JJK control state"
        )));
    }
    let plan = context
        .store
        .plan_runtime_control_restore(if name == "undo" { -1 } else { 1 }, context.workspace_id)
        .map_err(unavailable_store)?;
    let before = capture_runtime_git_snapshot(&context.git, cwd)?;
    let leaving = current.as_ref().map(|state| state.git_oid.clone());
    if let Err(error) =
        restore_runtime_git_snapshot(&context.git, cwd, &plan.git, leaving.as_deref())
    {
        let _ = restore_runtime_git_snapshot(&context.git, cwd, &before, None);
        return Err(error);
    }
    if !snapshot_matches_live(&context.git, cwd, &plan.git, leaving.as_deref())? {
        let _ = restore_runtime_git_snapshot(&context.git, cwd, &before, None);
        return Err(RuntimeError::Unavailable(format!(
            "{name} Git snapshot verification failed; original control state restored"
        )));
    }
    let event = runtime_event(
        &context,
        cwd,
        if name == "undo" {
            "ControlUndone"
        } else {
            "ControlRedone"
        },
        serde_json::to_vec(
            &serde_json::json!({"from_cursor":plan.from_cursor,"to_cursor":plan.to_cursor}),
        )
        .map_err(internal)?,
    )?;
    if let Err(error) = context
        .store
        .apply_runtime_control_restore(&event, plan.to_cursor)
    {
        let _ = restore_runtime_git_snapshot(&context.git, cwd, &before, None);
        return Err(internal(error));
    }
    #[derive(Serialize)]
    struct Result<'a> {
        command: &'a str,
        state_id: Option<String>,
        commit: Option<String>,
        from_cursor: usize,
        to_cursor: usize,
    }
    emit(
        format,
        &Result {
            command: name,
            state_id: plan
                .current
                .as_ref()
                .map(|state| display_state_id(&state.state_id))
                .transpose()?,
            commit: plan.current.map(|state| state.git_oid),
            from_cursor: plan.from_cursor,
            to_cursor: plan.to_cursor,
        },
    )?;
    Ok(0)
}

fn completion(args: &[OsString]) -> Result<i32, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::InvalidArguments(
            "usage: jjk completion <bash|zsh|fish|powershell|pwsh>".into(),
        ));
    }
    let shell = args[0]
        .to_str()
        .ok_or_else(|| RuntimeError::InvalidArguments("shell must be UTF-8".into()))?;
    let script = crate::cli::completion::generate_completion(shell)
        .map_err(|error| RuntimeError::InvalidArguments(error.to_string()))?;
    print!("{script}");
    Ok(0)
}
fn visibility(archived: bool, args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let command = if archived { "archive" } else { "recover" };
    let (format, target) = state_argument(args, command)?;
    let mut context = context(cwd)?;
    let state = context
        .store
        .resolve_state_row(&target)
        .map_err(unavailable_store)?;
    if archived
        && context
            .store
            .current_state_row(context.workspace_id)
            .map_err(internal)?
            .as_ref()
            .is_some_and(|current| current.state_id == state.state_id)
    {
        return Err(RuntimeError::Unavailable(
            "refusing to archive the current state; return elsewhere first".into(),
        ));
    }
    let event_type = if archived {
        "StateArchived"
    } else {
        "StateRecovered"
    };
    let state_id = display_state_id(&state.state_id)?;
    let (request, common_dir) = mutation_request(
        &context,
        cwd,
        command,
        serde_json::json!({"state_id":state_id,"archived":archived}),
        serde_json::json!({"projection":"state-visibility","archived":archived}),
        None,
    )?;
    let store = &mut context.store;
    let effect = |_prepared: &PreparedOperation<'_>| Ok::<(), EffectFailure<RuntimeError>>(());
    let verify = |_effect: &()| Ok::<bool, RuntimeError>(true);
    let projection_state = state.clone();
    let state_for_result = state_id.clone();
    let commit =
        move |_effect: &()| -> Result<RuntimeMutationCommit<RuntimeProjection>, RuntimeError> {
            let payload = serde_json::to_vec(&serde_json::json!({"state_id":state_for_result}))
                .map_err(internal)?;
            Ok(fact_commit(
                event_type,
                payload,
                RuntimeProjection::Archive {
                    state: projection_state,
                    archived,
                },
                serde_json::to_vec(
                    &serde_json::json!({"state_id":state_for_result,"archived":archived}),
                )
                .map_err(internal)?,
            ))
        };
    crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
        .map_err(transaction_error)?;
    #[derive(Serialize)]
    struct VisibilityResult {
        command: &'static str,
        state_id: String,
        archived: bool,
    }
    emit(
        format,
        &VisibilityResult {
            command,
            state_id,
            archived,
        },
    )?;
    Ok(0)
}
fn recover(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    if args
        .iter()
        .any(|value| value == OsStr::new("--abort") || value == OsStr::new("--resume"))
    {
        return recover_pick_conflict(args, cwd);
    }
    let operation_id = args
        .iter()
        .filter_map(|value| value.to_str())
        .find(|value| !value.starts_with('-'))
        .and_then(|value| Uuid::parse_str(value).ok());
    if let Some(operation_id) = operation_id {
        let context = context(cwd)?;
        if crate::ports::operation::OperationStore::operation(&context.store, operation_id)
            .map_err(internal)?
            .is_some()
        {
            return recover_pick_conflict(args, cwd);
        }
    }
    visibility(false, args, cwd)
}

fn recover_pick_conflict(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let (format, operation_id, action) = conflict_recover_arguments(args)?;
    let mut context = context(cwd)?;
    let operation =
        crate::ports::operation::OperationStore::operation(&context.store, operation_id)
            .map_err(internal)?
            .ok_or_else(|| {
                RuntimeError::Unavailable(format!("operation {operation_id} does not exist"))
            })?;
    if operation.command_kind != "pick" {
        return Err(RuntimeError::Unavailable(format!(
            "operation {operation_id} is not a pick conflict"
        )));
    }
    let receipt = conflict_receipt(&operation)?;
    match action {
        ConflictRecoveryAction::Inspect => {
            let mut result = serde_json::to_value(&receipt).map_err(internal)?;
            result["command"] = serde_json::Value::String("recover".into());
            result["status"] = serde_json::Value::String(operation.status.as_str().into());
            emit(format, &result)?;
            Ok(0)
        }
        ConflictRecoveryAction::Resume => Err(RuntimeError::Unavailable(format!(
            "operation {operation_id} recovery required: conflict resolution resume is unsupported; inspect {} and abort or resolve explicitly",
            receipt.conflict_artifact
        ))),
        ConflictRecoveryAction::Abort => {
            if operation.status != crate::ports::operation::OperationStatus::AwaitingResolution {
                return Err(RuntimeError::Unavailable(format!(
                    "operation {operation_id} is {}, not awaiting resolution",
                    operation.status.as_str()
                )));
            }
            let snapshot: crate::adapters::sqlite::RuntimeGitSnapshot =
                serde_json::from_slice(&fs::read(&receipt.preimage_artifact).map_err(internal)?)
                    .map_err(|error| {
                        RuntimeError::Internal(format!(
                            "invalid conflict preimage artifact: {error}"
                        ))
                    })?;
            let needs_restore = !snapshot_matches_live(&context.git, cwd, &snapshot, None)?;
            let head = context.store.head().map_err(internal)?;
            let aborting = recovery_lifecycle_event(
                &context,
                operation_id,
                &operation.command_kind,
                crate::app::transaction::LifecycleEvent::Aborting,
                operation_event_count(&context.store, operation_id)?,
                head,
            )?;
            context
                .store
                .transition_operation(
                    head,
                    &aborting,
                    operation_id,
                    crate::ports::operation::OperationStatus::Aborting,
                    None,
                )
                .map_err(internal)?;
            if needs_restore {
                restore_runtime_git_snapshot(&context.git, cwd, &snapshot, None)?;
            }
            let head = context.store.head().map_err(internal)?;
            let aborted = recovery_lifecycle_event(
                &context,
                operation_id,
                &operation.command_kind,
                crate::app::transaction::LifecycleEvent::Aborted,
                operation_event_count(&context.store, operation_id)?,
                head,
            )?;
            context
                .store
                .transition_operation(
                    head,
                    &aborted,
                    operation_id,
                    crate::ports::operation::OperationStatus::Aborted,
                    None,
                )
                .map_err(internal)?;
            #[derive(Serialize)]
            struct Result<'a> {
                command: &'static str,
                operation_id: String,
                status: &'static str,
                conflict_artifact: &'a str,
                next_action: &'static str,
            }
            emit(
                format,
                &Result {
                    command: "recover",
                    operation_id: operation_id.to_string(),
                    status: "aborted",
                    conflict_artifact: &receipt.conflict_artifact,
                    next_action: "retry_pick",
                },
            )?;
            Ok(0)
        }
    }
}
#[derive(Clone, Copy)]
enum ConflictRecoveryAction {
    Inspect,
    Abort,
    Resume,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PendingOperationView {
    pub(crate) operation_id: String,
    pub(crate) command: String,
    pub(crate) command_kind: String,
    pub(crate) phase: String,
    pub(crate) status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) conflict_artifact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) artifact_sha256: Option<String>,
    pub(crate) next_action: String,
}

pub(crate) fn pending_operations(
    store: &SqliteStore,
) -> Result<Vec<PendingOperationView>, RuntimeError> {
    let operations =
        crate::ports::operation::OperationStore::pending_operations(store).map_err(internal)?;
    operations.into_iter().map(|operation| {
        let (conflict_artifact, artifact_sha256, next_action) = if operation.command_kind == "pick"
            && operation.status == crate::ports::operation::OperationStatus::AwaitingResolution
        {
            let receipt = conflict_receipt(&operation)?;
            (Some(receipt.conflict_artifact), Some(receipt.artifact_sha256), receipt.next_action)
        } else {
            (None, None, format!(
                "preserve work and inspect operation {} with `jjk doctor --json` before another mutation",
                operation.operation_id
            ))
        };
        let status = operation.status.as_str().to_owned();
        Ok(PendingOperationView {
            operation_id: operation.operation_id.to_string(),
            command: operation.command_kind.clone(),
            command_kind: operation.command_kind,
            phase: status.clone(),
            status,
            conflict_artifact,
            artifact_sha256,
            next_action,
        })
    }).collect()
}

pub(crate) fn pending_conflict_operations(
    store: &SqliteStore,
) -> Result<Vec<PendingOperationView>, RuntimeError> {
    Ok(pending_operations(store)?
        .into_iter()
        .filter(|operation| operation.command == "pick")
        .collect())
}

fn conflict_recover_arguments(
    args: &[OsString],
) -> Result<(Format, Uuid, ConflictRecoveryAction), RuntimeError> {
    let mut format = Format::Human(default_width());
    let mut operation_id = None;
    let mut action = None;
    for argument in args {
        match argument.to_str().ok_or_else(|| {
            RuntimeError::InvalidArguments("recover arguments must be UTF-8".into())
        })? {
            "--json" | "--format=json" => format = Format::Json,
            "--format=human" | "--no-color" => {}
            "--abort" if action.is_none() => action = Some(ConflictRecoveryAction::Abort),
            "--resume" if action.is_none() => action = Some(ConflictRecoveryAction::Resume),
            value if value.starts_with('-') => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "unknown option `{value}`"
                )));
            }
            value if operation_id.is_none() => {
                operation_id = Some(Uuid::parse_str(value).map_err(|_| {
                    RuntimeError::InvalidArguments("recover operation id must be a UUID".into())
                })?)
            }
            _ => {
                return Err(RuntimeError::InvalidArguments(
                    "recover requires exactly one operation id".into(),
                ));
            }
        }
    }
    Ok((
        format,
        operation_id.ok_or_else(|| {
            RuntimeError::InvalidArguments("recover requires an operation id".into())
        })?,
        action.unwrap_or(ConflictRecoveryAction::Inspect),
    ))
}

fn conflict_receipt(
    operation: &crate::ports::operation::OperationRecord,
) -> Result<PickConflictReceipt, RuntimeError> {
    serde_json::from_slice(operation.result.as_deref().ok_or_else(|| {
        RuntimeError::Internal(format!(
            "operation {} has no conflict receipt",
            operation.operation_id
        ))
    })?)
    .map_err(|error| {
        RuntimeError::Internal(format!(
            "invalid conflict receipt for operation {}: {error}",
            operation.operation_id
        ))
    })
}

fn operation_event_count(store: &SqliteStore, operation_id: Uuid) -> Result<u32, RuntimeError> {
    let head = store.head().map_err(internal)?;
    let limit = usize::try_from(head.local_seq)
        .map_err(internal)?
        .saturating_add(1);
    let count = store
        .events_after(0, limit)
        .map_err(internal)?
        .into_iter()
        .filter(|event| event.record.operation_id == operation_id)
        .count();
    u32::try_from(count).map_err(internal)
}

fn recovery_lifecycle_event(
    context: &RuntimeContext,
    operation_id: Uuid,
    command_kind: &str,
    phase: crate::app::transaction::LifecycleEvent,
    ordinal: u32,
    head: crate::ports::journal::JournalHead,
) -> Result<EventRecord, RuntimeError> {
    let (event_type, phase_name) = match phase {
        crate::app::transaction::LifecycleEvent::Prepared => ("OperationPrepared", "prepared"),
        crate::app::transaction::LifecycleEvent::Applying => ("ApplyStarted", "applying"),
        crate::app::transaction::LifecycleEvent::ConflictPaused => {
            ("ConflictPaused", "awaiting_resolution")
        }
        crate::app::transaction::LifecycleEvent::Aborting => ("AbortStarted", "aborting"),
        crate::app::transaction::LifecycleEvent::Aborted => ("OperationAborted", "aborted"),
        crate::app::transaction::LifecycleEvent::Verifying => ("VerificationStarted", "verifying"),
        crate::app::transaction::LifecycleEvent::RepairRequired => {
            ("RepairRequired", "repair_required")
        }
    };
    let mut event = EventRecord {
        event_id: Uuid::now_v7(),
        repo_id: context.store.repository_uuid().map_err(internal)?,
        event_type: event_type.into(),
        event_schema_version: 1,
        envelope_version: crate::adapters::sqlite::ENVELOPE_VERSION,
        operation_id,
        operation_ordinal: ordinal,
        actor_id: Uuid::now_v7(),
        actor_kind: ActorKind::Human,
        recorded_at_utc: now_utc()?,
        observed_at_utc: None,
        repository_fingerprint: repository_fingerprint(
            &context.git,
            context
                .git
                .discover(Path::new("."))
                .ok()
                .and_then(|discovery| discovery.worktree_root)
                .as_deref()
                .unwrap_or(Path::new(".")),
            context.store.repository_uuid().map_err(internal)?,
            &context.root_token,
        )?,
        payload_codec: PayloadCodec::CanonicalJsonV1,
        payload: serde_json::to_vec(
            &serde_json::json!({"command_kind":command_kind,"phase":phase_name}),
        )
        .map_err(internal)?,
        provenance: b"runtime-v1".to_vec(),
        evidence_manifest: Vec::new(),
        dedup_key: None,
        previous_event_hash: head.event_hash,
        event_hash: [0; 32],
    };
    event.event_hash = runtime_mutation_event_digest(&event);
    Ok(event)
}

fn runtime_mutation_event_digest(event: &EventRecord) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"jjk-runtime-mutation-event-v1\0");
    hash.update(event.event_id.as_bytes());
    hash.update(event.repo_id.as_bytes());
    hash.update(event.operation_id.as_bytes());
    hash.update(event.operation_ordinal.to_be_bytes());
    hash.update(event.previous_event_hash);
    for bytes in [
        event.event_type.as_bytes(),
        event.repository_fingerprint.as_slice(),
        event.payload.as_slice(),
        event.provenance.as_slice(),
        event.evidence_manifest.as_slice(),
    ] {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.finalize().into()
}

fn freeze(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let action = args
        .first()
        .and_then(|value| value.to_str())
        .unwrap_or("create");
    let mut tail = if matches!(action, "create" | "verify" | "inspect") {
        &args[1..]
    } else {
        args
    };
    let format = extract_format(&mut tail)?;
    if matches!(action, "verify" | "inspect") {
        if tail.len() != 1 {
            return Err(RuntimeError::InvalidArguments(
                "freeze verify requires one path".into(),
            ));
        }
        let path = Path::new(&tail[0]);
        let manifest = verify_freeze(path)?;
        #[derive(Serialize)]
        struct FreezeReadResult {
            command: &'static str,
            action: &'static str,
            path: String,
            freeze_id: String,
            healthy: bool,
            included_state_ids: serde_json::Value,
            required_oids: serde_json::Value,
        }
        emit(
            format,
            &FreezeReadResult {
                command: "freeze",
                action: "verified",
                path: path.display().to_string(),
                freeze_id: manifest["freeze_id"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned(),
                healthy: true,
                included_state_ids: manifest["included_state_ids"].clone(),
                required_oids: manifest["required_oids"].clone(),
            },
        )?;
        return Ok(0);
    }
    if action != "create" {
        return Err(RuntimeError::InvalidArguments(
            "usage: jjk freeze create [path] | verify <path>".into(),
        ));
    }
    if tail.len() > 1 {
        return Err(RuntimeError::InvalidArguments(
            "freeze create accepts at most one path".into(),
        ));
    }
    let mut context = context(cwd)?;
    let freeze_id = format!("freeze_{}", Uuid::now_v7().simple());
    let destination = tail.first().map(PathBuf::from).unwrap_or_else(|| {
        context
            .store
            .path()
            .parent()
            .expect("store parent")
            .join("freezes")
            .join(format!("{freeze_id}.jjkfreeze"))
    });
    let reservation = SafeDestination::new(&destination)
        .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;
    let states = context.store.state_rows().map_err(internal)?;
    let required_oids = states
        .iter()
        .map(|state| state.git_oid.clone())
        .collect::<Vec<_>>();
    let included_state_ids = states
        .iter()
        .map(|state| display_state_id(&state.state_id))
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = serde_json::json!({"format":"jjk-freeze","format_version":1,"freeze_id":freeze_id,"included_state_ids":included_state_ids,"required_oids":required_oids});
    let (request, common_dir) = mutation_request(
        &context,
        cwd,
        "freeze",
        serde_json::json!({"action":"create","destination":destination,"freeze_id":freeze_id}),
        serde_json::json!({"bundle_revision":"--all","publish_directory":destination}),
        Some(serde_json::json!({"destination":destination,"must_be_absent":true})),
    )?;
    let git = &context.git;
    let store = &mut context.store;
    let manifest_for_effect = manifest.clone();
    let effect = move |_prepared: &PreparedOperation<'_>| {
        (|| -> Result<PathBuf, RuntimeError> {
            let staging = reservation.create_staging_directory().map_err(internal)?;
            let root = staging.external_path_verified().map_err(internal)?;
            fs::create_dir_all(root.join("metadata")).map_err(internal)?;
            fs::create_dir_all(root.join("git")).map_err(internal)?;
            required_os(
                git,
                cwd,
                [
                    OsString::from("bundle"),
                    OsString::from("create"),
                    root.join("git/objects.bundle").into_os_string(),
                    OsString::from("--all"),
                ],
            )?;
            fs::write(root.join("metadata/events.cbor"), b"[]").map_err(internal)?;
            fs::write(
                root.join("metadata/view.json"),
                serde_json::to_vec(&states).map_err(internal)?,
            )
            .map_err(internal)?;
            let bytes = serde_json::to_vec(&manifest_for_effect).map_err(internal)?;
            fs::write(root.join("manifest.json"), &bytes).map_err(internal)?;
            fs::write(
                root.join("manifest.sha256"),
                format!("{}\n", hex::encode(Sha256::digest(&bytes))),
            )
            .map_err(internal)?;
            verify_freeze(&root)?;
            reservation.publish_directory(staging).map_err(internal)
        })()
        .map_err(EffectFailure::Indeterminate)
    };
    let destination_for_verify = destination.clone();
    let verify = move |published: &PathBuf| -> Result<bool, RuntimeError> {
        Ok(published == &destination_for_verify && verify_freeze(published).is_ok())
    };
    let freeze_id_for_commit = freeze_id.clone();
    let destination_for_commit = destination.clone();
    let manifest_for_commit = manifest.clone();
    let commit = move |_published: &PathBuf| -> Result<RuntimeMutationCommit<RuntimeProjection>, RuntimeError> {
        let payload = serde_json::to_vec(&serde_json::json!({"freeze_id":freeze_id_for_commit,"path":destination_for_commit,"manifest":manifest_for_commit})).map_err(internal)?;
        Ok(fact_commit("FreezeCreated", payload.clone(), RuntimeProjection::Record { kind:"freeze".into(), id:freeze_id_for_commit, value:payload.clone() }, payload))
    };
    crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
        .map_err(transaction_error)?;
    #[derive(Serialize)]
    struct FreezeCreateResult {
        command: &'static str,
        action: &'static str,
        path: String,
        freeze_id: String,
        healthy: bool,
        included_state_ids: Vec<String>,
        required_oids: Vec<String>,
    }
    emit(
        format,
        &FreezeCreateResult {
            command: "freeze",
            action: "created",
            path: destination.display().to_string(),
            freeze_id,
            healthy: true,
            included_state_ids,
            required_oids,
        },
    )?;
    Ok(0)
}

fn verify_freeze(path: &Path) -> Result<serde_json::Value, RuntimeError> {
    let bytes = fs::read(path.join("manifest.json")).map_err(internal)?;
    let checksum = fs::read_to_string(path.join("manifest.sha256")).map_err(internal)?;
    if checksum.trim() != hex::encode(Sha256::digest(&bytes)) {
        return Err(RuntimeError::Internal(
            "freeze manifest checksum mismatch".into(),
        ));
    }
    if !path.join("git/objects.bundle").is_file()
        || !path.join("metadata/events.cbor").is_file()
        || !path.join("metadata/view.json").is_file()
    {
        return Err(RuntimeError::Internal(
            "freeze artifact is incomplete".into(),
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| RuntimeError::Internal(format!("freeze manifest is corrupt: {error}")))
}

fn validate(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let parsed = crate::cli::output::parse_native_output(args).map_err(|_| {
        RuntimeError::InvalidArguments("invalid validation presentation option".into())
    })?;
    let format = if parsed.mode == crate::cli::output::OutputMode::Json {
        Format::Json
    } else {
        Format::Human(parsed.width.unwrap_or_else(default_width))
    };
    let semantic = parsed
        .semantic_args()
        .map(OsStr::to_os_string)
        .collect::<Vec<_>>();
    let delimiter = semantic
        .iter()
        .position(|v| v == OsStr::new("--"))
        .ok_or_else(|| {
            RuntimeError::InvalidArguments(
                "usage: jjk validate <state> --suite <name> -- <program> [args...]".into(),
            )
        })?;
    let left = &semantic[..delimiter];
    let command = &semantic[delimiter + 1..];
    if command.is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "validation requires a program".into(),
        ));
    }
    let subject = left
        .first()
        .and_then(|v| v.to_str())
        .ok_or_else(|| RuntimeError::InvalidArguments("validation requires a state".into()))?;
    let suite_index = left
        .iter()
        .position(|v| v == OsStr::new("--suite"))
        .ok_or_else(|| RuntimeError::InvalidArguments("validation requires --suite".into()))?;
    let suite = left
        .get(suite_index + 1)
        .and_then(|v| v.to_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| RuntimeError::InvalidArguments("--suite requires a name".into()))?
        .to_owned();
    if left.len() != 3 || suite_index != 1 {
        return Err(RuntimeError::InvalidArguments(
            "usage: jjk validate <state> --suite <name> -- <program> [args...]".into(),
        ));
    }
    let mut context = context(cwd)?;
    let state = context
        .store
        .resolve_state_row(subject)
        .map_err(unavailable_store)?;
    let validation_id = ValidationId::new_v7();
    let tree = required_os(
        &context.git,
        cwd,
        [
            OsString::from("rev-parse"),
            OsString::from(format!("{}^{{tree}}", state.git_oid)),
        ],
    )?;
    let content_digest = format!("{}:{}", context.object_format, tree);
    let argv = command
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let (request, common_dir) = mutation_request(
        &context,
        cwd,
        "validate",
        serde_json::json!({"validation_id":validation_id,"subject_state":display_state_id(&state.state_id)?,"suite":suite,"argv":argv}),
        serde_json::json!({"execute_validation":argv}),
        None,
    )?;
    let child = command.to_vec();
    let effect = move |_prepared: &PreparedOperation<'_>| {
        Command::new(&child[0])
            .args(&child[1..])
            .current_dir(cwd)
            .output()
            .map_err(|error| {
                EffectFailure::Indeterminate(RuntimeError::Internal(error.to_string()))
            })
    };
    let verify = |_output: &std::process::Output| Ok::<bool, RuntimeError>(true);
    let id = validation_id.to_string();
    let subject_id = display_state_id(&state.state_id)?;
    let suite_value = suite.clone();
    let argv_value = argv.clone();
    let recorded_at = now_utc()?;
    let commit=move|output:&std::process::Output|->Result<RuntimeMutationCommit<RuntimeProjection>,RuntimeError>{let exit_code=output.status.code().unwrap_or(1);let outcome=if output.status.success(){"pass"}else{"fail"};let evidence_digest=hex::encode(Sha256::digest(serde_json::to_vec(&serde_json::json!({"argv":argv_value,"stdout":hex::encode(&output.stdout),"stderr":hex::encode(&output.stderr),"exit_code":exit_code})).map_err(internal)?));let value=serde_json::json!({"command":"validate","validation_id":id,"subject_state":subject_id,"suite":suite_value,"outcome":outcome,"argv":argv_value,"content_digest":content_digest,"evidence_digest":evidence_digest,"exit_code":exit_code,"recorded_at":recorded_at});let bytes=serde_json::to_vec(&value).map_err(internal)?;Ok(fact_commit("ValidationRecorded",bytes.clone(),RuntimeProjection::Record{kind:"validation".into(),id:id.clone(),value:bytes.clone()},bytes))};
    let store = &mut context.store;
    let operation =
        crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
            .map_err(transaction_error)?;
    let bytes = operation
        .result
        .ok_or_else(|| RuntimeError::Internal("validation committed without result".into()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(internal)?;
    let exit_code = value["exit_code"].as_i64().unwrap_or(1) as i32;
    emit(format, &value)?;
    Ok(exit_code)
}

fn handoff(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let action = args.first().and_then(|v| v.to_str()).ok_or_else(|| {
        RuntimeError::InvalidArguments("usage: jjk handoff create|show|consume".into())
    })?;
    let mut tail = &args[1..];
    let format = extract_format(&mut tail)?;
    let mut context = context(cwd)?;
    match action {
        "create" => {
            if tail.first() != Some(&OsString::from("--request")) || tail.len() != 2 {
                return Err(RuntimeError::InvalidArguments(
                    "usage: jjk handoff create --request <handoff.json>".into(),
                ));
            }
            let request = crate::app::command::handoff::parse_runtime_handoff(
                &fs::read(Path::new(&tail[1])).map_err(internal)?,
            )
            .map_err(RuntimeError::InvalidArguments)?;
            context
                .store
                .resolve_state_row(&request.base_state.to_string())
                .map_err(unavailable_store)?;
            if let Some(state) = request.produced_state {
                context
                    .store
                    .resolve_state_row(&state.to_string())
                    .map_err(unavailable_store)?;
            }
            for validation in &request.validation_ids {
                if context
                    .store
                    .runtime_record("validation", &validation.to_string())
                    .map_err(internal)?
                    .is_none()
                {
                    return Err(RuntimeError::Unavailable(format!(
                        "validation `{validation}` not found"
                    )));
                }
            }
            let id = HandoffId::new_v7().to_string();
            let created_at = now_utc()?;
            let owner = serde_json::json!({"actor_id":request.owner.actor_id.to_string(),"worker_id":request.owner.worker_id.map(|id|id.to_string())});
            let value = serde_json::json!({"command":"handoff","action":"create","handoff_id":id,"owner":owner,"objective":request.objective,"base_state":request.base_state.to_string(),"produced_state":request.produced_state.map(|id|id.to_string()),"validation_ids":request.validation_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),"remaining_risks":request.remaining_risks,"resume":{"program":request.resume.program,"args":request.resume.args,"cwd":request.resume.cwd},"status":"offered","created_at":created_at,"version":1});
            let bytes = serde_json::to_vec(&value).map_err(internal)?;
            let (mutation, common_dir) = mutation_request(
                &context,
                cwd,
                "handoff-create",
                value.clone(),
                serde_json::json!({"record":"handoff","handoff_id":id}),
                None,
            )?;
            let record_id = id.clone();
            let record = bytes.clone();
            let effect = |_: &PreparedOperation<'_>| Ok::<(), EffectFailure<RuntimeError>>(());
            let verify = |_: &()| Ok::<bool, RuntimeError>(true);
            let commit = move |_: &()| {
                Ok::<RuntimeMutationCommit<RuntimeProjection>, RuntimeError>(fact_commit(
                    "HandoffCreated",
                    record.clone(),
                    RuntimeProjection::Record {
                        kind: "handoff".into(),
                        id: record_id.clone(),
                        value: record.clone(),
                    },
                    record.clone(),
                ))
            };
            let operation = crate::app::runtime_mutation::execute(
                &common_dir,
                &mut context.store,
                mutation,
                effect,
                verify,
                commit,
            )
            .map_err(transaction_error)?;
            let output: serde_json::Value =
                serde_json::from_slice(&operation.result.ok_or_else(|| {
                    RuntimeError::Internal("handoff committed without result".into())
                })?)
                .map_err(internal)?;
            emit(format, &output)?;
        }
        "show" | "consume" => {
            if tail.len() != 1 {
                return Err(RuntimeError::InvalidArguments(format!(
                    "handoff {action} requires one id"
                )));
            }
            let id = tail[0]
                .to_str()
                .ok_or_else(|| RuntimeError::InvalidArguments("handoff id must be UTF-8".into()))?;
            id.parse::<HandoffId>()
                .map_err(|error| RuntimeError::InvalidArguments(error.to_string()))?;
            let bytes = context
                .store
                .runtime_record("handoff", id)
                .map_err(internal)?
                .ok_or_else(|| RuntimeError::Unavailable(format!("handoff `{id}` not found")))?;
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).map_err(internal)?;
            if action == "consume" {
                if value["status"] != "offered" {
                    return Err(RuntimeError::Unavailable(format!(
                        "handoff `{id}` was already accepted"
                    )));
                }
                value["status"] = serde_json::Value::String("accepted".into());
                value["accepted_at"] = serde_json::Value::String(now_utc()?);
                value["action"] = serde_json::Value::String("consume".into());
                let updated = serde_json::to_vec(&value).map_err(internal)?;
                let (mutation, common_dir) = mutation_request(
                    &context,
                    cwd,
                    "handoff-consume",
                    serde_json::json!({"handoff_id":id}),
                    serde_json::json!({"transition":"offered-to-accepted"}),
                    None,
                )?;
                let record_id = id.to_owned();
                let record = updated.clone();
                let effect = |_: &PreparedOperation<'_>| Ok::<(), EffectFailure<RuntimeError>>(());
                let verify = |_: &()| Ok::<bool, RuntimeError>(true);
                let commit = move |_: &()| {
                    Ok::<RuntimeMutationCommit<RuntimeProjection>, RuntimeError>(fact_commit(
                        "HandoffAccepted",
                        record.clone(),
                        RuntimeProjection::Record {
                            kind: "handoff".into(),
                            id: record_id.clone(),
                            value: record.clone(),
                        },
                        record.clone(),
                    ))
                };
                let operation = crate::app::runtime_mutation::execute(
                    &common_dir,
                    &mut context.store,
                    mutation,
                    effect,
                    verify,
                    commit,
                )
                .map_err(transaction_error)?;
                value = serde_json::from_slice(&operation.result.ok_or_else(|| {
                    RuntimeError::Internal("handoff acceptance committed without result".into())
                })?)
                .map_err(internal)?;
            } else {
                value["action"] = serde_json::Value::String("show".into());
            }
            emit(format, &value)?;
        }
        _ => {
            return Err(RuntimeError::InvalidArguments(
                "usage: jjk handoff create|show|consume".into(),
            ));
        }
    }
    Ok(0)
}

fn activate_state(
    command: &str,
    format: Format,
    context: &mut RuntimeContext,
    cwd: &Path,
    state: RuntimeStateRow,
) -> Result<i32, RuntimeError> {
    execute_activation(command, context, cwd, &state, None, None)?;
    #[derive(Serialize)]
    struct ActivationResult<'a> {
        command: &'a str,
        state_id: String,
        commit: String,
    }
    emit(
        format,
        &ActivationResult {
            command,
            state_id: display_state_id(&state.state_id)?,
            commit: state.git_oid,
        },
    )?;
    Ok(0)
}

fn execute_activation(
    command: &str,
    context: &mut RuntimeContext,
    cwd: &Path,
    state: &RuntimeStateRow,
    navigation: Option<RuntimeNavigation>,
    saved_git: Option<RuntimeGitSnapshot>,
) -> Result<(), RuntimeError> {
    let leaving = context
        .store
        .current_state_row(context.workspace_id)
        .map_err(internal)?
        .map(|row| row.git_oid);
    let leaving_for_verify = leaving.clone();
    let pre_head = observation_optional(&context.git, cwd, ["rev-parse", "--verify", "HEAD"])?;
    let pre_index = hex::encode(Sha256::digest(observation_required(
        &context.git,
        cwd,
        ["ls-files", "--stage"],
    )?));
    let pre_status = observation_required(&context.git, cwd, ["status", "--porcelain=v2"])?;
    let target_id = display_state_id(&state.state_id)?;
    let (request, common_dir) = mutation_request(
        context,
        cwd,
        command,
        serde_json::json!({"state_id":target_id,"workspace_id":context.workspace_id}),
        serde_json::json!({"checkout_detached":state.git_oid,"activate_projection":true,"navigation":navigation}),
        Some(
            serde_json::json!({"pre_head":pre_head,"pre_index_entries_sha256":pre_index,"pre_status":pre_status,"target":state.git_oid}),
        ),
    )?;
    let git = &context.git;
    // Navigation is a control-plane operation: record before/after Git control snapshots so
    // `undo` reverts the navigation itself rather than the most recent capture.
    let before_git = capture_runtime_git_snapshot(git, cwd)?;
    let store = &mut context.store;
    let target_for_effect = state.clone();
    let saved_git_verify = saved_git.clone();
    let effect =
        move |_prepared: &PreparedOperation<'_>| -> Result<String, EffectFailure<RuntimeError>> {
            if let Some(snapshot) = &saved_git {
                restore_runtime_git_snapshot(git, cwd, snapshot, leaving.as_deref())
                    .map_err(EffectFailure::Indeterminate)?;
            } else {
                restore_state_checkout(git, cwd, &target_for_effect)
                    .map_err(EffectFailure::Indeterminate)?;
            }
            Ok(target_for_effect.git_oid.clone())
        };
    let target_oid = state.git_oid.clone();
    let verified_live = std::cell::RefCell::new(None);
    let verified_live_ref = &verified_live;
    let verify = move |_effect: &String| -> Result<bool, RuntimeError> {
        if let Some(expected) = &saved_git_verify {
            let (matches, live) =
                snapshot_matches_live_with(git, cwd, expected, leaving_for_verify.as_deref())?;
            *verified_live_ref.borrow_mut() = Some(live);
            return Ok(matches);
        }
        let state_ref = format!("refs/jjk/states/{}", display_state_id(&state.state_id)?);
        Ok(
            observation_optional(git, cwd, ["rev-parse", "--verify", &state_ref])?
                == Some(target_oid.clone())
                && workspace_matches_state(git, cwd, Some(state), None)?,
        )
    };
    let workspace_id = context.workspace_id;
    let relative_locator = context.relative_locator.clone();
    let projection_state = state.clone();
    let navigation_projection = navigation.map(|value| {
        RuntimeProjection::Raw(crate::ports::projection::ProjectionUpdate {
            projection_name: "runtime-navigation-v1".to_owned(),
            reducer_version: 1,
            key: workspace_id.as_bytes().to_vec(),
            value: serde_json::to_vec(&value).expect("runtime navigation serializes"),
            event_index: 0,
        })
    });
    let command_name = command.to_owned();
    let state_id = target_id.clone();
    let commit =
        move |_effect: &String| -> Result<RuntimeMutationCommit<RuntimeProjection>, RuntimeError> {
            let payload = serde_json::to_vec(
                &serde_json::json!({"state_id":state_id,"command":command_name}),
            )
            .map_err(internal)?;
            let mut projections = vec![RuntimeProjection::Activate {
                state: projection_state.clone(),
                workspace_id,
                relative_locator: relative_locator.clone(),
                head_oid: projection_state.git_oid.clone(),
            }];
            projections.extend(navigation_projection);
            let after = match verified_live_ref.borrow_mut().take() {
                Some(live) => live,
                None => capture_runtime_git_snapshot(git, cwd)?,
            };
            projections.push(RuntimeProjection::ControlGit {
                before: before_git.clone(),
                after,
            });
            Ok(RuntimeMutationCommit {
                facts: vec![runtime_fact("StateActivated", payload)],
                projections,
                result: serde_json::to_vec(
                    &serde_json::json!({"state_id":state_id,"commit":projection_state.git_oid}),
                )
                .map_err(internal)?,
            })
        };
    crate::app::runtime_mutation::execute(&common_dir, store, request, effect, verify, commit)
        .map_err(transaction_error)?;
    Ok(())
}

/// Restores an ordinary semantic state without moving any branch or JJK reachability ref.
///
/// No attempt-branch attachment is inferred here: the runtime does not yet persist a verified
/// attempt-to-branch ownership binding. Detaching is the only truthful attachment and gives Git,
/// the index, and the worktree one coherent target while every existing future remains reachable.
fn restore_state_checkout(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    state: &RuntimeStateRow,
) -> Result<(), RuntimeError> {
    required_os(
        git,
        cwd,
        [
            OsString::from("checkout"),
            OsString::from("--detach"),
            OsString::from("--force"),
            OsString::from(&state.git_oid),
        ],
    )?;
    Ok(())
}
fn backup(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let subcommand = args
        .first()
        .and_then(|value| value.to_str())
        .unwrap_or("create");
    let mut tail = if matches!(subcommand, "create" | "verify") {
        &args[1..]
    } else {
        args
    };
    if !matches!(subcommand, "create" | "verify") {
        return Err(RuntimeError::InvalidArguments(
            "usage: jjk backup [create [path] | verify <path>] [--json]".into(),
        ));
    }
    let format = extract_format(&mut tail)?;
    let context = context(cwd)?;
    match subcommand {
        "create" => {
            let destination = tail
                .first()
                .map(Path::new)
                .map(Path::to_path_buf)
                .unwrap_or_else(|| {
                    context
                        .store
                        .path()
                        .parent()
                        .expect("store parent")
                        .join("backups")
                        .join(format!("{}.sqlite3", backup_stamp()))
                });
            let destination = SafeDestination::new(&destination)
                .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;
            let snapshot = capture_runtime_git_snapshot_inline(&context.git, cwd)?;
            let object_bundle = create_runtime_git_bundle(
                &context.git,
                cwd,
                context.store.path().parent().expect("store parent"),
            )?;
            let info = context
                .store
                .verified_backup_to(destination, &snapshot, &object_bundle)
                .map_err(internal)?;
            #[derive(Serialize)]
            struct Result {
                command: &'static str,
                action: &'static str,
                path: String,
                schema_version: u32,
                journal_events: u64,
                journal_head: String,
            }
            emit(
                format,
                &Result {
                    command: "backup",
                    action: "created",
                    path: info.path.display().to_string(),
                    schema_version: info.schema.schema_version,
                    journal_events: info.head.local_seq,
                    journal_head: hex::encode(info.head.event_hash),
                },
            )?;
        }
        "verify" => {
            let path = tail.first().map(Path::new).ok_or_else(|| {
                RuntimeError::InvalidArguments("`jjk backup verify` requires a path".into())
            })?;
            if tail.len() != 1 {
                return Err(RuntimeError::InvalidArguments(
                    "`jjk backup verify` requires exactly one path".into(),
                ));
            }
            let info = SqliteStore::verify_backup_file(path).map_err(internal)?;
            #[derive(Serialize)]
            struct Result {
                command: &'static str,
                action: &'static str,
                path: String,
                healthy: bool,
                schema_version: u32,
                journal_events: u64,
                journal_head: String,
            }
            emit(
                format,
                &Result {
                    command: "backup",
                    action: "verified",
                    path: info.path.display().to_string(),
                    healthy: true,
                    schema_version: info.schema.schema_version,
                    journal_events: info.head.local_seq,
                    journal_head: hex::encode(info.head.event_hash),
                },
            )?;
        }
        _ => unreachable!(),
    }
    Ok(0)
}

fn load(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let mut tail = args;
    let format = extract_format(&mut tail)?;
    let backup = tail.first().map(PathBuf::from).ok_or_else(|| {
        RuntimeError::InvalidArguments(
            "usage: jjk load <backup.sqlite3> --into <new-repository> [--json]".into(),
        )
    })?;
    if tail.get(1).and_then(|value| value.to_str()) != Some("--into") || tail.len() != 3 {
        return Err(RuntimeError::InvalidArguments(
            "usage: jjk load <backup.sqlite3> --into <new-repository> [--json]".into(),
        ));
    }
    let target = PathBuf::from(&tail[2]);
    if target.exists() {
        return Err(RuntimeError::Unavailable(format!(
            "load target already exists: {}",
            target.display()
        )));
    }
    let info = SqliteStore::verify_backup_file(&backup).map_err(internal)?;
    let snapshot = SqliteStore::read_backup_git_snapshot(&backup).map_err(internal)?;
    let object_bundle = SqliteStore::read_backup_git_bundle(&backup).map_err(internal)?;
    let reservation = SafeDestination::new(&target)
        .map_err(|error| RuntimeError::Unavailable(error.to_string()))?;
    let staging = reservation.create_staging_directory().map_err(internal)?;
    let staging_path = staging.external_path_verified().map_err(internal)?;
    let final_token = repository_root_token_for_future(&target.join(".git"))?;
    let restored_workspace =
        SqliteStore::read_backup_primary_workspace(&backup).map_err(internal)?;
    let final_locator = canonical_future_path(&target)?
        .as_os_str()
        .as_encoded_bytes()
        .to_vec();
    let result = (|| -> Result<(), RuntimeError> {
        required_os(
            &GitCli::new("git", OsProcess),
            cwd,
            [
                OsString::from("clone"),
                OsString::from("--no-local"),
                cwd.as_os_str().to_owned(),
                staging_path.as_os_str().to_owned(),
            ],
        )?;
        let target_git = GitCli::new("git", OsProcess);
        let discovery = target_git.discover(&staging_path).map_err(git_error)?;
        let bundle_path = discovery.common_dir.join("jjk-load.bundle");
        atomic_bytes(&bundle_path, &object_bundle)?;
        required_os(
            &target_git,
            &staging_path,
            [
                OsString::from("bundle"),
                OsString::from("verify"),
                bundle_path.as_os_str().to_owned(),
            ],
        )?;
        required_os(
            &target_git,
            &staging_path,
            [
                OsString::from("bundle"),
                OsString::from("unbundle"),
                bundle_path.as_os_str().to_owned(),
            ],
        )?;
        fs::remove_file(&bundle_path).map_err(internal)?;
        let database = SqliteStore::database_path(&discovery.common_dir);
        if let Some(parent) = database.parent() {
            fs::create_dir_all(parent).map_err(internal)?;
        }
        fs::copy(&backup, &database).map_err(internal)?;
        SqliteStore::rebind_backup_file(&database, &final_token).map_err(internal)?;
        SqliteStore::rebind_primary_workspace(&database, restored_workspace, &final_locator)
            .map_err(internal)?;
        let mut restored_snapshot = snapshot.clone();
        restored_snapshot
            .refs
            .retain(|reference| reference.name != b"refs/remotes/origin/HEAD");
        restore_runtime_git_snapshot(&target_git, &staging_path, &restored_snapshot, None)?;
        let expected_index = restored_snapshot.index.clone();
        // Index bytes are compared semantically below; everything else must reproduce.
        let staged_index_path = absolute_git_path(
            &staging_path,
            observation_required(
                &target_git,
                &staging_path,
                ["rev-parse", "--git-path", "index"],
            )?,
        );
        restored_snapshot.index = fs::read(&staged_index_path).unwrap_or_default();
        let control_reproduced =
            snapshot_matches_live(&target_git, &staging_path, &restored_snapshot, None)?;
        let expected_index_path = discovery.common_dir.join("jjk-load-verify.index");
        atomic_bytes(&expected_index_path, &expected_index)?;
        let mut expected_index_env = BTreeMap::new();
        expected_index_env.insert(
            OsString::from("GIT_INDEX_FILE"),
            Some(expected_index_path.as_os_str().to_owned()),
        );
        let expected_index_tree = required_output(
            target_git
                .run_with_env(&staging_path, ["write-tree"], expected_index_env)
                .map_err(git_error)?,
        )?;
        fs::remove_file(&expected_index_path).map_err(internal)?;
        let observed_index_tree = observation_required(&target_git, &staging_path, ["write-tree"])?;
        let index_semantically_equal = expected_index_tree == observed_index_tree;
        if !control_reproduced || !index_semantically_equal {
            return Err(RuntimeError::Internal(
                "staged load verification did not reproduce the backup Git control surface".into(),
            ));
        }
        Ok(())
    })();
    if let Err(error) = result {
        drop(staging);
        return Err(error);
    }
    reservation.publish_directory(staging).map_err(internal)?;
    #[derive(Serialize)]
    struct LoadResult {
        command: &'static str,
        action: &'static str,
        source: String,
        target: String,
        journal_events: u64,
        journal_head: String,
    }
    emit(
        format,
        &LoadResult {
            command: "load",
            action: "restored",
            source: backup.display().to_string(),
            target: target.display().to_string(),
            journal_events: info.head.local_seq,
            journal_head: hex::encode(info.head.event_hash),
        },
    )?;
    Ok(0)
}

fn create_runtime_git_bundle(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    control_root: &Path,
) -> Result<Vec<u8>, RuntimeError> {
    let path = control_root.join(format!("backup-objects-{}.bundle", Uuid::now_v7().simple()));
    let retention_ref = format!("refs/jjk/backup/{}", Uuid::now_v7().simple());
    let result = (|| {
        let tree = required_os(git, cwd, [OsString::from("write-tree")])?;
        let commit = required_os(
            git,
            cwd,
            [
                OsString::from("-c"),
                OsString::from("user.name=jjk backup"),
                OsString::from("-c"),
                OsString::from("user.email=backup@jjk.invalid"),
                OsString::from("commit-tree"),
                OsString::from(&tree),
                OsString::from("-m"),
                OsString::from("jjk backup index retention"),
            ],
        )?;
        required_os(
            git,
            cwd,
            [
                OsString::from("update-ref"),
                OsString::from(&retention_ref),
                OsString::from(&commit),
            ],
        )?;
        let bundled = required_os(
            git,
            cwd,
            [
                OsString::from("bundle"),
                OsString::from("create"),
                path.as_os_str().to_owned(),
                OsString::from("--all"),
            ],
        );
        let cleanup = required_os(
            git,
            cwd,
            [
                OsString::from("update-ref"),
                OsString::from("-d"),
                OsString::from(&retention_ref),
            ],
        );
        bundled?;
        cleanup?;
        let bytes = fs::read(&path).map_err(internal)?;
        if bytes.is_empty() {
            return Err(RuntimeError::Internal(
                "Git created an empty backup object bundle".into(),
            ));
        }
        Ok(bytes)
    })();
    let _ = required_os(
        git,
        cwd,
        [
            OsString::from("update-ref"),
            OsString::from("-d"),
            OsString::from(&retention_ref),
        ],
    );
    let _ = fs::remove_file(&path);
    result
}

fn extract_format(args: &mut &[OsString]) -> Result<Format, RuntimeError> {
    let mut format = Format::Human(default_width());
    while args
        .last()
        .is_some_and(|value| value == OsStr::new("--json") || value == OsStr::new("--format=json"))
    {
        format = Format::Json;
        *args = &args[..args.len() - 1];
    }
    Ok(format)
}

fn backup_stamp() -> String {
    format!("backup-{}", Uuid::now_v7().simple())
}

fn doctor(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let context = context(cwd)?;
    context.store.integrity_check().map_err(internal)?;
    let head = context.store.head().map_err(internal)?;
    let pending_operations = pending_operations(&context.store)?;
    let recovery_required = !pending_operations.is_empty();
    let hard_recovery = pending_operations
        .iter()
        .any(|operation| operation.phase != "awaiting_resolution");
    let discovery = context.git.discover(cwd).map_err(git_error)?;
    let jj = crate::adapters::jj::JjCli::new("jj", OsProcess)
        .capability_report(cwd, Some(&discovery.common_dir));
    #[derive(Serialize)]
    struct Result {
        command: &'static str,
        healthy: bool,
        recovery_required: bool,
        pending_operations: Vec<PendingOperationView>,
        journal_events: u64,
        journal_head: String,
        jj: crate::adapters::jj::JjCapabilityReport,
    }
    emit(
        format,
        &Result {
            command: "doctor",
            healthy: !hard_recovery,
            recovery_required,
            pending_operations,
            journal_events: head.local_seq,
            journal_head: hex::encode(head.event_hash),
            jj,
        },
    )?;
    Ok(if hard_recovery { 5 } else { 0 })
}

fn status(args: &[OsString], cwd: &Path) -> Result<i32, RuntimeError> {
    let format = presentation(args)?;
    let git = GitCli::new("git", OsProcess);
    let discovery = git.discover(cwd).map_err(git_error)?;
    let initialized = discovery.common_dir.join("jjk/state.sqlite3").exists();
    let (orientation, pending_operations) = if initialized {
        let context = context(cwd)?;
        let orientation = context.store.current_state_row(context.workspace_id).map_err(internal)?.map(|state| serde_json::json!({
            "state_id": display_state_id(&state.state_id).unwrap(),
            "attempt_id": display_attempt_id(&state.attempt_id).unwrap(),
            "workspace_id": crate::domain::WorkspaceId::from_uuid(context.workspace_id).unwrap().to_string(),
            "label": state.label,
        }));
        let pending = pending_operations(&context.store)?;
        (orientation, pending)
    } else {
        (None, Vec::new())
    };
    let (porcelain_v2, branches, jj) =
        std::thread::scope(|scope| -> std::result::Result<_, RuntimeError> {
            let status = scope.spawn(|| {
                if discovery.is_bare {
                    Ok(String::new())
                } else {
                    observation_required(&git, cwd, ["status", "--porcelain=v2", "--branch"])
                }
            });
            let refs = scope.spawn(|| {
                observation_required(
                    &git,
                    cwd,
                    ["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
                )
            });
            let jj = scope.spawn(|| {
                crate::adapters::jj::JjCli::new("jj", OsProcess)
                    .capability_report(cwd, Some(&discovery.common_dir))
            });
            let porcelain_v2 = status
                .join()
                .map_err(|_| RuntimeError::Internal("status probe panicked".into()))??;
            let branches = refs
                .join()
                .map_err(|_| RuntimeError::Internal("branch probe panicked".into()))??
                .lines()
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let jj = jj
                .join()
                .map_err(|_| RuntimeError::Internal("JJ capability probe panicked".into()))?;
            Ok((porcelain_v2, branches, jj))
        })?;
    #[derive(Serialize)]
    struct Result {
        command: &'static str,
        initialized: bool,
        current_state: Option<String>,
        porcelain_v2: String,
        branches: Vec<String>,
        orientation: Option<serde_json::Value>,
        recovery_required: bool,
        pending_operations: Vec<PendingOperationView>,
        jj: crate::adapters::jj::JjCapabilityReport,
    }
    let current_state = orientation
        .as_ref()
        .and_then(|value| value["state_id"].as_str())
        .map(str::to_owned);
    let recovery_required = !pending_operations.is_empty();
    let hard_recovery = pending_operations
        .iter()
        .any(|operation| operation.phase != "awaiting_resolution");
    emit(
        format,
        &Result {
            command: "status",
            initialized,
            current_state,
            branches,
            porcelain_v2,
            orientation,
            recovery_required,
            pending_operations,
            jj,
        },
    )?;
    Ok(if hard_recovery { 5 } else { 0 })
}

struct RuntimeContext {
    git: GitCli<OsProcess>,
    store: SqliteStore,
    workspace_id: Uuid,
    relative_locator: Vec<u8>,
    root_token: Vec<u8>,
    object_format: String,
}

fn mutation_request(
    context: &RuntimeContext,
    cwd: &Path,
    command: &str,
    request: serde_json::Value,
    effects: serde_json::Value,
    recovery: Option<serde_json::Value>,
) -> Result<(RuntimeMutationRequest, PathBuf), RuntimeError> {
    let discovery = context.git.discover(cwd).map_err(git_error)?;
    Ok((
        RuntimeMutationRequest {
            operation_id: Uuid::now_v7(),
            repo_id: context.store.repository_uuid().map_err(internal)?,
            actor_id: Uuid::now_v7(),
            actor_kind: ActorKind::Human,
            command_kind: command.to_owned(),
            recorded_at_utc: now_utc()?,
            repository_fingerprint: repository_fingerprint(
                &context.git,
                cwd,
                context.store.repository_uuid().map_err(internal)?,
                &context.root_token,
            )?,
            request: serde_json::to_vec(&request).map_err(internal)?,
            expected_effects: serde_json::to_vec(&effects).map_err(internal)?,
            recovery_artifact: recovery
                .map(|value| serde_json::to_vec(&value).map_err(internal))
                .transpose()?,
            provenance: b"runtime-v1".to_vec(),
            lock_timeout: std::time::Duration::from_secs(5),
        },
        discovery.common_dir,
    ))
}

fn runtime_fact(event_type: &str, payload: Vec<u8>) -> RuntimeFact {
    RuntimeFact {
        event_type: event_type.to_owned(),
        payload,
        provenance: b"runtime-v1".to_vec(),
        evidence_manifest: Vec::new(),
        dedup_key: None,
    }
}

fn transaction_error<
    Store: std::fmt::Display,
    Effect: std::fmt::Display,
    Verify: std::fmt::Display,
    Commit: std::fmt::Display,
>(
    error: CoordinationError<crate::adapters::os::lock::LockError, Store, Effect, Verify, Commit>,
) -> RuntimeError {
    match error {
        CoordinationError::Lock(error) => RuntimeError::Unavailable(error.to_string()),
        CoordinationError::Store(error) => RuntimeError::Internal(error.to_string()),
        CoordinationError::Fault {
            source: crate::app::transaction::TransactionFault::Configuration(message),
            ..
        } => RuntimeError::InvalidArguments(message),
        CoordinationError::Fault {
            source,
            operation: None,
        } => RuntimeError::Unavailable(source.to_string()),
        CoordinationError::Fault {
            source,
            operation: Some(operation),
        } => match operation.status {
            crate::ports::operation::OperationStatus::Aborted => RuntimeError::Unavailable(
                format!("operation {} aborted: {source}", operation.operation_id),
            ),
            crate::ports::operation::OperationStatus::Committed => {
                RuntimeError::Unavailable(format!(
                    "operation {} committed but result delivery is indeterminate: {source}",
                    operation.operation_id
                ))
            }
            _ => RuntimeError::Unavailable(format!(
                "operation {} requires repair: {source}",
                operation.operation_id
            )),
        },
        CoordinationError::EffectAborted { source, .. }
        | CoordinationError::ConflictPaused { source, .. } => {
            RuntimeError::Unavailable(source.to_string())
        }
        CoordinationError::EffectRepairRequired { source, operation } => {
            RuntimeError::Unavailable(format!(
                "operation {} requires repair: {source}",
                operation.operation_id
            ))
        }
        CoordinationError::Verification { source, operation } => {
            RuntimeError::Unavailable(format!(
                "operation {} requires repair: {source}",
                operation.operation_id
            ))
        }
        CoordinationError::VerificationFailed(operation)
        | CoordinationError::RecoveryRequired(operation) => RuntimeError::Unavailable(format!(
            "operation {} requires repair",
            operation.operation_id
        )),
        CoordinationError::CommitData { source, operation } => RuntimeError::Unavailable(format!(
            "operation {} requires repair: {source}",
            operation.operation_id
        )),
        CoordinationError::Indeterminate { operation, source } => {
            RuntimeError::Unavailable(format!(
                "operation {} is indeterminate and requires repair: {source}",
                operation.operation_id
            ))
        }
    }
}

fn fact_commit(
    event_type: &str,
    payload: Vec<u8>,
    projection: RuntimeProjection,
    result: Vec<u8>,
) -> RuntimeMutationCommit<RuntimeProjection> {
    RuntimeMutationCommit {
        facts: vec![runtime_fact(event_type, payload)],
        projections: vec![projection],
        result,
    }
}

fn workspace_locator(worktree_root: &Path, common_dir: &Path) -> Vec<u8> {
    let locator = worktree_root
        .components()
        .skip(common_dir.components().count())
        .collect::<PathBuf>();
    #[cfg(windows)]
    let locator = locator.to_string_lossy().replace('\\', "/").to_lowercase();
    #[cfg(windows)]
    return locator.into_bytes();
    #[cfg(not(windows))]
    locator.as_os_str().as_encoded_bytes().to_vec()
}

fn context(cwd: &Path) -> Result<RuntimeContext, RuntimeError> {
    let git = GitCli::new("git", OsProcess);
    let discovery = git.discover(cwd).map_err(git_error)?;
    let root_token = repository_root_token(&discovery.common_dir)?;
    let store = SqliteStore::open_existing(
        &discovery.common_dir,
        &root_token,
        StoreOpenOptions::default(),
    )
    .map_err(internal)?;
    let relative_locator = workspace_locator(
        discovery.worktree_root.as_deref().unwrap_or(cwd),
        &discovery.common_dir,
    );
    let mut workspace_seed = b"jjk-workspace-v1\0".to_vec();
    workspace_seed.extend(&root_token);
    workspace_seed.push(0);
    workspace_seed.extend(&relative_locator);
    let derived_workspace_id = deterministic_v7_uuid(&workspace_seed);
    let locator_workspace = store
        .workspace_id_for_locator(&relative_locator)
        .map_err(internal)?;
    let head_workspace = observation_optional(&git, cwd, ["rev-parse", "--verify", "HEAD"])?
        .map(|head| store.workspace_id_for_head(&head).map_err(internal))
        .transpose()?
        .flatten();
    let workspace_id = if let Some(id) = locator_workspace.or(head_workspace) {
        id
    } else if store
        .current_state_row(derived_workspace_id)
        .map_err(internal)?
        .is_some()
    {
        derived_workspace_id
    } else {
        store
            .sole_workspace_id()
            .map_err(internal)?
            .unwrap_or(derived_workspace_id)
    };
    Ok(RuntimeContext {
        git,
        store,
        workspace_id,
        relative_locator,
        root_token,
        object_format: match discovery.object_format {
            crate::ports::repository::ObjectFormat::Sha1 => "sha1".into(),
            crate::ports::repository::ObjectFormat::Sha256 => "sha256".into(),
            crate::ports::repository::ObjectFormat::Other(value) => {
                value.to_string_lossy().into_owned()
            }
        },
    })
}

#[derive(Serialize)]
struct StateView {
    command: String,
    state_id: String,
    commit: String,
    kind: String,
    label: String,
    message: String,
    attempt_id: String,
    logical_parent: Option<String>,
    created_seq: u64,
    archived: bool,
    starred: bool,
}

pub(crate) fn capture_runtime_git_snapshot(
    git: &GitCli<OsProcess>,
    cwd: &Path,
) -> Result<RuntimeGitSnapshot, RuntimeError> {
    capture_runtime_git_snapshot_with(git, cwd, false)
}

/// Every ref with its target and symbolic binding, sorted by name.
pub(crate) fn capture_git_refs(
    git: &GitCli<OsProcess>,
    cwd: &Path,
) -> Result<Vec<RuntimeGitRef>, RuntimeError> {
    let refs_raw = git
        .required(
            cwd,
            [
                "for-each-ref",
                "--sort=refname",
                "--format=%(refname)%00%(objectname)%00%(symref)%00",
            ],
        )
        .map_err(git_error)?;
    let mut refs = Vec::new();
    for line in refs_raw
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let fields = line.split(|byte| *byte == 0).collect::<Vec<_>>();
        if fields.len() < 3 {
            return Err(RuntimeError::Internal(
                "Git emitted malformed reference snapshot".into(),
            ));
        }
        refs.push(RuntimeGitRef {
            name: fields[0].to_vec(),
            target: fields[1].to_vec(),
            symbolic: (!fields[2].is_empty()).then(|| fields[2].to_vec()),
        });
    }
    Ok(refs)
}

fn capture_runtime_git_snapshot_with(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    inline: bool,
) -> Result<RuntimeGitSnapshot, RuntimeError> {
    let refs = capture_git_refs(git, cwd)?;
    let symbolic = git
        .run(
            cwd,
            observation_args([
                OsString::from("symbolic-ref"),
                OsString::from("-q"),
                OsString::from("HEAD"),
            ]),
        )
        .map_err(git_error)?;
    let head_symbolic = (symbolic.exit_code == 0).then(|| trim_eol(symbolic.stdout));
    let head_oid =
        observation_optional(git, cwd, ["rev-parse", "--verify", "HEAD"])?.map(String::into_bytes);
    let index_path = repo_facts(git, cwd)?.index_path;
    let index = match fs::read(&index_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(internal(error)),
    };
    if inline {
        let worktree = capture_git_visible_entries(git, cwd)?;
        return Ok(RuntimeGitSnapshot {
            refs,
            head_symbolic,
            head_oid,
            index,
            worktree,
            tree: None,
            index_blob: None,
        });
    }
    let objects = snapshot_object_env(git, cwd)?;
    let index_blob = if index.is_empty() {
        None
    } else {
        Some(hash_object(git, cwd, &objects, &index_path, true)?)
    };
    let tree = git_visible_tree(git, cwd, &objects)?;
    Ok(RuntimeGitSnapshot {
        refs,
        head_symbolic,
        head_oid,
        index: Vec::new(),
        worktree: Vec::new(),
        tree: Some(tree),
        index_blob,
    })
}

/// Snapshot whose worktree content and index bytes are carried inline. Used for backups,
/// which must stay portable to repositories without the private object directory.
pub(crate) fn capture_runtime_git_snapshot_inline(
    git: &GitCli<OsProcess>,
    cwd: &Path,
) -> Result<RuntimeGitSnapshot, RuntimeError> {
    capture_runtime_git_snapshot_with(git, cwd, true)
}

/// Environment that routes object writes to the JJK-private object directory under the Git
/// common directory while reading through the repository's own objects. Git maintenance never
/// visits this directory, so snapshot objects survive `git gc` without retention refs, and
/// no user-visible ref or object changes for a snapshot.
/// Repository locations resolved once per process for a given cwd. A command is one short
/// process and never relocates the repository, so re-asking Git for these on every helper
/// call only costs subprocess spawns.
#[derive(Clone, Debug)]
struct RepoFacts {
    common_dir: PathBuf,
    index_path: PathBuf,
    objects_dir: PathBuf,
    toplevel: Option<PathBuf>,
}

fn repo_facts(git: &GitCli<OsProcess>, cwd: &Path) -> Result<RepoFacts, RuntimeError> {
    static CACHE: std::sync::Mutex<Option<(PathBuf, RepoFacts)>> = std::sync::Mutex::new(None);
    {
        let cache = CACHE.lock().map_err(|_| internal("poisoned cache"))?;
        if let Some((cached_cwd, facts)) = cache.as_ref() {
            if cached_cwd == cwd {
                return Ok(facts.clone());
            }
        }
    }
    let lines = observation_required(
        git,
        cwd,
        [
            "rev-parse",
            "--git-common-dir",
            "--absolute-git-dir",
            "--git-path",
            "index",
            "--git-path",
            "objects",
        ],
    )?;
    let mut lines = lines.lines().map(|line| line.to_owned());
    let common_dir = absolute_git_path(cwd, lines.next().unwrap_or_default());
    let git_dir = absolute_git_path(cwd, lines.next().unwrap_or_default());
    let index_path = lines.next().filter(|value| !value.is_empty()).map_or_else(
        || git_dir.join("index"),
        |value| absolute_git_path(cwd, value),
    );
    let objects_dir = lines.next().filter(|value| !value.is_empty()).map_or_else(
        || common_dir.join("objects"),
        |value| absolute_git_path(cwd, value),
    );
    let toplevel = observation_optional(git, cwd, ["rev-parse", "--show-toplevel"])?
        .filter(|root| !root.is_empty())
        .map(PathBuf::from);
    let facts = RepoFacts {
        common_dir,
        index_path,
        objects_dir,
        toplevel,
    };
    *CACHE.lock().map_err(|_| internal("poisoned cache"))? =
        Some((cwd.to_path_buf(), facts.clone()));
    Ok(facts)
}

fn snapshot_object_env(
    git: &GitCli<OsProcess>,
    cwd: &Path,
) -> Result<BTreeMap<OsString, Option<OsString>>, RuntimeError> {
    let facts = repo_facts(git, cwd)?;
    let private_objects = facts.common_dir.join("jjk").join("objects");
    if !private_objects.join("pack").is_dir() {
        fs::create_dir_all(private_objects.join("info")).map_err(internal)?;
        fs::create_dir_all(private_objects.join("pack")).map_err(internal)?;
    }
    let mut env = BTreeMap::new();
    env.insert(
        OsString::from("GIT_OBJECT_DIRECTORY"),
        Some(private_objects.into_os_string()),
    );
    env.insert(
        OsString::from("GIT_ALTERNATE_OBJECT_DIRECTORIES"),
        Some(facts.objects_dir.into_os_string()),
    );
    Ok(env)
}

fn absolute_git_path(cwd: &Path, value: String) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

/// Private scratch index path under the control directory.
fn scratch_index_path(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    label: &str,
) -> Result<PathBuf, RuntimeError> {
    let scratch = repo_facts(git, cwd)?.common_dir.join("jjk").join("tmp");
    fs::create_dir_all(&scratch).map_err(internal)?;
    Ok(scratch.join(format!("{label}-{}.index", Uuid::now_v7().simple())))
}

fn with_index(
    index: &Path,
    mut env: BTreeMap<OsString, Option<OsString>>,
) -> BTreeMap<OsString, Option<OsString>> {
    env.insert(
        OsString::from("GIT_INDEX_FILE"),
        Some(index.as_os_str().to_owned()),
    );
    env
}

/// Blob ID of the file at `path`; with `write`, the blob is stored in the private object
/// directory.
fn hash_object(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    objects: &BTreeMap<OsString, Option<OsString>>,
    path: &Path,
    write: bool,
) -> Result<String, RuntimeError> {
    let mut args = vec![OsString::from("hash-object")];
    if write {
        args.push(OsString::from("-w"));
    }
    args.push(OsString::from("--"));
    args.push(path.as_os_str().to_owned());
    required_output(
        git.run_with_env(cwd, observation_args(args), objects.clone())
            .map_err(git_error)?,
    )
}

/// Stages every Git-visible path of the checkout into the index selected by `env`, exactly as
/// `git add -A` would, except that untracked nested repositories are left alone: Git refuses
/// to add an embedded checkout without a commit and would otherwise record one as a gitlink,
/// and neither belongs to a JJK capture. Gitlinks already in the index keep Git's semantics.
/// Pathspecs are top-anchored, so `cwd` may be any directory of the checkout.
fn add_all_excluding_nested(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    env: BTreeMap<OsString, Option<OsString>>,
) -> Result<(), RuntimeError> {
    let root = worktree_root(git, cwd)?.unwrap_or_else(|| cwd.to_path_buf());
    let listing = git
        .required(
            &root,
            [
                "ls-files",
                "-z",
                "--full-name",
                "--others",
                "--exclude-standard",
            ],
        )
        .map_err(git_error)?;
    let mut pathspecs = b":/\0".to_vec();
    let mut excluded = 0usize;
    // `ls-files --others` lists a nested repository as `<dir>/` instead of descending into it.
    for nested in listing
        .split(|byte| *byte == 0)
        .filter(|line| line.ends_with(b"/"))
    {
        pathspecs.extend_from_slice(b":(top,exclude,literal)");
        pathspecs.extend_from_slice(&nested[..nested.len() - 1]);
        pathspecs.push(0);
        excluded += 1;
    }
    let mut args = vec![OsString::from("add"), OsString::from("-A")];
    let pathspec_file = if excluded == 0 {
        None
    } else {
        let file = scratch_index_path(git, cwd, "pathspec")?;
        fs::write(&file, &pathspecs).map_err(internal)?;
        let mut flag = OsString::from("--pathspec-from-file=");
        flag.push(file.as_os_str());
        args.push(flag);
        args.push(OsString::from("--pathspec-file-nul"));
        Some(file)
    };
    let result = required_output(
        git.run_with_env(cwd, observation_args(args), env)
            .map_err(git_error)?,
    );
    if let Some(file) = pathspec_file {
        let _ = fs::remove_file(file);
    }
    result.map(|_| ())
}

/// Tree of every Git-visible path (index entries plus untracked, non-ignored files), built in
/// a private index so the user's index is untouched; blobs land in the private object
/// directory, deduplicated against the repository's own objects.
fn git_visible_tree(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    objects: &BTreeMap<OsString, Option<OsString>>,
) -> Result<String, RuntimeError> {
    let root = worktree_root(git, cwd)?.unwrap_or_else(|| cwd.to_path_buf());
    let index = scratch_index_path(git, cwd, "snapshot")?;
    let env = with_index(&index, objects.clone());
    let result = (|| -> Result<String, RuntimeError> {
        add_all_excluding_nested(git, &root, env.clone())?;
        required_output(
            git.run_with_env(&root, observation_args([OsString::from("write-tree")]), env)
                .map_err(git_error)?,
        )
    })();
    let _ = fs::remove_file(&index);
    result
}

/// Raw index bytes a snapshot restores: inline bytes, or the private-object blob.
fn snapshot_index_bytes(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    snapshot: &RuntimeGitSnapshot,
) -> Result<Vec<u8>, RuntimeError> {
    let Some(blob) = &snapshot.index_blob else {
        return Ok(snapshot.index.clone());
    };
    let objects = snapshot_object_env(git, cwd)?;
    let output = git
        .run_with_env(
            cwd,
            observation_args([
                OsString::from("cat-file"),
                OsString::from("blob"),
                OsString::from(blob),
            ]),
            objects,
        )
        .map_err(git_error)?;
    if output.exit_code != 0 {
        return Err(RuntimeError::Unavailable(format!(
            "snapshot index blob {blob} is missing from the private object directory"
        )));
    }
    Ok(output.stdout)
}

/// Writes every path of `tree` into the worktree at `root` (bytes, modes, symlinks) through a
/// private index; the user's index is untouched.
fn materialize_tree(git: &GitCli<OsProcess>, root: &Path, tree: &str) -> Result<(), RuntimeError> {
    let objects = snapshot_object_env(git, root)?;
    let index = scratch_index_path(git, root, "restore")?;
    let env = with_index(&index, objects);
    let result = (|| -> Result<(), RuntimeError> {
        required_output(
            git.run_with_env(
                root,
                observation_args([OsString::from("read-tree"), OsString::from(tree)]),
                env.clone(),
            )
            .map_err(git_error)?,
        )?;
        required_output(
            git.run_with_env(
                root,
                observation_args([
                    OsString::from("checkout-index"),
                    OsString::from("-a"),
                    OsString::from("-f"),
                ]),
                env,
            )
            .map_err(git_error)?,
        )?;
        Ok(())
    })();
    let _ = fs::remove_file(&index);
    result
}

/// Worktree root of the checkout that `cwd` belongs to, when one exists.
fn worktree_root(git: &GitCli<OsProcess>, cwd: &Path) -> Result<Option<PathBuf>, RuntimeError> {
    Ok(repo_facts(git, cwd)?.toplevel)
}

/// Paths Git itself considers part of the checkout: index entries plus untracked files that
/// are not ignored. Ignored content (`target/`, `node_modules/`, `.worktrees/`, …) and nested
/// repositories are never part of a JJK snapshot, so they are never stored, restored, or
/// deleted. Paths are repository-relative bytes with `/` separators, sorted, deduplicated.
fn git_visible_paths(git: &GitCli<OsProcess>, root: &Path) -> Result<Vec<Vec<u8>>, RuntimeError> {
    let raw = git
        .required(
            root,
            [
                "ls-files",
                "-z",
                "--full-name",
                "--stage",
                "--others",
                "--exclude-standard",
            ],
        )
        .map_err(git_error)?;
    // Index entries print as `<mode> <oid> <stage>\t<path>`; untracked paths print bare.
    // Gitlinks (mode 160000) are directories owned by another repository.
    let mut paths = raw
        .split(|byte| *byte == 0)
        .filter(|line| !line.is_empty())
        .filter_map(|line| match line.iter().position(|byte| *byte == b'\t') {
            Some(tab) if line.starts_with(b"160000 ") => {
                let _ = tab;
                None
            }
            Some(tab) => Some(line[tab + 1..].to_vec()),
            None if line.ends_with(b"/") => None,
            None => Some(line.to_vec()),
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Snapshot entry for one repository-relative path, or `None` when the path is absent or a
/// directory (a nested repository or gitlink).
fn worktree_entry_at(
    root: &Path,
    relative: &[u8],
) -> Result<Option<RuntimeWorktreeEntry>, RuntimeError> {
    let path = root.join(os_string(relative)?);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(internal(error)),
    };
    if metadata.is_dir() {
        return Ok(None);
    }
    if metadata.file_type().is_symlink() {
        return Ok(Some(RuntimeWorktreeEntry::Symlink {
            path: relative.to_vec(),
            target: fs::read_link(&path)
                .map_err(internal)?
                .as_os_str()
                .as_encoded_bytes()
                .to_vec(),
        }));
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let mode = u32::from(metadata.permissions().readonly());
    Ok(Some(RuntimeWorktreeEntry::Regular {
        path: relative.to_vec(),
        mode,
        bytes: fs::read(path).map_err(internal)?,
    }))
}

fn capture_git_visible_entries(
    git: &GitCli<OsProcess>,
    cwd: &Path,
) -> Result<Vec<RuntimeWorktreeEntry>, RuntimeError> {
    let Some(root) = worktree_root(git, cwd)? else {
        return Ok(Vec::new());
    };
    let mut entries = Vec::new();
    for relative in git_visible_paths(git, &root)? {
        if let Some(entry) = worktree_entry_at(&root, &relative)? {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// Repository-relative paths of every blob and symlink in `tree` (a commit or tree object);
/// gitlinks are excluded.
fn tree_paths(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    tree: &str,
) -> Result<std::collections::BTreeSet<Vec<u8>>, RuntimeError> {
    let objects = snapshot_object_env(git, cwd)?;
    let output = git
        .run_with_env(
            cwd,
            observation_args(["ls-tree", "-r", "-z", "--full-tree", tree].map(OsString::from)),
            objects,
        )
        .map_err(git_error)?;
    if output.exit_code != 0 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    // Entries print as `<mode> <type> <oid>\t<path>`; only blobs (files and symlinks) are
    // JJK's to write or delete — commit entries are nested repositories.
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|line| {
            let tab = line.iter().position(|byte| *byte == b'\t')?;
            let (meta, path) = line.split_at(tab);
            meta.split(|byte| *byte == b' ')
                .nth(1)
                .filter(|kind| *kind == b"blob")
                .map(|_| path[1..].to_vec())
        })
        .collect())
}

fn entry_path(entry: &RuntimeWorktreeEntry) -> &[u8] {
    match entry {
        RuntimeWorktreeEntry::Regular { path, .. } | RuntimeWorktreeEntry::Symlink { path, .. } => {
            path
        }
    }
}

/// Whether the live checkout reproduces `expected`: refs, HEAD, and index bytes are equal and
/// every expected worktree entry is present byte-for-byte. Git-visible paths that are absent
/// from `expected` are tolerated as uncaptured extras unless they belong to `known_tree` —
/// the tree JJK just moved away from — in which case the restore failed to remove them.
pub(crate) fn snapshot_matches_live(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    expected: &RuntimeGitSnapshot,
    known_tree: Option<&str>,
) -> Result<bool, RuntimeError> {
    snapshot_matches_live_with(git, cwd, expected, known_tree).map(|(matches, _)| matches)
}

/// Like [`snapshot_matches_live`], also returning the live snapshot it captured so callers
/// that need the post-effect snapshot do not capture it twice.
pub(crate) fn snapshot_matches_live_with(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    expected: &RuntimeGitSnapshot,
    known_tree: Option<&str>,
) -> Result<(bool, RuntimeGitSnapshot), RuntimeError> {
    let live = capture_runtime_git_snapshot(git, cwd)?;
    let matches = snapshot_matches_captured(git, cwd, expected, known_tree, &live)?;
    Ok((matches, live))
}

/// Whether the live index carries exactly what `expected` captured. The raw file is compared
/// first (bytes, or the content-addressed blob); when it differs, the entries are compared —
/// Git rewrites the index file for stat-cache refreshes (`git status`, `git diff`, editors),
/// which changes bytes without changing a single staged path, mode, or object.
fn index_matches_snapshot(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    expected: &RuntimeGitSnapshot,
) -> Result<bool, RuntimeError> {
    let index_path = repo_facts(git, cwd)?.index_path;
    let raw_matches = match &expected.index_blob {
        Some(blob) => {
            index_path.is_file()
                && hash_object(
                    git,
                    cwd,
                    &snapshot_object_env(git, cwd)?,
                    &index_path,
                    false,
                )? == *blob
        }
        None => {
            let live_index = match fs::read(&index_path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(internal(error)),
            };
            live_index == expected.index
        }
    };
    if raw_matches {
        return Ok(true);
    }
    let expected_bytes = snapshot_index_bytes(git, cwd, expected)?;
    let live_entries = if index_path.is_file() {
        index_entries(git, cwd, None)?
    } else {
        Vec::new()
    };
    let expected_entries = if expected_bytes.is_empty() {
        Vec::new()
    } else {
        let scratch = scratch_index_path(git, cwd, "compare")?;
        fs::write(&scratch, &expected_bytes).map_err(internal)?;
        let entries = index_entries(git, cwd, Some(&scratch));
        let _ = fs::remove_file(&scratch);
        entries?
    };
    Ok(live_entries == expected_entries)
}

/// `<mode> <oid> <stage>\t<path>` lines of an index — the user's, or the file at `index`.
fn index_entries(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    index: Option<&Path>,
) -> Result<Vec<u8>, RuntimeError> {
    let env = match index {
        Some(index) => with_index(index, BTreeMap::new()),
        None => BTreeMap::new(),
    };
    required_output_bytes(
        git.run_with_env(
            cwd,
            observation_args(["ls-files", "-z", "--full-name", "--stage"].map(OsString::from)),
            env,
        )
        .map_err(git_error)?,
    )
}

fn snapshot_matches_captured(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    expected: &RuntimeGitSnapshot,
    known_tree: Option<&str>,
    live: &RuntimeGitSnapshot,
) -> Result<bool, RuntimeError> {
    if live.refs != expected.refs
        || live.head_symbolic != expected.head_symbolic
        || live.head_oid != expected.head_oid
    {
        return Ok(false);
    }
    if !index_matches_snapshot(git, cwd, expected)? {
        return Ok(false);
    }
    let root = worktree_root(git, cwd)?.unwrap_or_else(|| cwd.to_path_buf());
    let expected_paths = match &expected.tree {
        Some(tree) => {
            // Content-addressed: an identical live tree proves the content outright; otherwise
            // the worktree projected onto the tree's paths must still hash to it (extras).
            if live.tree.as_deref() != Some(tree.as_str())
                && state_relative_content_tree(git, cwd, tree)? != *tree
            {
                return Ok(false);
            }
            tree_paths(git, cwd, tree)?
        }
        None => {
            // Inline entries are checked on disk, not through the Git-visible listing:
            // snapshots written before 0.3.0 may carry ignored paths, which a restore still
            // reproduces.
            for entry in &expected.worktree {
                if worktree_entry_at(&root, entry_path(entry))?.as_ref() != Some(entry) {
                    return Ok(false);
                }
            }
            expected
                .worktree
                .iter()
                .map(|entry| entry_path(entry).to_vec())
                .collect()
        }
    };
    let Some(known_tree) = known_tree else {
        return Ok(true);
    };
    let stale = tree_paths(git, cwd, known_tree)?;
    let live_paths = match &live.tree {
        Some(tree) => tree_paths(git, cwd, tree)?,
        None => live
            .worktree
            .iter()
            .map(|entry| entry_path(entry).to_vec())
            .collect(),
    };
    Ok(!live_paths
        .iter()
        .any(|path| !expected_paths.contains(path) && stale.contains(path)))
}

/// Restores refs, HEAD, index bytes, and snapshot worktree entries. Only paths JJK knows are
/// removed from the checkout: paths tracked by the current index, paths owned by `snapshot`,
/// and paths of `known_tree` (the tree being left). Uncaptured files — untracked extras
/// created after a capture, and everything ignored — are never touched.
pub(crate) fn restore_runtime_git_snapshot(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    snapshot: &RuntimeGitSnapshot,
    known_tree: Option<&str>,
) -> Result<(), RuntimeError> {
    let current_refs = capture_git_refs(git, cwd)?;
    let root = worktree_root(git, cwd)?.unwrap_or_else(|| cwd.to_path_buf());
    let mut known = match &snapshot.tree {
        Some(tree) => tree_paths(git, cwd, tree)?,
        None => snapshot
            .worktree
            .iter()
            .map(|entry| entry_path(entry).to_vec())
            .collect::<std::collections::BTreeSet<_>>(),
    };
    known.extend(
        git.required(&root, ["ls-files", "-z", "--full-name", "--cached"])
            .map_err(git_error)?
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(<[u8]>::to_vec),
    );
    if let Some(tree) = known_tree {
        known.extend(tree_paths(git, cwd, tree)?);
    }
    let removable = git_visible_paths(git, &root)?
        .into_iter()
        .filter(|path| known.contains(path))
        .collect::<Vec<_>>();
    let target_names = snapshot
        .refs
        .iter()
        .map(|item| item.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let current_by_name = current_refs
        .iter()
        .map(|item| (item.name.clone(), item))
        .collect::<BTreeMap<_, _>>();
    for reference in current_refs
        .iter()
        .filter(|item| !target_names.contains(&item.name))
    {
        if reference.symbolic.is_some() {
            required_os(
                git,
                cwd,
                [
                    OsString::from("symbolic-ref"),
                    OsString::from("--delete"),
                    os_string(&reference.name)?,
                ],
            )?;
        } else {
            required_os(
                git,
                cwd,
                [
                    OsString::from("update-ref"),
                    OsString::from("-d"),
                    os_string(&reference.name)?,
                ],
            )?;
        }
    }
    for reference in &snapshot.refs {
        if current_by_name.get(&reference.name) == Some(&reference) {
            continue;
        }
        if let Some(symbolic) = &reference.symbolic {
            required_os(
                git,
                cwd,
                [
                    OsString::from("symbolic-ref"),
                    os_string(&reference.name)?,
                    os_string(symbolic)?,
                ],
            )?;
        } else {
            required_os(
                git,
                cwd,
                [
                    OsString::from("update-ref"),
                    os_string(&reference.name)?,
                    os_string(&reference.target)?,
                ],
            )?;
        }
    }
    if let Some(symbolic) = &snapshot.head_symbolic {
        required_os(
            git,
            cwd,
            [
                OsString::from("symbolic-ref"),
                OsString::from("HEAD"),
                os_string(symbolic)?,
            ],
        )?;
    } else if let Some(oid) = &snapshot.head_oid {
        required_os(
            git,
            cwd,
            [
                OsString::from("update-ref"),
                OsString::from("--no-deref"),
                OsString::from("HEAD"),
                os_string(oid)?,
            ],
        )?;
    }
    let invoking_directory = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let root = fs::canonicalize(&root).unwrap_or(root);
    for relative in &removable {
        let path = root.join(os_string(relative)?);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(internal(error)),
        }
        remove_empty_parents(&root, &path, &invoking_directory);
    }
    if let Some(tree) = &snapshot.tree {
        materialize_tree(git, &root, tree)?;
    } else {
        for entry in &snapshot.worktree {
            restore_worktree_entry(&root, entry)?;
        }
    }
    let index_path = repo_facts(git, cwd)?.index_path;
    let index_bytes = snapshot_index_bytes(git, cwd, snapshot)?;
    if index_bytes.is_empty() {
        match fs::remove_file(&index_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(internal(error)),
        }
    } else {
        atomic_bytes(&index_path, &index_bytes)?;
    }
    Ok(())
}

/// Prunes directories left empty by a removal, stopping at `root` and never touching `keep`
/// or its ancestors: `keep` is the invoking directory, and deleting it — even if a restore
/// recreates the path — leaves the user's shell in an unlinked directory. Git has no empty
/// directories, so nothing observable is lost by pruning; an empty directory left behind
/// is harmless.
fn remove_empty_parents(root: &Path, path: &Path, keep: &Path) {
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if directory == root || keep.starts_with(directory) || fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}
fn restore_worktree_entry(root: &Path, entry: &RuntimeWorktreeEntry) -> Result<(), RuntimeError> {
    match entry {
        RuntimeWorktreeEntry::Regular { path, mode, bytes } => {
            #[cfg(not(unix))]
            let _ = mode;
            let target = root.join(os_string(path)?);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(internal)?;
            }
            atomic_bytes(&target, bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(*mode))
                    .map_err(internal)?;
            }
        }
        RuntimeWorktreeEntry::Symlink { path, target } => {
            #[cfg(not(unix))]
            let _ = target;
            let destination = root.join(os_string(path)?);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(internal)?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(os_string(target)?, destination).map_err(internal)?;
            }
            #[cfg(not(unix))]
            {
                return Err(RuntimeError::Unavailable(
                    "symlink snapshot restore is unsupported on this platform".into(),
                ));
            }
        }
    }
    Ok(())
}
fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), RuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(internal)?;
    }
    let temporary = path.with_extension(format!("jjk-restore-{}", Uuid::now_v7().simple()));
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(internal)?;
        file.write_all(bytes).map_err(internal)?;
        file.sync_all().map_err(internal)?;
    }
    fs::rename(temporary, path).map_err(internal)
}
fn trim_eol(mut bytes: Vec<u8>) -> Vec<u8> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    bytes
}
fn os_string(bytes: &[u8]) -> Result<OsString, RuntimeError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes.to_vec())
            .map(OsString::from)
            .map_err(internal)
    }
}
fn ensure_local_git_exclude(common_dir: &Path, pattern: &str) -> Result<(), RuntimeError> {
    let path = common_dir.join("info").join("exclude");
    let mut bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(internal(error)),
    };
    if bytes
        .split(|byte| *byte == b'\n')
        .any(|line| line.strip_suffix(b"\r").unwrap_or(line) == pattern.as_bytes())
    {
        return Ok(());
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(pattern.as_bytes());
    bytes.push(b'\n');
    atomic_bytes(&path, &bytes)
}
fn lexical_repository_root(git: &GitCli<OsProcess>, cwd: &Path) -> Result<PathBuf, RuntimeError> {
    let prefix = observation_required(git, cwd, ["rev-parse", "--show-prefix"])?;
    let depth = Path::new(&prefix)
        .components()
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count();
    let mut root = cwd.to_path_buf();
    for _ in 0..depth {
        root = root
            .parent()
            .ok_or_else(|| {
                RuntimeError::Internal("Git worktree prefix escapes the invocation path".into())
            })?
            .to_path_buf();
    }
    Ok(root)
}

fn state_view(
    command: &str,
    state: RuntimeStateRow,
    starred: bool,
) -> Result<StateView, RuntimeError> {
    Ok(StateView {
        command: command.into(),
        state_id: display_state_id(&state.state_id)?,
        commit: state.git_oid,
        kind: state.kind,
        label: state.label,
        message: state.message,
        attempt_id: display_attempt_id(&state.attempt_id)?,
        logical_parent: state
            .logical_parent
            .as_deref()
            .map(display_state_id)
            .transpose()?,
        created_seq: state.created_seq,
        archived: state.archived,
        starred,
    })
}

fn runtime_event_for_head(
    store: &SqliteStore,
    head: &crate::ports::journal::JournalHead,
    event_type: &str,
    payload: Vec<u8>,
    actor_kind: ActorKind,
    recorded_at_utc: &str,
    repository_fingerprint: &[u8],
) -> Result<EventRecord, RuntimeError> {
    let operation_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let mut hash_input = b"jjk-runtime-event-v1\0".to_vec();
    hash_input.extend(event_id.as_bytes());
    hash_input.extend(store.repository_uuid().map_err(internal)?.as_bytes());
    hash_input.extend(event_type.as_bytes());
    hash_input.extend(operation_id.as_bytes());
    hash_input.extend(&head.event_hash);
    hash_input.extend(&payload);
    let event_hash: [u8; 32] = Sha256::digest(hash_input).into();
    Ok(EventRecord {
        event_id,
        repo_id: store.repository_uuid().map_err(internal)?,
        event_type: event_type.into(),
        event_schema_version: 1,
        envelope_version: crate::adapters::sqlite::ENVELOPE_VERSION,
        operation_id,
        operation_ordinal: 0,
        actor_id: Uuid::now_v7(),
        actor_kind,
        recorded_at_utc: recorded_at_utc.into(),
        observed_at_utc: Some(recorded_at_utc.into()),
        repository_fingerprint: repository_fingerprint.to_vec(),
        payload_codec: PayloadCodec::CanonicalJsonV1,
        payload,
        provenance: b"git-import-v1".to_vec(),
        evidence_manifest: Vec::new(),
        dedup_key: None,
        previous_event_hash: head.event_hash,
        event_hash,
    })
}
fn runtime_event(
    context: &RuntimeContext,
    cwd: &Path,
    event_type: &str,
    payload: Vec<u8>,
) -> Result<EventRecord, RuntimeError> {
    let store = &context.store;
    let repository_fingerprint = repository_fingerprint(
        &context.git,
        cwd,
        store.repository_uuid().map_err(internal)?,
        &context.root_token,
    )?;
    let head = store.head().map_err(internal)?;
    let operation_id = Uuid::now_v7();
    let event_id = Uuid::now_v7();
    let recorded_at_utc = now_utc()?;
    let mut hash_input = b"jjk-runtime-event-v1\0".to_vec();
    hash_input.extend(event_id.as_bytes());
    hash_input.extend(store.repository_uuid().map_err(internal)?.as_bytes());
    hash_input.extend(event_type.as_bytes());
    hash_input.extend(operation_id.as_bytes());
    hash_input.extend(&head.event_hash);
    hash_input.extend(&payload);
    let event_hash: [u8; 32] = Sha256::digest(hash_input).into();
    Ok(EventRecord {
        event_id,
        repo_id: store.repository_uuid().map_err(internal)?,
        event_type: event_type.into(),
        event_schema_version: 1,
        envelope_version: crate::adapters::sqlite::ENVELOPE_VERSION,
        operation_id,
        operation_ordinal: 0,
        actor_id: Uuid::now_v7(),
        actor_kind: ActorKind::Human,
        recorded_at_utc,
        observed_at_utc: None,
        repository_fingerprint,
        payload_codec: PayloadCodec::CanonicalJsonV1,
        payload,
        provenance: b"runtime-v1".to_vec(),
        evidence_manifest: Vec::new(),
        dedup_key: None,
        previous_event_hash: head.event_hash,
        event_hash,
    })
}

fn canonical_future_path(path: &Path) -> Result<PathBuf, RuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| RuntimeError::Internal("future path has no parent".into()))?;
    Ok(fs::canonicalize(parent).map_err(internal)?.join(
        path.file_name()
            .ok_or_else(|| RuntimeError::Internal("future path has no name".into()))?,
    ))
}
fn repository_root_token_for_future(common_dir: &Path) -> Result<Vec<u8>, RuntimeError> {
    let parent = common_dir.parent().ok_or_else(|| {
        RuntimeError::Internal("future Git common directory has no parent".into())
    })?;
    let canonical_parent = fs::canonicalize(parent.parent().unwrap_or(parent)).map_err(internal)?;
    let canonical = if parent.parent().is_some() {
        canonical_parent
            .join(parent.file_name().unwrap_or_default())
            .join(common_dir.file_name().unwrap_or_default())
    } else {
        canonical_parent.join(common_dir.file_name().unwrap_or_default())
    };
    #[cfg(windows)]
    let canonical_bytes = canonical
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
        .into_bytes();
    #[cfg(not(windows))]
    let canonical_bytes = canonical.as_os_str().as_encoded_bytes().to_vec();
    let mut bytes = b"jjk-safe-space-v1\0".to_vec();
    bytes.extend(canonical_bytes);
    Ok(Sha256::digest(bytes).to_vec())
}
fn repository_root_token(common_dir: &Path) -> Result<Vec<u8>, RuntimeError> {
    let canonical = fs::canonicalize(common_dir).map_err(internal)?;
    #[cfg(windows)]
    let canonical_bytes = canonical
        .to_string_lossy()
        .replace('\\', "/")
        .to_lowercase()
        .into_bytes();
    #[cfg(not(windows))]
    let canonical_bytes = canonical.as_os_str().as_encoded_bytes().to_vec();
    let mut bytes = b"jjk-safe-space-v1\0".to_vec();
    bytes.extend(canonical_bytes);
    Ok(Sha256::digest(bytes).to_vec())
}

fn repository_fingerprint(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    repo_id: Uuid,
    root_token: &[u8],
) -> Result<Vec<u8>, RuntimeError> {
    let head = observation_optional(git, cwd, ["rev-parse", "--verify", "HEAD"])?;
    let staged = git
        .run(
            cwd,
            observation_args([
                OsString::from("ls-files"),
                OsString::from("--stage"),
                OsString::from("-z"),
            ]),
        )
        .map_err(git_error)?;
    if staged.exit_code != 0 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&staged.stderr).trim().into(),
        ));
    }
    let index_digest = hex::encode(Sha256::digest(&staged.stdout));
    let status = observation_required(git, cwd, ["status", "--porcelain=v2", "--branch"])?;
    serde_json::to_vec(&serde_json::json!({"repo_id":repo_id,"root_token":hex::encode(root_token),"head":head,"index_entries_sha256":index_digest,"status":status})).map_err(internal)
}

fn unique_state_label(
    store: &SqliteStore,
    kind: &str,
    message: &str,
) -> Result<String, RuntimeError> {
    let base = crate::adapters::sqlite::label_base(message);
    let base = if base.is_empty() {
        kind.to_owned()
    } else {
        base
    };
    let labels = store
        .state_rows()
        .map_err(internal)?
        .into_iter()
        .map(|row| row.label)
        .collect::<std::collections::BTreeSet<_>>();
    if !labels.contains(&base) {
        return Ok(base);
    }
    for suffix in 2_u64.. {
        let candidate = format!("{base}-{suffix}");
        if !labels.contains(&candidate) {
            return Ok(candidate);
        }
    }
    unreachable!()
}

fn display_state_id(value: &str) -> Result<String, RuntimeError> {
    Ok(StateId::from_uuid(parse_state_id(value)?)
        .map_err(internal)?
        .to_string())
}
fn display_attempt_id(value: &str) -> Result<String, RuntimeError> {
    Ok(AttemptId::from_uuid(parse_attempt_id(value)?)
        .map_err(internal)?
        .to_string())
}
fn parse_state_id(value: &str) -> Result<Uuid, RuntimeError> {
    parse_hex_uuid(value, "state id")
}
fn parse_attempt_id(value: &str) -> Result<Uuid, RuntimeError> {
    parse_hex_uuid(value, "attempt id")
}
fn parse_hex_uuid(value: &str, field: &str) -> Result<Uuid, RuntimeError> {
    Uuid::from_slice(&hex::decode(value).map_err(internal)?)
        .map_err(|error| RuntimeError::Internal(format!("invalid {field}: {error}")))
}
fn deterministic_v7_uuid(seed: &[u8]) -> Uuid {
    let mut bytes: [u8; 16] = Sha256::digest(seed)[..16]
        .try_into()
        .expect("fixed digest slice");
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "attempt".to_owned()
    } else {
        slug
    }
}
fn workspace_matches_state(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    state: Option<&RuntimeStateRow>,
    captured: Option<&RuntimeGitSnapshot>,
) -> Result<bool, RuntimeError> {
    let Some(state) = state else {
        return Ok(observation_required(git, cwd, ["status", "--porcelain"])?.is_empty());
    };
    // Two things must hold before a restore may overwrite the checkout. The worktree content of
    // state-tracked paths must equal the state tree (untracked extras are tolerated: a restore
    // never deletes uncaptured paths), and the index must carry nothing a restore would lose:
    // it equals the worktree, or it is exactly the index the current state captured — the
    // ordinary JJK flow (commit, edit without `git add`, capture, navigate) leaves the index
    // behind the worktree, and a return to the current state brings that index back.
    let cached = git
        .run(
            cwd,
            observation_args([
                OsString::from("diff-index"),
                OsString::from("--cached"),
                OsString::from("--quiet"),
                OsString::from(&state.git_oid),
                OsString::from("--"),
            ]),
        )
        .map_err(git_error)?;
    if cached.exit_code > 1 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&cached.stderr).trim().into(),
        ));
    }
    // Fast path: index equals the state tree and the worktree equals the index.
    let index_matches_worktree = index_matches_worktree(git, cwd)?;
    if cached.exit_code == 0 && index_matches_worktree {
        return Ok(true);
    }
    let state_tree = observation_required_os(
        git,
        cwd,
        [
            OsString::from("rev-parse"),
            OsString::from(format!("{}^{{tree}}", state.git_oid)),
        ],
    )?;
    if state_relative_content_tree(git, cwd, &state.git_oid)? != state_tree {
        return Ok(false);
    }
    if index_matches_worktree {
        return Ok(true);
    }
    match captured {
        Some(snapshot) => index_matches_snapshot(git, cwd, snapshot),
        None => Ok(false),
    }
}

/// Git control snapshot the current state captured, when one exists.
fn captured_snapshot(
    context: &RuntimeContext,
    state: Option<&RuntimeStateRow>,
) -> Result<Option<RuntimeGitSnapshot>, RuntimeError> {
    let Some(state) = state else {
        return Ok(None);
    };
    context
        .store
        .runtime_git_snapshot_for_state(context.workspace_id, &state.state_id)
        .map_err(internal)
}
/// Whether every index entry's content equals the worktree, judged by content rather than the
/// index stat cache. `git diff-files` trusts stat data, so right after a restore (index bytes
/// rewritten, files rewritten) it reports phantom changes until a porcelain command refreshes
/// the index; this check compares a private refreshed copy of the index instead and never
/// writes the user's index.
fn index_matches_worktree(git: &GitCli<OsProcess>, cwd: &Path) -> Result<bool, RuntimeError> {
    // Fast path: when the stat cache is fresh and reports no change, the index matches.
    // Only a stat-dirty answer (exit 1) needs the content-aware check below.
    let quick = git
        .run(
            cwd,
            observation_args([
                OsString::from("diff-files"),
                OsString::from("--quiet"),
                OsString::from("--"),
            ]),
        )
        .map_err(git_error)?;
    if quick.exit_code == 0 {
        return Ok(true);
    }
    if quick.exit_code > 1 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&quick.stderr).trim().into(),
        ));
    }
    let index_path = repo_facts(git, cwd)?.index_path;
    let index_bytes = match fs::read(&index_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(internal(error)),
    };
    let index_tree = git
        .run(cwd, observation_args([OsString::from("write-tree")]))
        .map_err(git_error)?;
    if index_tree.exit_code != 0 {
        // Unmerged entries: the index cannot represent one tree, so it cannot match.
        return Ok(false);
    }
    let private_index = scratch_index_path(git, cwd, "refresh")?;
    fs::write(&private_index, &index_bytes).map_err(internal)?;
    let mut env = BTreeMap::new();
    env.insert(
        OsString::from("GIT_INDEX_FILE"),
        Some(private_index.as_os_str().to_owned()),
    );
    let result = (|| -> Result<String, RuntimeError> {
        required_output(
            git.run_with_env(
                cwd,
                observation_args([OsString::from("add"), OsString::from("-u")]),
                env.clone(),
            )
            .map_err(git_error)?,
        )?;
        required_output(
            git.run_with_env(cwd, observation_args([OsString::from("write-tree")]), env)
                .map_err(git_error)?,
        )
    })();
    let _ = fs::remove_file(&private_index);
    Ok(result? == String::from_utf8_lossy(&index_tree.stdout).trim())
}
/// Tree ID of the worktree as seen through a private index seeded from the state tree and
/// refreshed with `add -u`: modifications and deletions of state-captured paths change the
/// result, untracked extras do not — the same tolerance the real-index path has. The user's
/// real index is never touched.
fn state_relative_content_tree(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    state_oid: &str,
) -> Result<String, RuntimeError> {
    let private_index = scratch_index_path(git, cwd, "match")?;
    let env = with_index(&private_index, snapshot_object_env(git, cwd)?);
    let result = (|| -> Result<String, RuntimeError> {
        required_output(
            git.run_with_env(
                cwd,
                observation_args([
                    OsString::from("read-tree"),
                    OsString::from(format!("{state_oid}^{{tree}}")),
                ]),
                env.clone(),
            )
            .map_err(git_error)?,
        )?;
        required_output(
            git.run_with_env(
                cwd,
                observation_args([OsString::from("add"), OsString::from("-u")]),
                env.clone(),
            )
            .map_err(git_error)?,
        )?;
        required_output(
            git.run_with_env(cwd, observation_args([OsString::from("write-tree")]), env)
                .map_err(git_error)?,
        )
    })();
    let _ = fs::remove_file(&private_index);
    result
}
fn now_utc() -> Result<String, RuntimeError> {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(internal)
}
fn required_output_bytes(output: crate::ports::git::GitOutput) -> Result<Vec<u8>, RuntimeError> {
    if output.exit_code != 0 {
        Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ))
    } else {
        Ok(output.stdout)
    }
}

fn required_output(output: crate::ports::git::GitOutput) -> Result<String, RuntimeError> {
    if output.exit_code != 0 {
        Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ))
    } else {
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().into())
    }
}

fn presentation(args: &[OsString]) -> Result<Format, RuntimeError> {
    let mut format = Format::Human(default_width());
    let mut index = 0;
    while index < args.len() {
        let option = args[index]
            .to_str()
            .ok_or_else(|| RuntimeError::InvalidArguments("options must be UTF-8".into()))?;
        match option {
            "--json" | "--format=json" => format = Format::Json,
            "--format=human" => format = Format::Human(default_width()),
            "--no-color" => {}
            "--format" => {
                index += 1;
                format = match args.get(index).and_then(|value| value.to_str()) {
                    Some("json") => Format::Json,
                    Some("human") => Format::Human(default_width()),
                    _ => {
                        return Err(RuntimeError::InvalidArguments(
                            "`--format` requires human or json".into(),
                        ));
                    }
                };
            }
            "--width" => {
                index += 1;
                let width = parse_width(args.get(index).and_then(|value| value.to_str()))?;
                if matches!(format, Format::Human(_)) {
                    format = Format::Human(width);
                }
            }
            value if value.starts_with("--width=") => {
                let width = parse_width(Some(&value[8..]))?;
                if matches!(format, Format::Human(_)) {
                    format = Format::Human(width);
                }
            }
            _ => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "unknown option `{option}`"
                )));
            }
        }
        index += 1;
    }
    Ok(format)
}
fn default_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|width| *width > 0)
        .unwrap_or(80)
}
fn parse_width(value: Option<&str>) -> Result<usize, RuntimeError> {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| RuntimeError::InvalidArguments("width must be positive".into()))
}
fn capture_arguments(args: &[OsString]) -> Result<(Format, String), RuntimeError> {
    let index = args
        .iter()
        .position(|value| value == OsStr::new("--"))
        .ok_or_else(|| RuntimeError::InvalidArguments("capture requires `-- <message>`".into()))?;
    let message = args[index + 1..]
        .iter()
        .map(|value| value.to_str())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| RuntimeError::InvalidArguments("message must be UTF-8".into()))?
        .join(" ");
    if message.trim().is_empty() {
        return Err(RuntimeError::InvalidArguments(
            "message cannot be empty".into(),
        ));
    }
    Ok((presentation(&args[..index])?, message))
}
fn state_argument(args: &[OsString], command: &str) -> Result<(Format, String), RuntimeError> {
    let mut format = Format::Human(default_width());
    let mut target = None;
    let mut index = 0;
    while index < args.len() {
        let value = args[index].to_str().ok_or_else(|| {
            RuntimeError::InvalidArguments(format!("`{command}` arguments must be UTF-8"))
        })?;
        match value {
            "--json" | "--format=json" => format = Format::Json,
            "--format=human" | "--no-color" => {}
            "--format" => {
                index += 1;
                format = match args.get(index).and_then(|value| value.to_str()) {
                    Some("json") => Format::Json,
                    Some("human") => Format::Human(default_width()),
                    _ => {
                        return Err(RuntimeError::InvalidArguments(
                            "`--format` requires human or json".into(),
                        ));
                    }
                };
            }
            value if value.starts_with('-') => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "unknown option `{value}`"
                )));
            }
            value if target.is_none() => target = Some(value.to_owned()),
            _ => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "`{command}` requires exactly one state id or label"
                )));
            }
        }
        index += 1;
    }
    Ok((
        format,
        target.ok_or_else(|| {
            RuntimeError::InvalidArguments(format!(
                "`{command}` requires exactly one state id or label"
            ))
        })?,
    ))
}
fn optional_state_argument(
    args: &[OsString],
    command: &str,
) -> Result<(Format, Option<String>), RuntimeError> {
    let mut format = Format::Human(default_width());
    let mut target = None;
    let mut index = 0;
    while index < args.len() {
        let value = args[index].to_str().ok_or_else(|| {
            RuntimeError::InvalidArguments(format!("`{command}` arguments must be UTF-8"))
        })?;
        match value {
            "--json" | "--format=json" => format = Format::Json,
            "--format=human" | "--no-color" => {}
            "--format" => {
                index += 1;
                format = match args.get(index).and_then(|value| value.to_str()) {
                    Some("json") => Format::Json,
                    Some("human") => Format::Human(default_width()),
                    _ => {
                        return Err(RuntimeError::InvalidArguments(
                            "`--format` requires human or json".into(),
                        ));
                    }
                };
            }
            value if value.starts_with('-') => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "unknown option `{value}`"
                )));
            }
            value if target.is_none() => target = Some(value.to_owned()),
            _ => {
                return Err(RuntimeError::InvalidArguments(format!(
                    "`{command}` accepts at most one state id or label"
                )));
            }
        }
        index += 1;
    }
    Ok((format, target))
}
fn emit(format: Format, value: &impl Serialize) -> Result<(), RuntimeError> {
    let json = serde_json::to_value(value).map_err(internal)?;
    match format {
        Format::Json => println!("{}", serde_json::to_string(&json).map_err(internal)?),
        Format::Human(width) => println!("{}", render_runtime_human(&json, width)),
    }
    Ok(())
}

fn unavailable_store(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::Unavailable(error.to_string())
}
fn append_navigation(navigation: &mut RuntimeNavigation, current: &str, next: &str) {
    if navigation.entries.is_empty() {
        navigation.entries.push(current.into());
        navigation.cursor = Some(0);
    }
    let cursor = navigation.cursor.unwrap_or(navigation.entries.len() - 1);
    navigation.entries.truncate(cursor + 1);
    navigation.entries.push(next.into());
    // Back/forward history is a convenience window, not an archive: bound it so control
    // records stay small no matter how long a workspace lives.
    if navigation.entries.len() > NAVIGATION_HISTORY_LIMIT {
        let excess = navigation.entries.len() - NAVIGATION_HISTORY_LIMIT;
        navigation.entries.drain(..excess);
    }
    navigation.cursor = Some(navigation.entries.len() - 1);
}
/// Maximum retained back/forward entries per workspace.
const NAVIGATION_HISTORY_LIMIT: usize = 200;
fn create_commit(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    tree: &str,
    parent: Option<&str>,
    message: &str,
) -> Result<String, RuntimeError> {
    let mut args = vec![
        OsString::from("-c"),
        OsString::from("user.name=JJK"),
        OsString::from("-c"),
        OsString::from("user.email=jjk@localhost"),
        OsString::from("commit-tree"),
        OsString::from(tree),
    ];
    if let Some(parent) = parent {
        args.extend([OsString::from("-p"), OsString::from(parent)]);
    }
    args.extend([OsString::from("-m"), OsString::from(message)]);
    required_os(git, cwd, args)
}
fn stable_patch_id(cwd: &Path, diff: &[u8]) -> Result<String, RuntimeError> {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new("git")
        .args(["patch-id", "--stable"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(internal)?;
    child
        .stdin
        .take()
        .ok_or_else(|| RuntimeError::Internal("patch-id stdin unavailable".into()))?
        .write_all(diff)
        .map_err(internal)?;
    let output = child.wait_with_output().map_err(internal)?;
    if !output.status.success() {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| RuntimeError::Internal("git patch-id produced no identity".into()))
}
fn conflict_paths(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    env: BTreeMap<OsString, Option<OsString>>,
) -> Result<Vec<String>, RuntimeError> {
    let output = git
        .run_with_env(cwd, ["ls-files", "-u", "-z"], env)
        .map_err(git_error)?;
    if output.exit_code != 0 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    let mut paths = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|item| !item.is_empty())
        .filter_map(|entry| {
            entry
                .split(|byte| *byte == b'\t')
                .nth(1)
                .map(|path| String::from_utf8_lossy(path).into_owned())
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn git_control_receipt_preimage(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    snapshot: &RuntimeGitSnapshot,
) -> Result<serde_json::Value, RuntimeError> {
    let refs = git
        .run(
            cwd,
            [
                "for-each-ref",
                "--sort=refname",
                "--format=%(refname)%00%(objectname)%00%(symref)%00",
            ],
        )
        .map_err(git_error)?;
    let stages = git
        .run(cwd, ["ls-files", "--stage", "-z"])
        .map_err(git_error)?;
    let status = git
        .run(cwd, ["status", "--porcelain=v2", "-z"])
        .map_err(git_error)?;
    let symbolic = git
        .run(cwd, ["symbolic-ref", "-q", "HEAD"])
        .map_err(git_error)?;
    let head = git
        .run(cwd, ["rev-parse", "--verify", "HEAD"])
        .map_err(git_error)?;
    let tracked_paths = git
        .run(cwd, ["ls-files", "-z"])
        .map_err(git_error)?
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect::<std::collections::BTreeSet<_>>();
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    let live_index = if snapshot.index_blob.is_some() || snapshot.index.is_empty() {
        let index_path = repo_facts(git, cwd)?.index_path;
        match fs::read(&index_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(internal(error)),
        }
    } else {
        snapshot.index.clone()
    };
    for entry in &capture_git_visible_entries(git, cwd)? {
        let (path, value) = worktree_receipt(entry);
        if tracked_paths.contains(&path) {
            tracked.push(value);
        } else {
            untracked.push(value);
        }
    }
    Ok(
        serde_json::json!({"head_attachment_hex":hex::encode(symbolic.stdout),"head_oid_hex":hex::encode(head.stdout),"refs_hex":hex::encode(refs.stdout),"index_sha256":hex::encode(Sha256::digest(&live_index)),"index_stages_hex":hex::encode(stages.stdout),"status_hex":hex::encode(status.stdout),"tracked":tracked,"untracked":untracked}),
    )
}

fn worktree_receipt(entry: &RuntimeWorktreeEntry) -> (Vec<u8>, serde_json::Value) {
    match entry {
        RuntimeWorktreeEntry::Regular { path, bytes, .. } => (
            path.clone(),
            serde_json::json!({"path":String::from_utf8_lossy(path),"bytes_hex":hex::encode(bytes)}),
        ),
        RuntimeWorktreeEntry::Symlink { path, target } => (
            path.clone(),
            serde_json::json!({"path":String::from_utf8_lossy(path),"bytes_hex":hex::encode(target)}),
        ),
    }
}
fn render_runtime_human(value: &serde_json::Value, width: usize) -> String {
    let command = value
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("jjk");
    let rendered = match command {
        "setup" => format!(
            "safe space: {}\nstore: {}\nrepository: {} ({})",
            value["repository"].as_str().unwrap_or("unknown"),
            value["store"].as_str().unwrap_or("unknown"),
            value["repository_id"].as_str().unwrap_or("unknown"),
            if value["created"].as_bool().unwrap_or(false) {
                "created"
            } else {
                "ready"
            }
        ),
        "current" => format!(
            "current: {}{}\nkind: {}\nlabel: {}\nattempt: {}\ngit: {}\nparent: {}",
            if value["starred"].as_bool().unwrap_or(false) {
                "★ "
            } else {
                ""
            },
            value["state_id"].as_str().unwrap_or("none"),
            value["kind"].as_str().unwrap_or("state"),
            value["label"].as_str().unwrap_or(""),
            value["attempt_id"].as_str().unwrap_or("none"),
            value["commit"].as_str().unwrap_or("none"),
            value["logical_parent"].as_str().unwrap_or("none")
        ),
        "see" | "story" => value["states"]
            .as_array()
            .map(|states| {
                if states.is_empty() {
                    return "No visible states saved yet.".to_owned();
                }
                let current = value["current_state"].as_str();
                let mut lines = vec!["   state          kind     label".to_owned()];
                for state in states {
                    let id = state["state_id"].as_str().unwrap_or("unknown");
                    let current_marker = if current == Some(id) { "*" } else { " " };
                    let star_marker = if state["starred"].as_bool().unwrap_or(false) {
                        "★"
                    } else {
                        " "
                    };
                    let short = id.get(..id.len().min(14)).unwrap_or(id);
                    lines.push(format!(
                        "{current_marker}{star_marker} {short:<14} {:<8} {}",
                        state["kind"].as_str().unwrap_or("state"),
                        state["label"].as_str().unwrap_or("")
                    ));
                }
                lines.join("\n")
            })
            .unwrap_or_else(|| "No visible states saved yet.".to_owned()),
        "star" | "unstar" => format!(
            "{} {}{}",
            if value["starred"].as_bool().unwrap_or(false) {
                "starred"
            } else {
                "unstarred"
            },
            value["state_id"].as_str().unwrap_or("unknown"),
            if value["changed"].as_bool() == Some(false) {
                " (unchanged)"
            } else {
                ""
            }
        ),
        "doctor" => {
            let mut lines = vec![
                format!(
                    "integrity: {}",
                    if value["healthy"].as_bool().unwrap_or(false) {
                        "healthy"
                    } else {
                        "repair required"
                    }
                ),
                format!("events: {}", value["journal_events"]),
                format!(
                    "journal: {}",
                    value["journal_head"].as_str().unwrap_or("none")
                ),
            ];
            append_pending_operation_lines(&mut lines, value);
            lines.join("\n")
        }
        "status" => {
            let mut lines = value["porcelain_v2"]
                .as_str()
                .unwrap_or("")
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if !value["branches"].as_array().is_none_or(Vec::is_empty) {
                lines.push(format!(
                    "branches: {}",
                    value["branches"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if let Some(orientation) = value.get("orientation").filter(|value| !value.is_null()) {
                lines.push(format!(
                    "JJK current: {}",
                    orientation["state_id"].as_str().unwrap_or("none")
                ));
                lines.push(format!(
                    "attempt: {}",
                    orientation["attempt_id"].as_str().unwrap_or("none")
                ));
            }
            lines.push(format!(
                "recovery: {}",
                if value["recovery_required"].as_bool().unwrap_or(false) {
                    "required"
                } else {
                    "none"
                }
            ));
            append_pending_operation_lines(&mut lines, value);
            lines.join("\n")
        }
        _ if value.get("attempt_id").is_some() && value.get("from_state").is_some() => format!(
            "forked: {}\nfrom: {}\nobjective: {}",
            value["attempt_id"].as_str().unwrap_or("unknown"),
            value["from_state"].as_str().unwrap_or("unknown"),
            value["objective"].as_str().unwrap_or("")
        ),
        _ if value.get("state_id").is_some() => format!(
            "{command}: {}\ngit: {}",
            value["state_id"].as_str().unwrap_or("unknown"),
            value["commit"].as_str().unwrap_or("unchanged")
        ),
        _ => serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| "jjk: output unavailable".to_owned()),
    };
    rendered
        .lines()
        .map(|line| fit_line(line, width))
        .collect::<Vec<_>>()
        .join("\n")
}
fn append_pending_operation_lines(lines: &mut Vec<String>, value: &serde_json::Value) {
    for operation in value["pending_operations"].as_array().into_iter().flatten() {
        lines.push(format!(
            "operation {}: {} {}",
            operation["operation_id"].as_str().unwrap_or("unknown"),
            operation["command"]
                .as_str()
                .or_else(|| operation["command_kind"].as_str())
                .unwrap_or("unknown"),
            operation["phase"]
                .as_str()
                .or_else(|| operation["status"].as_str())
                .unwrap_or("unknown"),
        ));
        if let Some(next_action) = operation["next_action"].as_str() {
            lines.push(format!("next: {next_action}"));
        }
    }
}

fn fit_line(line: &str, width: usize) -> String {
    if line.chars().count() <= width {
        return line.to_owned();
    }
    if width <= 3 {
        return line.chars().take(width).collect();
    }
    let mut bounded = line.chars().take(width - 3).collect::<String>();
    bounded.push_str("...");
    bounded
}
fn required<const N: usize>(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    args: [&str; N],
) -> Result<String, RuntimeError> {
    required_os(git, cwd, args.map(OsString::from))
}
fn optional<const N: usize>(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    args: [&str; N],
) -> Result<Option<String>, RuntimeError> {
    let output = git.run(cwd, args).map_err(git_error)?;
    Ok((output.exit_code == 0).then(|| String::from_utf8_lossy(&output.stdout).trim().into()))
}
fn required_os<I: IntoIterator<Item = OsString>>(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    args: I,
) -> Result<String, RuntimeError> {
    let output = git.run(cwd, args).map_err(git_error)?;
    if output.exit_code != 0 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().into())
}
fn observation_args<I: IntoIterator<Item = OsString>>(args: I) -> Vec<OsString> {
    [
        OsString::from("-c"),
        OsString::from("core.fsmonitor=false"),
        OsString::from("-c"),
        OsString::from("core.hooksPath=/dev/null"),
    ]
    .into_iter()
    .chain(args)
    .collect()
}
fn observation_required<const N: usize>(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    args: [&str; N],
) -> Result<String, RuntimeError> {
    observation_required_os(git, cwd, args.map(OsString::from))
}
fn observation_optional<const N: usize>(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    args: [&str; N],
) -> Result<Option<String>, RuntimeError> {
    let output = git
        .run(cwd, observation_args(args.map(OsString::from)))
        .map_err(git_error)?;
    Ok((output.exit_code == 0).then(|| String::from_utf8_lossy(&output.stdout).trim().into()))
}
fn observation_required_os<I: IntoIterator<Item = OsString>>(
    git: &GitCli<OsProcess>,
    cwd: &Path,
    args: I,
) -> Result<String, RuntimeError> {
    let output = git.run(cwd, observation_args(args)).map_err(git_error)?;
    if output.exit_code != 0 {
        return Err(RuntimeError::Unavailable(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().into())
}
fn internal(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::Internal(error.to_string())
}
fn git_error(error: GitError) -> RuntimeError {
    match error {
        GitError::Command { diagnostic, .. } => RuntimeError::Unavailable(diagnostic),
        other => internal(other),
    }
}
