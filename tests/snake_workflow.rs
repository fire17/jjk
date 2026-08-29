use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const COLOR: &str = "color.txt";
const FAST: &str = "fast.txt";
const CONFLICT: &str = "conflict.txt";

struct Repository {
    _directory: TempDir,
    root: PathBuf,
    jjk: PathBuf,
}

impl Repository {
    fn new(files: &[(&str, &[u8])]) -> Self {
        let directory = TempDir::new().expect("create isolated repository");
        let root = directory.path().join("repository");
        fs::create_dir(&root).expect("create repository root");
        git_ok(&root, &["init", "-q", "-b", "main"]);
        git_ok(&root, &["config", "core.autocrlf", "false"]);
        git_ok(&root, &["config", "user.name", "JJK Snake Fixture"]);
        git_ok(
            &root,
            &["config", "user.email", "snake@jjk.example.invalid"],
        );
        for (path, bytes) in files {
            fs::write(root.join(path), bytes).expect("write deterministic fixture bytes");
        }
        git_ok(&root, &["add", "--all"]);
        git_ok(&root, &["commit", "-qm", "fixture substrate"]);

        let repository = Self {
            _directory: directory,
            root,
            jjk: assert_cmd::cargo::cargo_bin!("jjk").to_path_buf(),
        };
        repository.jjk_json_ok(&["setup", "--json"]);
        repository
    }

    fn write_and_stage(&self, path: &str, bytes: &[u8]) {
        fs::write(self.root.join(path), bytes).expect("write workflow bytes");
        git_ok(&self.root, &["add", "--", path]);
    }

    fn capture(&self, kind: &str, label: &str) -> Value {
        self.jjk_json_ok(&[kind, "--json", "--", label])
    }

    fn run_jjk(&self, args: &[&str]) -> Output {
        Command::new(&self.jjk)
            .args(args)
            .current_dir(&self.root)
            .output()
            .expect("run compiled jjk")
    }

