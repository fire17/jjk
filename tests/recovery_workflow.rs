use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const BASE: &[u8] = b"base\0fixture\n";
const ONE: &[u8] = b"state-one\0exact\n";
const TWO: &[u8] = b"state-two\0exact\n";
const THREE: &[u8] = b"state-three\0exact\n";
const ALTERNATE: &[u8] = b"state-alternate\0exact\n";

struct Harness {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
    xdg_config: PathBuf,
    xdg_state: PathBuf,
    global_git_config: PathBuf,
    jjk: PathBuf,
}

impl Harness {
    fn new(name: &str) -> Self {
        let temp = TempDir::new().expect("create isolated recovery fixture");
        let root = temp.path().join(name);
        let home = temp.path().join("home");
        let xdg_config = temp.path().join("xdg/config");
        let xdg_state = temp.path().join("xdg/state");
        let global_git_config = temp.path().join("gitconfig");
        fs::create_dir_all(&root).expect("create repository root");
        fs::create_dir_all(&home).expect("create isolated HOME");
        fs::create_dir_all(&xdg_config).expect("create isolated XDG config");
        fs::create_dir_all(&xdg_state).expect("create isolated XDG state");
        fs::write(&global_git_config, b"").expect("create empty global Git config");

        let fixture = Self {
            _temp: temp,
            root,
            home,
            xdg_config,
            xdg_state,
            global_git_config,
            jjk: assert_cmd::cargo::cargo_bin!("jjk").to_path_buf(),
        };
        fixture.git(&fixture.root, &["init", "-q", "-b", "main"]);
        fixture.git(
            &fixture.root,
            &["config", "user.name", "JJK Recovery Fixture"],
        );
        fixture.git(
            &fixture.root,
            &["config", "user.email", "recovery@example.invalid"],
        );
        fs::write(fixture.root.join("state.bin"), BASE).expect("write deterministic base bytes");
        fixture.git(&fixture.root, &["add", "state.bin"]);
        fixture.git(&fixture.root, &["commit", "-qm", "deterministic base"]);
        let setup = fixture.jjk_json(&fixture.root, &["setup", "--json"]);
        assert_eq!(setup["command"], "setup");
        assert_eq!(setup["created"], true);
        fixture
    }

    fn command(&self, cwd: &Path, program: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg_config)
            .env("XDG_STATE_HOME", &self.xdg_state)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_git_config)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env("GIT_AUTHOR_DATE", "2001-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2001-01-01T00:00:00Z");
        command
    }

    fn run_git(&self, cwd: &Path, args: &[&str]) -> Output {
        self.command(cwd, Path::new("git"), args)
            .output()
            .expect("execute real Git")
    }

    fn git(&self, cwd: &Path, args: &[&str]) -> Vec<u8> {
        let output = self.run_git(cwd, args);
        assert!(
            output.status.success(),
            "git {} failed with {:?}: {}",
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    fn run_jjk(&self, cwd: &Path, args: &[&str]) -> Output {
        self.command(cwd, &self.jjk, args)
            .output()
            .expect("execute compiled jjk")
    }

    fn jjk_json(&self, cwd: &Path, args: &[&str]) -> Value {
        let output = self.run_jjk(cwd, args);
        assert!(
            output.status.success(),
            "jjk {} failed with {:?}: stdout={} stderr={}",
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        success_payload(parse_json(&output.stdout, "successful JJK command"))
    }

    fn jjk_json_failure(&self, cwd: &Path, args: &[&str], expected_exit: i32) -> Value {
        let output = self.run_jjk(cwd, args);
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "jjk {} had unexpected result: stdout={} stderr={}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let value = parse_json(
            if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            },
            "failed JJK --json command",
        );
        assert!(
            value.get("error").is_some_and(|error| !error.is_null())
                || value.get("outcome").and_then(Value::as_str) == Some("failed"),
            "failure JSON does not identify a failed outcome: {value}"
        );
        value
    }

    fn capture(&self, cwd: &Path, label: &str, bytes: &[u8]) -> Value {
        fs::write(cwd.join("state.bin"), bytes).expect("write deterministic state bytes");
        self.git(cwd, &["add", "state.bin"]);
        let state = self.jjk_json(cwd, &["step", "--json", "--", label]);
        assert_eq!(state["command"], "step");
        assert!(
            state["state_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("st_"))
        );
        assert!(state["commit"].as_str().is_some_and(|oid| !oid.is_empty()));
        state
    }

    fn temp_path(&self, name: &str) -> PathBuf {
        self._temp.path().join(name)
    }
}

#[derive(Debug, PartialEq)]
struct SurfaceFingerprint {
    repository: RepositoryFingerprint,
    jjk: JjkFingerprint,
}

#[derive(Debug, PartialEq)]
struct RepositoryFingerprint {
    files: BTreeMap<String, FileImage>,
    refs: Vec<u8>,
    head_ref: Option<Vec<u8>>,
    head_oid: Vec<u8>,
    index_tree: Vec<u8>,
    index_entries: Vec<u8>,
    index_flags: Vec<u8>,
    status: Vec<u8>,
}

#[derive(Debug, PartialEq)]
enum FileImage {
    Regular { mode: u32, bytes: Vec<u8> },
    Symlink(Vec<u8>),
}

#[derive(Debug, PartialEq)]
struct JjkFingerprint {
    current: Value,
    visible_states: Vec<Value>,
}

fn parse_json(bytes: &[u8], context: &str) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!(
            "{context} did not emit one JSON value: {error}; bytes={}",
            String::from_utf8_lossy(bytes)
        )
    })
}