    fn jjk_json_ok(&self, args: &[&str]) -> Value {
        let output = self.run_jjk(args);
        assert!(
            output.status.success(),
            "jjk {} failed\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        parse_json(&output)
    }

    fn current(&self) -> Value {
        self.jjk_json_ok(&["current", "--json"])
    }

    fn graph(&self) -> Value {
        self.jjk_json_ok(&["see", "--json"])
    }

    fn doctor(&self) -> Value {
        self.jjk_json_ok(&["doctor", "--json"])
    }

    fn return_to(&self, state: &Value) -> Value {
        self.jjk_json_ok(&["return", field(state, "state_id"), "--json"])
    }

    fn state_ref_oid(&self, state: &Value) -> String {
        let state_ref = format!("refs/jjk/states/{}^{{commit}}", field(state, "state_id"));
        git_text(&self.root, &["rev-parse", "--verify", &state_ref])
    }

    fn index_tree(&self) -> String {
        git_text(&self.root, &["write-tree"])
    }
}

#[test]
fn val_core_003_004_canonical_snake_preserves_futures_and_picks_only_the_atomic_delta() {
    let repository = Repository::new(&[(COLOR, b"color=seed\n"), (FAST, b"fast=false\n")]);

    repository.write_and_stage(COLOR, b"color=green\n");
    let green = repository.capture("step", "green");
    repository.write_and_stage(COLOR, b"color=purple\n");
    let purple = repository.capture("step", "purple");

    let purple_commit = field(&purple, "commit").to_owned();
    let purple_tree = git_text(
        &repository.root,
        &["rev-parse", &format!("{purple_commit}^{{tree}}")],
    );
    let purple_ref = repository.state_ref_oid(&purple);
    let purple_color = git_blob(&repository.root, &purple_commit, COLOR);
    let purple_fast = git_blob(&repository.root, &purple_commit, FAST);

    repository.return_to(&green);
    let graph_before_noop = repository.graph();
    let journal_before_noop = repository.doctor()["journal_events"]
        .as_u64()
        .expect("doctor exposes journal_events");
    let noop = repository.jjk_json_ok(&["save", "--json", "--", "still green"]);
    assert_eq!(
        noop["created"], false,
        "an unchanged capture must report an explicit no-op"
    );
    assert_eq!(field(&noop, "state_id"), field(&green, "state_id"));
    assert_eq!(
        repository.graph(),
        graph_before_noop,
        "no-op capture must create no semantic fact"
    );
    assert_eq!(
        repository.doctor()["journal_events"].as_u64(),
        Some(journal_before_noop),
        "no-op capture must create no journal event"
    );

    repository.write_and_stage(COLOR, b"color=orange\n");
    let orange = repository.capture("step", "orange");
    repository.return_to(&purple);
    repository.write_and_stage(FAST, b"fast=true\n");
    let fast_purple = repository.capture("step", "fast purple");

    let pre_pick_graph = repository.graph();
    assert_parent(&pre_pick_graph, &purple, &green);
    assert_parent(&pre_pick_graph, &orange, &green);
    assert_parent(&pre_pick_graph, &fast_purple, &purple);
    assert_eq!(
        field(&fast_purple, "attempt_id"),
        field(&purple, "attempt_id")
    );
    assert_ne!(
        field(&orange, "attempt_id"),
        field(&purple, "attempt_id"),
        "the first divergent capture after historical return must create a sibling attempt"
    );

    repository.return_to(&orange);
    let target_base_commit = field(&orange, "commit").to_owned();
    let source_patch_id = stable_patch_id(
        &repository.root,
        field(&purple, "commit"),
        field(&fast_purple, "commit"),
    );
    let states_before_pick = state_count(&pre_pick_graph);
    let pick = repository.jjk_json_ok(&["pick", field(&fast_purple, "state_id"), "--json"]);

    assert_eq!(field(&pick, "kind"), "cherry");
    assert_eq!(
        field(&pick, "source_state"),
        field(&fast_purple, "state_id")
    );
    assert_eq!(field(&pick, "source_parent"), field(&purple, "state_id"));
    assert_eq!(field(&pick, "target_base"), field(&orange, "state_id"));
    assert_eq!(field(&pick, "patch_id"), source_patch_id);
    assert!(
        pick.get("conflict_resolution").is_some(),
        "pick JSON must expose conflict_resolution"
    );
    assert!(
        pick["conflict_resolution"].is_null(),
        "a clean pick has no conflict resolution artifact"
    );
    assert_nonempty_field(&pick, "provenance_id");

    assert_eq!(
        fs::read(repository.root.join(COLOR)).expect("read final color"),
        b"color=orange\n"
    );
    assert_eq!(
        fs::read(repository.root.join(FAST)).expect("read final fast flag"),
        b"fast=true\n"
    );

    let result_commit = field(&pick, "commit");
    let applied_patch_id = stable_patch_id(&repository.root, &target_base_commit, result_commit);
    assert_eq!(
        applied_patch_id, source_patch_id,
        "the applied delta must have the source parent-to-state patch identity"
    );
    let source_paths = git_text(
        &repository.root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            field(&purple, "commit"),
            field(&fast_purple, "commit"),
        ],
    );
    let result_paths = git_text(
        &repository.root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            &target_base_commit,
            result_commit,
        ],
    );
    assert_eq!(source_paths, FAST);
    assert_eq!(
        result_paths, source_paths,
        "pick must change exactly the source delta's paths"
    );
    assert_eq!(
        git_text(
            &repository.root,
            &["ls-tree", "-r", "--name-only", result_commit]
        ),
        format!("{COLOR}\n{FAST}"),
        "the result tree must contain only the target files"
    );
    assert_eq!(field(&pick, "patch_id"), applied_patch_id);
    assert_exact_parent(&repository.root, result_commit, &target_base_commit);
    assert_not_ancestor(&repository.root, &purple_commit, result_commit);
    assert_not_ancestor(
        &repository.root,
        field(&fast_purple, "commit"),
        result_commit,
    );

    let graph_after_pick = repository.graph();
    assert_eq!(state_count(&graph_after_pick), states_before_pick + 1);
    assert_parent(&graph_after_pick, &pick, &orange);
    assert_eq!(
        field(state(&graph_after_pick, field(&pick, "state_id")), "kind"),
        "cherry"
    );
    assert_eq!(
        field(&repository.current(), "state_id"),
        field(&pick, "state_id")
    );

    assert_eq!(
        repository.state_ref_oid(&purple),
        purple_ref,
        "pick must not move the purple source state ref"
    );
    assert_eq!(
        git_text(
            &repository.root,
            &[
                "rev-parse",
                &format!("{}^{{tree}}", field(&purple, "commit"))
            ]
        ),
        purple_tree,
        "pick must not rewrite the purple source tree"
    );
    assert_eq!(
        git_blob(&repository.root, &purple_commit, COLOR),
        purple_color
    );
    assert_eq!(
        git_blob(&repository.root, &purple_commit, FAST),
        purple_fast
    );
    assert_eq!(purple_color, b"color=purple\n");
    assert_eq!(purple_fast, b"fast=false\n");

    for saved in [&green, &purple, &orange, &fast_purple, &pick] {
        let oid = repository.state_ref_oid(saved);
        assert_eq!(
            oid,
            field(saved, "commit"),
            "every semantic future must retain a Git ref"
        );
        git_ok(
            &repository.root,
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        );
    }
    assert_exact_parent(
        &repository.root,
        field(&purple, "commit"),
        field(&green, "commit"),
    );
    assert_exact_parent(
        &repository.root,
        field(&orange, "commit"),
        field(&green, "commit"),
    );
    assert_exact_parent(
        &repository.root,
        field(&fast_purple, "commit"),
        field(&purple, "commit"),
    );

    git_ok(&repository.root, &["fsck", "--full", "--strict"]);

    repository.return_to(&green);
    git_ok(
        &repository.root,
        &["reset", "--hard", field(&green, "commit")],
    );
    let graph_before_ambiguous_down = repository.graph();
    let index_before_ambiguous_down = repository.index_tree();
    let ambiguous = repository.run_jjk(&["down", "--json"]);
    assert_eq!(
        ambiguous.status.code(),
        Some(2),
        "ambiguous input uses the stable usage exit code"
    );
    assert!(
        !ambiguous.status.success(),
        "down with purple and orange children must require an exact choice"
    );
    let ambiguous_json = parse_json(&ambiguous);
    let diagnostic = machine_diagnostic(&ambiguous_json);
    assert!(
        diagnostic.contains("ambiguous") || diagnostic.contains("logical children"),
        "ambiguity must be diagnosed, got: {diagnostic}"
    );
    assert!(
        json_mentions(&ambiguous_json, field(&purple, "state_id")),
        "ambiguity output must expose the purple candidate"
    );
    assert!(
        json_mentions(&ambiguous_json, field(&orange, "state_id")),
        "ambiguity output must expose the orange candidate"
    );
    assert_eq!(
        repository.graph(),
        graph_before_ambiguous_down,
        "ambiguous navigation must not mutate the graph"
    );
    assert_eq!(
        repository.index_tree(),
        index_before_ambiguous_down,
        "ambiguous navigation must not mutate the index"
    );
    assert_eq!(
        fs::read(repository.root.join(COLOR)).expect("read green after refusal"),
        b"color=green\n"
    );
    assert_eq!(
        fs::read(repository.root.join(FAST)).expect("read green fast flag"),
        b"fast=false\n"
    );
}

#[test]
fn conflicting_pick_preserves_symbolic_head_and_mixed_dirty_workspace() {
    assert_conflicting_pick_preserves_complete_preimage(false);
}

#[test]
fn conflicting_pick_preserves_detached_head_and_mixed_dirty_workspace() {
    assert_conflicting_pick_preserves_complete_preimage(true);
}

fn assert_conflicting_pick_preserves_complete_preimage(detach_head: bool) {
    let repository = Repository::new(&[
        (CONFLICT, b"mode=seed\n"),
        ("staged.bin", b"staged base\0\x01\n"),
        ("unstaged.txt", b"unstaged base\n"),
    ]);

    repository.write_and_stage(CONFLICT, b"mode=green\n");
    let green = repository.capture("step", "conflict green");
    repository.write_and_stage(CONFLICT, b"mode=purple\n");
    let purple = repository.capture("step", "conflict purple");
    repository.return_to(&green);
    repository.write_and_stage(CONFLICT, b"mode=orange\n");
    let orange = repository.capture("step", "conflict orange");

    if detach_head {
        git_ok(&repository.root, &["checkout", "--detach", "-q", "HEAD"]);
    }
    git_ok(
        &repository.root,
        &["symbolic-ref", "refs/jjk/test-symbolic", "refs/heads/main"],
    );
    repository.write_and_stage("staged.bin", b"staged dirty\0\xfe\n");
    fs::write(repository.root.join("unstaged.txt"), b"unstaged dirty\r\n")
        .expect("write unstaged bytes");
    fs::write(
        repository.root.join("untracked.bin"),
        b"untracked\0\xff\x10\n",
    )
    .expect("write untracked bytes");

    let source_patch_id = stable_patch_id(
        &repository.root,
        field(&green, "commit"),
        field(&purple, "commit"),
    );
    let graph_before = repository.graph();
    let current_before = repository.current();
    let control_before = GitControlFingerprint::capture(&repository.root);
    let source_ref_before = repository.state_ref_oid(&purple);
    let target_ref_before = repository.state_ref_oid(&orange);

    let conflict = repository.run_jjk(&["pick", field(&purple, "state_id"), "--json"]);
    assert!(
        !conflict.status.success(),
        "an unresolved pick conflict must not report success"
    );
    assert_eq!(
        conflict.status.code(),
        Some(4),
        "an unresolved conflict uses the stable conflict exit code"
    );
    let conflict = parse_json(&conflict);
    assert_eq!(field(&conflict, "status"), "awaiting_resolution");
    assert_eq!(field(&conflict, "source_state"), field(&purple, "state_id"));
    assert_eq!(field(&conflict, "source_parent"), field(&green, "state_id"));
    assert_eq!(field(&conflict, "target_base"), field(&orange, "state_id"));
    assert_eq!(field(&conflict, "patch_id"), source_patch_id);
    assert_nonempty_field(&conflict, "composition_id");
    assert_nonempty_field(&conflict, "operation_id");
    assert_nonempty_field(&conflict, "conflict_artifact");
    assert!(
        conflict["conflicting_paths"]
            .as_array()
            .is_some_and(|paths| paths.iter().any(|path| path.as_str() == Some(CONFLICT))),
        "conflict output must identify the exact conflicting path"
    );
    assert!(
        conflict.get("state_id").is_none() || conflict["state_id"].is_null(),
        "a paused conflict must not manufacture a result state"
    );

    let artifact = PathBuf::from(field(&conflict, "conflict_artifact"));
    assert!(
        artifact.is_file(),
        "the conflict artifact must outlive the failed pick process"
    );
    let artifact_receipt: Value =
        serde_json::from_slice(&fs::read(&artifact).expect("read durable conflict artifact"))
            .expect("conflict artifact is a machine-readable receipt");
    assert_eq!(
        field(&artifact_receipt, "operation_id"),
        field(&conflict, "operation_id")
    );
    assert_eq!(field(&artifact_receipt, "status"), "awaiting_resolution");
    assert_eq!(
        field(&artifact_receipt, "source_state"),
        field(&purple, "state_id")
    );
    assert_eq!(
        field(&artifact_receipt, "source_parent"),
        field(&green, "state_id")
    );
    assert_eq!(
        field(&artifact_receipt, "target_base"),
        field(&orange, "state_id")
    );
    assert_eq!(field(&artifact_receipt, "patch_id"), source_patch_id);
    assert_eq!(
        artifact_receipt["preimage"],
        control_before.to_json(),
        "the receipt must bind the complete live control preimage"
    );

    let status = repository.jjk_json_ok(&["status", "--json"]);
    assert_eq!(status["recovery_required"], true);
    let pending = status["pending_operations"]
        .as_array()
        .expect("status exposes pending_operations");
    assert!(
        pending.iter().any(|operation| {
            operation.get("operation_id").and_then(Value::as_str)
                == Some(field(&conflict, "operation_id"))
                && operation.get("status").and_then(Value::as_str) == Some("awaiting_resolution")
                && operation.get("conflict_artifact").and_then(Value::as_str)
                    == Some(field(&conflict, "conflict_artifact"))
        }),
        "status must expose the durable conflict operation receipt: {status}"
    );

    let inspected =
        repository.jjk_json_ok(&["recover", field(&conflict, "operation_id"), "--json"]);
    assert_eq!(
        field(&inspected, "operation_id"),
        field(&conflict, "operation_id")
    );
    assert_eq!(field(&inspected, "status"), "awaiting_resolution");
    assert_eq!(
        field(&inspected, "conflict_artifact"),
        field(&conflict, "conflict_artifact")
    );
    assert_eq!(
        GitControlFingerprint::capture(&repository.root),
        control_before,
        "conflict inspection must be read-only"
    );

    let recovered = repository.jjk_json_ok(&[
        "recover",
        field(&conflict, "operation_id"),
        "--abort",
        "--json",
    ]);
    assert_eq!(
        field(&recovered, "operation_id"),
        field(&conflict, "operation_id")
    );
    assert_eq!(field(&recovered, "status"), "aborted");
    assert_eq!(
        field(&recovered, "conflict_artifact"),
        field(&conflict, "conflict_artifact")
    );
    assert_eq!(field(&recovered, "next_action"), "retry_pick");
    let recovered_status = repository.jjk_json_ok(&["status", "--json"]);
    assert_eq!(
        recovered_status["recovery_required"], false,
        "aborting the conflict must clear recovery mode"
    );
    assert!(
        recovered_status["pending_operations"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "aborted operations must leave the pending set: {recovered_status}"
    );

    assert_eq!(
        GitControlFingerprint::capture(&repository.root),
        control_before,
        "pick conflict and explicit abort must preserve the complete live Git preimage byte-for-byte"
    );
    assert!(
        git_bytes(&repository.root, &["ls-files", "-u"]).is_empty(),
        "the active target index must contain no conflict stages"
    );
    assert_eq!(
        without_projection_version(repository.current()),
        without_projection_version(current_before),
        "a paused pick must not advance current state"
    );
    assert_eq!(
        without_projection_version(repository.graph()),
        without_projection_version(graph_before),
        "a paused pick must not create a semantic result"
    );
    assert_eq!(
        repository.state_ref_oid(&purple),
        source_ref_before,
        "conflict must not move source state"
    );
    assert_eq!(
        repository.state_ref_oid(&orange),
        target_ref_before,
        "conflict must not move target state"
    );
    git_ok(&repository.root, &["fsck", "--full", "--strict"]);
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GitControlFingerprint {
    head_attachment: Vec<u8>,
    head_oid: Vec<u8>,
    refs: Vec<u8>,
    index: Vec<u8>,
    index_stages: Vec<u8>,
    status: Vec<u8>,
    tracked: Vec<(String, Vec<u8>)>,
    untracked: Vec<(String, Vec<u8>)>,
}

impl GitControlFingerprint {
    fn capture(root: &Path) -> Self {
        let head_attachment =
            git_bytes_allow_status(root, &["symbolic-ref", "-q", "HEAD"], &[0, 1]);
        let head_oid = git_bytes(root, &["rev-parse", "--verify", "HEAD"]);
        let refs = git_bytes(
            root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)%00",
            ],
        );
        let index_stages = git_bytes(root, &["ls-files", "--stage", "-z"]);
        let status = git_bytes(root, &["status", "--porcelain=v2", "-z"]);
        let index = fs::read(root.join(".git/index")).expect("read exact Git index bytes");
        let tracked = git_paths(root, &["ls-files", "-z"])
            .into_iter()
            .map(|path| {
                let bytes = fs::read(root.join(&path)).expect("read tracked worktree path");
                (path, bytes)
            })
            .collect();
        let untracked = git_paths(root, &["ls-files", "--others", "--exclude-standard", "-z"])
            .into_iter()
            .map(|path| {
                let bytes = fs::read(root.join(&path)).expect("read untracked worktree path");
                (path, bytes)
            })
            .collect();
        Self {
            head_attachment,
            head_oid,
            refs,
            index,
            index_stages,
            status,
            tracked,
            untracked,
        }
    }

    fn to_json(&self) -> Value {
        serde_json::json!({
            "head_attachment_hex": hex_bytes(&self.head_attachment),
            "head_oid_hex": hex_bytes(&self.head_oid),
            "refs_hex": hex_bytes(&self.refs),
            "index_sha256": hex_bytes(&Sha256::digest(&self.index)),
            "index_stages_hex": hex_bytes(&self.index_stages),
            "status_hex": hex_bytes(&self.status),
            "tracked": self.tracked.iter().map(|(path, bytes)| serde_json::json!({"path": path, "bytes_hex": hex_bytes(bytes)})).collect::<Vec<_>>(),
            "untracked": self.untracked.iter().map(|(path, bytes)| serde_json::json!({"path": path, "bytes_hex": hex_bytes(bytes)})).collect::<Vec<_>>(),
        })
    }
}