fn success_payload(value: Value) -> Value {
    match value.get("result") {
        Some(result) if !result.is_null() => result.clone(),
        _ => value,
    }
}

fn state_id(state: &Value) -> &str {
    state["state_id"].as_str().expect("state_id string")
}

fn state_commit(state: &Value) -> &str {
    state["commit"].as_str().expect("state commit string")
}

fn surface(fixture: &Harness, cwd: &Path) -> SurfaceFingerprint {
    SurfaceFingerprint {
        repository: repository_fingerprint(fixture, cwd),
        jjk: jjk_fingerprint(fixture, cwd),
    }
}

fn repository_fingerprint(fixture: &Harness, cwd: &Path) -> RepositoryFingerprint {
    let status = fixture.git(
        cwd,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    );
    let refs = fixture.git(
        cwd,
        &[
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname)%00%(objectname)%00%(symref)%00",
        ],
    );
    // The `jjk/trail` mirror branch is derived from the current state by design (it moves on
    // every navigation and undo/redo), so it is not part of the restorable surface.
    let refs = refs
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.starts_with(b"refs/heads/jjk/trail\0"))
        .flat_map(|line| line.iter().copied().chain(std::iter::once(b'\n')))
        .collect::<Vec<u8>>();
    let symbolic = fixture.run_git(cwd, &["symbolic-ref", "-q", "HEAD"]);
    let head_ref = match symbolic.status.code() {
        Some(0) => Some(symbolic.stdout),
        Some(1) => None,
        other => panic!(
            "git symbolic-ref failed with {other:?}: {}",
            String::from_utf8_lossy(&symbolic.stderr)
        ),
    };
    RepositoryFingerprint {
        files: filesystem_fingerprint(cwd),
        refs,
        head_ref,
        head_oid: fixture.git(cwd, &["rev-parse", "--verify", "HEAD"]),
        index_tree: fixture.git(cwd, &["write-tree"]),
        index_entries: fixture.git(cwd, &["ls-files", "--stage", "-z"]),
        index_flags: fixture.git(cwd, &["ls-files", "-v", "-z"]),
        status,
    }
}

fn jjk_fingerprint(fixture: &Harness, cwd: &Path) -> JjkFingerprint {
    let current = fixture.jjk_json(cwd, &["current", "--json"]);
    let graph = fixture.jjk_json(cwd, &["see", "--json"]);
    JjkFingerprint {
        current,
        visible_states: graph["states"]
            .as_array()
            .expect("see.states array")
            .clone(),
    }
}