fn git_paths(root: &Path, args: &[&str]) -> Vec<String> {
    let bytes = git_bytes(root, args);
    let mut paths = bytes
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).expect("fixture path is UTF-8"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn git_bytes_allow_status(root: &Path, args: &[&str], accepted: &[i32]) -> Vec<u8> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run Git");
    assert!(
        accepted.contains(&output.status.code().unwrap_or(-1)),
        "git {} returned unexpected status {:?}: {}",
        args.join(" "),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn hex_bytes(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn parse_json(output: &Output) -> Value {
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!(
            "command did not emit JSON: {error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn without_projection_version(mut value: Value) -> Value {
    if let Value::Object(object) = &mut value {
        object.remove("projection_version");
    }
    value
}

fn field<'a>(value: &'a Value, name: &str) -> &'a str {
    value
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("JSON field `{name}` must be a string: {value}"))
}

fn assert_nonempty_field(value: &Value, name: &str) {
    assert!(
        !field(value, name).is_empty(),
        "JSON field `{name}` must not be empty"
    );
}

fn state_count(graph: &Value) -> usize {
    graph["states"]
        .as_array()
        .expect("see JSON exposes states")
        .len()
}

fn state<'a>(graph: &'a Value, state_id: &str) -> &'a Value {
    graph["states"]
        .as_array()
        .expect("see JSON exposes states")
        .iter()
        .find(|candidate| candidate.get("state_id").and_then(Value::as_str) == Some(state_id))
        .unwrap_or_else(|| panic!("state {state_id} is visible in graph: {graph}"))
}