fn filesystem_fingerprint(root: &Path) -> BTreeMap<String, FileImage> {
    let mut files = BTreeMap::new();
    collect_files(root, root, &mut files);
    files
}

fn collect_files(root: &Path, directory: &Path, files: &mut BTreeMap<String, FileImage>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .collect::<Result<Vec<_>, _>>()
        .expect("collect fixture directory");
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if directory == root && entry.file_name() == OsStr::new(".git") {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).expect("inspect fixture path");
        if metadata.is_dir() {
            collect_files(root, &path, files);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("fixture path below root")
            .to_string_lossy()
            .replace('\\', "/");
        let image = if metadata.file_type().is_symlink() {
            FileImage::Symlink(
                fs::read_link(&path)
                    .expect("read fixture symlink")
                    .as_os_str()
                    .as_encoded_bytes()
                    .to_vec(),
            )
        } else {
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o7777
            };
            #[cfg(not(unix))]
            let mode = u32::from(metadata.permissions().readonly());
            FileImage::Regular {
                mode,
                bytes: fs::read(&path).expect("read fixture file"),
            }
        };
        assert!(
            files.insert(relative, image).is_none(),
            "duplicate fixture path"
        );
    }
}

fn visible_state<'a>(fingerprint: &'a JjkFingerprint, id: &str) -> &'a Value {
    fingerprint
        .visible_states
        .iter()
        .find(|state| state["state_id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("state {id} is not visible"))
}

fn visible_ids(fingerprint: &JjkFingerprint) -> BTreeSet<String> {
    fingerprint
        .visible_states
        .iter()
        .map(|state| {
            state["state_id"]
                .as_str()
                .expect("visible state ID")
                .to_owned()
        })
        .collect()
}

fn assert_state_materialized(fixture: &Harness, cwd: &Path, state: &Value, expected_bytes: &[u8]) {
    let id = state_id(state);
    let commit = state_commit(state);
    let current = fixture.jjk_json(cwd, &["current", "--json"]);
    assert_eq!(current["state_id"], id);
    assert_eq!(current["commit"], commit);
    assert_eq!(
        fs::read(cwd.join("state.bin")).expect("read materialized state"),
        expected_bytes
    );
    assert_eq!(
        trimmed(&fixture.git(cwd, &["write-tree"])),
        trimmed(&fixture.git(cwd, &["rev-parse", &format!("{commit}^{{tree}}")]))
    );
    assert_eq!(
        trimmed(&fixture.git(cwd, &["rev-parse", &format!("refs/jjk/states/{id}")])),
        commit.as_bytes()
    );
    assert_git_valid(fixture, cwd);
}

fn assert_git_valid(fixture: &Harness, cwd: &Path) {
    fixture.git(cwd, &["fsck", "--full"]);
    fixture.git(
        cwd,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    );
}

fn trimmed(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

fn remove_jjk_control_state(fixture: &Harness) -> PathBuf {
    let raw_common =
        String::from_utf8(fixture.git(&fixture.root, &["rev-parse", "--git-common-dir"]))
            .expect("UTF-8 common dir");
    let common = PathBuf::from(raw_common.trim());
    let common = if common.is_absolute() {
        common
    } else {
        fixture.root.join(common)
    };
    let control = common.join("jjk");
    let retained = fixture.temp_path("simulated-loss-jjk-control");
    fs::rename(&control, &retained).expect("move JJK metadata out to simulate loss");

    let refs = String::from_utf8(fixture.git(
        &fixture.root,
        &["for-each-ref", "--format=%(refname)", "refs/jjk"],
    ))
    .expect("UTF-8 JJK refs");
    for reference in refs.lines().filter(|line| !line.is_empty()) {
        fixture.git(&fixture.root, &["update-ref", "-d", reference]);
    }
    retained
}

#[test]
fn val_core_005_archive_recover_restores_exact_topology_and_every_other_surface() {
    let fixture = Harness::new("archive-recover");
    let first = fixture.capture(&fixture.root, "archive parent", ONE);
    let archived = fixture.capture(&fixture.root, "archive middle", TWO);
    let descendant = fixture.capture(&fixture.root, "archive descendant", THREE);
    fixture.jjk_json(&fixture.root, &["return", state_id(&first), "--json"]);

    let before = surface(&fixture, &fixture.root);
    assert_eq!(
        visible_state(&before.jjk, state_id(&descendant))["logical_parent"],
        state_id(&archived)
    );
    let archived_ref_before = fixture.git(
        &fixture.root,
        &[
            "rev-parse",
            &format!("refs/jjk/states/{}", state_id(&archived)),
        ],
    );

    let archive_result =
        fixture.jjk_json(&fixture.root, &["archive", state_id(&archived), "--json"]);
    assert_eq!(archive_result["state_id"], state_id(&archived));
    assert_eq!(archive_result["archived"], true);
    let hidden = surface(&fixture, &fixture.root);
    assert_eq!(
        hidden.repository, before.repository,
        "archive changed Git, index, or worktree bytes"
    );
    assert_eq!(
        hidden.jjk.current, before.jjk.current,
        "archive changed current projection"
    );
    assert!(!visible_ids(&hidden.jjk).contains(state_id(&archived)));
    assert!(visible_ids(&hidden.jjk).contains(state_id(&descendant)));
    assert_eq!(
        visible_state(&hidden.jjk, state_id(&descendant))["logical_parent"],
        state_id(&archived)
    );
    assert_eq!(
        fixture.git(
            &fixture.root,
            &[
                "rev-parse",
                &format!("refs/jjk/states/{}", state_id(&archived))
            ]
        ),
        archived_ref_before,
        "archive severed the retained Git anchor"
    );
    assert_git_valid(&fixture, &fixture.root);

    let recover_result =
        fixture.jjk_json(&fixture.root, &["recover", state_id(&archived), "--json"]);
    assert_eq!(recover_result["state_id"], state_id(&archived));
    assert_eq!(recover_result["archived"], false);
    let recovered = surface(&fixture, &fixture.root);
    assert_eq!(
        recovered, before,
        "recover did not restore the original public topology and exact repository surface"
    );
    assert_git_valid(&fixture, &fixture.root);
}

#[test]
fn val_core_005_back_forward_truncates_only_navigation_future_not_saved_future() {
    let fixture = Harness::new("navigation-truncation");
    let first = fixture.capture(&fixture.root, "navigation one", ONE);
    let second = fixture.capture(&fixture.root, "navigation two", TWO);
    let third = fixture.capture(&fixture.root, "navigation three", THREE);
    let third_surface = surface(&fixture, &fixture.root);
    let refs_with_third = third_surface.repository.refs.clone();

    let back = fixture.jjk_json(&fixture.root, &["back", "--json"]);
    assert_eq!(back["state_id"], state_id(&second));
    assert_eq!(back["history_position"], 2);
    assert_eq!(back["history_length"], 3);
    assert_state_materialized(&fixture, &fixture.root, &second, TWO);
    assert_eq!(
        repository_fingerprint(&fixture, &fixture.root).refs,
        refs_with_third
    );

    let forward = fixture.jjk_json(&fixture.root, &["forward", "--json"]);
    assert_eq!(forward["state_id"], state_id(&third));
    assert_eq!(forward["history_position"], 3);
    assert_eq!(forward["history_length"], 3);
    assert_eq!(surface(&fixture, &fixture.root), third_surface);

    fixture.jjk_json(&fixture.root, &["back", "--json"]);
    assert_state_materialized(&fixture, &fixture.root, &second, TWO);
    let alternate = fixture.capture(&fixture.root, "navigation alternate", ALTERNATE);
    assert_state_materialized(&fixture, &fixture.root, &alternate, ALTERNATE);
    let alternate_surface = surface(&fixture, &fixture.root);
    assert!(
        visible_ids(&alternate_surface.jjk).contains(state_id(&third)),
        "truncating navigation erased the saved future"
    );
    assert_eq!(
        trimmed(&fixture.git(
            &fixture.root,
            &[
                "rev-parse",
                &format!("refs/jjk/states/{}", state_id(&third))
            ]
        )),
        state_commit(&third).as_bytes(),
        "truncating navigation removed the old future's retention ref"
    );

    let back_after_divergence = fixture.jjk_json(&fixture.root, &["back", "--json"]);
    assert_eq!(back_after_divergence["state_id"], state_id(&second));
    assert_eq!(back_after_divergence["history_position"], 2);
    assert_eq!(back_after_divergence["history_length"], 3);
    assert_state_materialized(&fixture, &fixture.root, &second, TWO);
    let forward_after_divergence = fixture.jjk_json(&fixture.root, &["forward", "--json"]);
    assert_eq!(forward_after_divergence["state_id"], state_id(&alternate));
    assert_eq!(forward_after_divergence["history_position"], 3);
    assert_eq!(forward_after_divergence["history_length"], 3);
    assert_eq!(surface(&fixture, &fixture.root), alternate_surface);

    let before_refused_forward = surface(&fixture, &fixture.root);
    let failure = fixture.jjk_json_failure(&fixture.root, &["forward", "--json"], 3);
    assert!(failure.to_string().contains("no later navigation state"));
    assert_eq!(surface(&fixture, &fixture.root), before_refused_forward);
    assert!(visible_ids(&jjk_fingerprint(&fixture, &fixture.root)).contains(state_id(&first)));
}

#[test]
fn val_core_005_undo_redo_round_trips_refs_index_worktree_and_current_projection() {
    let fixture = Harness::new("whole-control-history");
    let first = fixture.capture(&fixture.root, "control first", ONE);
    let first_surface = surface(&fixture, &fixture.root);
    let second = fixture.capture(&fixture.root, "control second", TWO);
    let second_surface = surface(&fixture, &fixture.root);

    assert_ne!(
        first_surface.repository.refs,
        second_surface.repository.refs
    );
    assert_ne!(
        first_surface.repository.index_entries,
        second_surface.repository.index_entries
    );
    assert_ne!(
        first_surface.repository.files,
        second_surface.repository.files
    );
    assert_ne!(first_surface.jjk.current, second_surface.jjk.current);

    let undo = fixture.jjk_json(&fixture.root, &["undo", "--json"]);
    assert_eq!(undo["state_id"], state_id(&first));
    assert_eq!(undo["commit"], state_commit(&first));
    assert_eq!(
        undo["from_cursor"].as_u64(),
        undo["to_cursor"].as_u64().map(|cursor| cursor + 1)
    );
    assert_eq!(
        surface(&fixture, &fixture.root),
        first_surface,
        "undo restored only part of the control state"
    );
    assert_state_materialized(&fixture, &fixture.root, &first, ONE);
    let removed_second_ref = fixture.run_git(
        &fixture.root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/jjk/states/{}", state_id(&second)),
        ],
    );
    assert!(
        !removed_second_ref.status.success(),
        "undo left a ref absent from the exact prior ref namespace"
    );

    let redo = fixture.jjk_json(&fixture.root, &["redo", "--json"]);
    assert_eq!(redo["state_id"], state_id(&second));
    assert_eq!(redo["commit"], state_commit(&second));
    assert_eq!(
        redo["to_cursor"].as_u64(),
        redo["from_cursor"].as_u64().map(|cursor| cursor + 1)
    );
    assert_eq!(
        surface(&fixture, &fixture.root),
        second_surface,
        "redo did not restore the exact post-mutation control state"
    );
    assert_state_materialized(&fixture, &fixture.root, &second, TWO);
}

#[test]
fn val_backup_001_freeze_create_verify_and_tamper_rejection_are_non_mutating() {
    let fixture = Harness::new("freeze-verification");
    let first = fixture.capture(&fixture.root, "freeze first", ONE);
    let second = fixture.capture(&fixture.root, "freeze second", TWO);
    let before = surface(&fixture, &fixture.root);
    let artifact = fixture.temp_path("portable-freeze.jjkfreeze");
    let artifact_arg = artifact.to_str().expect("UTF-8 freeze path");

    let created = fixture.jjk_json(&fixture.root, &["freeze", "create", artifact_arg, "--json"]);
    assert_eq!(created["command"], "freeze");
    assert_eq!(created["action"], "created");
    assert_eq!(created["path"], artifact_arg);
    assert!(
        created["freeze_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    let included = created["included_state_ids"]
        .as_array()
        .expect("freeze included_state_ids");
    assert!(
        included
            .iter()
            .any(|id| id.as_str() == Some(state_id(&first)))
    );
    assert!(
        included
            .iter()
            .any(|id| id.as_str() == Some(state_id(&second)))
    );
    assert!(created["required_oids"].as_array().is_some_and(|oids| {
        oids.iter()
            .any(|oid| oid.as_str() == Some(state_commit(&second)))
    }));
    assert!(
        artifact.is_dir(),
        "freeze was not published as a self-describing artifact"
    );
    assert_eq!(
        surface(&fixture, &fixture.root),
        before,
        "freeze creation mutated the repository control surface"
    );

    let verified = fixture.jjk_json(&fixture.root, &["freeze", "verify", artifact_arg, "--json"]);
    assert_eq!(verified["command"], "freeze");
    assert_eq!(verified["action"], "verified");
    assert_eq!(verified["healthy"], true);
    assert_eq!(verified["freeze_id"], created["freeze_id"]);
    assert_eq!(
        verified["included_state_ids"],
        created["included_state_ids"]
    );
    assert_eq!(verified["required_oids"], created["required_oids"]);
    assert_eq!(
        surface(&fixture, &fixture.root),
        before,
        "freeze verification mutated source state"
    );

    fs::write(artifact.join("manifest.json"), b"{\"tampered\":true}\n")
        .expect("tamper freeze manifest");
    let before_rejection = surface(&fixture, &fixture.root);
    let rejected = fixture.jjk_json_failure(
        &fixture.root,
        &["freeze", "verify", artifact_arg, "--json"],
        70,
    );
    assert!(
        rejected
            .to_string()
            .to_ascii_lowercase()
            .contains("checksum")
    );
    assert_eq!(
        surface(&fixture, &fixture.root),
        before_rejection,
        "tampered freeze verification changed source state"
    );
    assert_git_valid(&fixture, &fixture.root);
}

#[test]
fn val_backup_001_backup_load_recovers_after_metadata_and_ref_loss_with_exact_scope() {
    let fixture = Harness::new("backup-disaster");
    fixture.capture(&fixture.root, "backup first", ONE);
    let second = fixture.capture(&fixture.root, "backup second", TWO);

    fs::write(
        fixture.root.join("state.bin"),
        b"dirty-worktree\0after-state\n",
    )
    .expect("write dirty tracked bytes");
    fs::write(fixture.root.join("split.bin"), b"staged-image\0v1\n").expect("write staged image");
    fixture.git(&fixture.root, &["add", "split.bin"]);
    fs::write(fixture.root.join("split.bin"), b"worktree-image\0v2\n")
        .expect("write unstaged image over staged image");
    fs::write(
        fixture.root.join("untracked.bin"),
        b"untracked\0must-survive\n",
    )
    .expect("write untracked recovery bytes");

    let source_before = surface(&fixture, &fixture.root);
    assert_eq!(source_before.jjk.current["state_id"], state_id(&second));
    let backup = fixture.temp_path("disaster.jjkbak");
    let backup_arg = backup.to_str().expect("UTF-8 backup path");
    let created = fixture.jjk_json(&fixture.root, &["backup", "create", backup_arg, "--json"]);
    assert_eq!(created["command"], "backup");
    assert_eq!(created["action"], "created");
    assert_eq!(created["path"], backup_arg);
    assert!(
        created["journal_head"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64)
    );
    assert_eq!(
        surface(&fixture, &fixture.root),
        source_before,
        "online backup mutated its source scope"
    );

    let verified = fixture.jjk_json(&fixture.root, &["backup", "verify", backup_arg, "--json"]);
    assert_eq!(verified["healthy"], true);
    assert_eq!(verified["journal_head"], created["journal_head"]);
    assert_eq!(verified["journal_events"], created["journal_events"]);
    assert_eq!(
        surface(&fixture, &fixture.root),
        source_before,
        "backup verification mutated its source"
    );

    let retained_control = remove_jjk_control_state(&fixture);
    assert!(
        retained_control.exists(),
        "simulated metadata loss did not retain evidence out of band"
    );
    let damaged_before_load = repository_fingerprint(&fixture, &fixture.root);
    let current_after_loss = fixture.run_jjk(&fixture.root, &["current", "--json"]);
    assert!(
        !current_after_loss.status.success(),
        "current unexpectedly survived total local metadata loss"
    );

    let restored = fixture.temp_path("restored-from-disaster");
    let restored_arg = restored.to_str().expect("UTF-8 restore target");
    let loaded = fixture.jjk_json(
        &fixture.root,
        &["load", backup_arg, "--into", restored_arg, "--json"],
    );
    assert_eq!(loaded["command"], "load");
    assert_eq!(loaded["action"], "restored");
    assert_eq!(loaded["source"], backup_arg);
    assert_eq!(loaded["target"], restored_arg);
    assert_eq!(
        repository_fingerprint(&fixture, &fixture.root),
        damaged_before_load,
        "load mutated the damaged source repository"
    );

    let restored_surface = surface(&fixture, &restored);
    assert_eq!(
        restored_surface, source_before,
        "disaster restore did not reproduce refs, index, files, and JJK projection exactly"
    );
    assert_eq!(
        fs::read(restored.join("state.bin")).expect("restored tracked worktree"),
        b"dirty-worktree\0after-state\n"
    );
    assert_eq!(
        fs::read(restored.join("split.bin")).expect("restored split worktree"),
        b"worktree-image\0v2\n"
    );
    assert_eq!(
        fs::read(restored.join("untracked.bin")).expect("restored untracked file"),
        b"untracked\0must-survive\n"
    );
    assert_git_valid(&fixture, &restored);
    let doctor = fixture.jjk_json(&restored, &["doctor", "--json"]);
    assert_eq!(doctor["healthy"], true);
}

#[test]
fn val_core_005_dirty_navigation_refuses_without_losing_any_file_index_ref_or_projection() {
    let fixture = Harness::new("dirty-refusal");
    let first = fixture.capture(&fixture.root, "dirty target", ONE);
    fixture.capture(&fixture.root, "dirty current", TWO);

    fs::write(
        fixture.root.join("state.bin"),
        b"unstaged-tracked\0unique\n",
    )
    .expect("write unstaged tracked bytes");
    fs::write(fixture.root.join("split.bin"), b"staged-only\0unique\n")
        .expect("write staged bytes");
    fixture.git(&fixture.root, &["add", "split.bin"]);
    fs::write(
        fixture.root.join("split.bin"),
        b"unstaged-over-staged\0unique\n",
    )
    .expect("write split worktree bytes");
    fs::write(fixture.root.join("untracked.bin"), b"untracked\0unique\n")
        .expect("write unique untracked bytes");

    let before = surface(&fixture, &fixture.root);
    let refused =
        fixture.jjk_json_failure(&fixture.root, &["return", state_id(&first), "--json"], 3);
    let diagnostic = refused.to_string().to_ascii_lowercase();
    assert!(
        diagnostic.contains("workspace")
            || diagnostic.contains("worktree")
            || diagnostic.contains("index"),
        "dirty refusal lacks a machine-readable workspace diagnosis: {refused}"
    );
    assert_eq!(
        surface(&fixture, &fixture.root),
        before,
        "dirty refusal changed a protected surface"
    );
    assert_eq!(
        fs::read(fixture.root.join("state.bin")).expect("preserved tracked bytes"),
        b"unstaged-tracked\0unique\n"
    );
    assert_eq!(
        fs::read(fixture.root.join("split.bin")).expect("preserved split bytes"),
        b"unstaged-over-staged\0unique\n"
    );
    assert_eq!(
        fs::read(fixture.root.join("untracked.bin")).expect("preserved untracked bytes"),
        b"untracked\0unique\n"
    );
    assert_git_valid(&fixture, &fixture.root);
}