fn assert_parent(graph: &Value, child: &Value, parent: &Value) {
    assert_eq!(
        field(state(graph, field(child, "state_id")), "logical_parent"),
        field(parent, "state_id")
    );
}

fn machine_diagnostic(value: &Value) -> &str {
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("machine failure must expose a diagnostic: {value}"))
}

fn json_mentions(value: &Value, needle: &str) -> bool {
    serde_json::to_string(value)
        .expect("serialize observed JSON")
        .contains(needle)
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {} failed\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_bytes(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run Git");
    assert!(
        output.status.success(),
        "git {} failed\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn git_text(root: &Path, args: &[&str]) -> String {
    String::from_utf8(git_bytes(root, args))
        .expect("Git text is UTF-8")
        .trim()
        .to_owned()
}

fn git_blob(root: &Path, commit: &str, path: &str) -> Vec<u8> {
    git_bytes(root, &["show", &format!("{commit}:{path}")])
}

fn stable_patch_id(root: &Path, from: &str, to: &str) -> String {
    let patch = git_bytes(
        root,
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            from,
            to,
            "--",
        ],
    );
    assert!(
        !patch.is_empty(),
        "the selected parent-to-state delta must not be empty"
    );
    let mut child = Command::new("git")
        .args(["patch-id", "--stable"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn git patch-id");
    child
        .stdin
        .take()
        .expect("patch-id stdin")
        .write_all(&patch)
        .expect("send exact patch");
    let output = child.wait_with_output().expect("wait for git patch-id");
    assert!(
        output.status.success(),
        "git patch-id failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("patch-id is UTF-8");
    let patch_id = text
        .split_whitespace()
        .next()
        .expect("patch-id output contains identity");
    assert_eq!(
        patch_id.len(),
        40,
        "SHA-1 fixture uses a 40-hex stable patch ID"
    );
    assert!(
        patch_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    );
    patch_id.to_owned()
}

fn assert_exact_parent(root: &Path, commit: &str, parent: &str) {
    let line = git_text(root, &["rev-list", "--parents", "-n", "1", commit]);
    let fields = line.split_whitespace().collect::<Vec<_>>();
    assert_eq!(
        fields,
        [commit, parent],
        "state commit must have exactly the declared sole parent"
    );
}

fn assert_not_ancestor(root: &Path, possible_ancestor: &str, descendant: &str) {
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", possible_ancestor, descendant])
        .current_dir(root)
        .output()
        .expect("run Git ancestry query");
    assert_eq!(
        output.status.code(),
        Some(1),
        "{possible_ancestor} must not leak into ancestry of {descendant}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
