use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::TempDir;

const PROCESS_DEADLINE: Duration = Duration::from_secs(60);
const BARRIER_DEADLINE: Duration = Duration::from_secs(30);
const WORKER_ENV: &str = "JJK_CONCURRENCY_CAPTURE_WORKER";

#[derive(Debug)]
struct Fixture {
    _directory: TempDir,
    root: PathBuf,
    home: PathBuf,
    global_config: PathBuf,
    jjk: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
struct CaptureReceipt {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl CaptureReceipt {
    fn success(&self) -> bool {
        self.code == Some(0)
    }
    fn diagnostic(&self) -> String {
        format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&self.stdout),
            String::from_utf8_lossy(&self.stderr)
        )
    }
}

#[test]
fn concurrency_capture_worker() {
    if env::var_os(WORKER_ENV).is_none() {
        return;
    }
    let ready = required_env_path("JJK_CONCURRENCY_READY");
    let release = required_env_path("JJK_CONCURRENCY_RELEASE");
    let receipt = required_env_path("JJK_CONCURRENCY_RECEIPT");
    fs::write(&ready, b"ready\n").expect("publish worker readiness");
    wait_for_path(&release, BARRIER_DEADLINE, "capture release barrier");

    let label = env::var("JJK_CAPTURE_LABEL").expect("JJK_CAPTURE_LABEL");
    let output = configured_command(Path::new(&env::var_os("JJK_BIN").expect("JJK_BIN")))
        .current_dir(required_env_path("JJK_WORKTREE"))
        .args(["step", "--json", "--", &label])
        .output()
        .expect("execute compiled jjk capture");
    let captured = CaptureReceipt {
        code: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    };
    fs::write(
        receipt,
        serde_json::to_vec(&captured).expect("encode capture receipt"),
    )
    .expect("persist capture receipt");
}

#[test]
fn linked_worktree_captures_share_one_store_and_preserve_both_states() {
    let fixture = fixture();
    let linked = fixture
        .root
        .parent()
        .expect("fixture parent")
        .join("linked workspace");
    git_success(
        &fixture,
        &fixture.root,
        &["worktree", "add", "-q", "-b", "linked", path_text(&linked)],
    );

    let primary_setup = json_result(&jjk_success(&fixture, &fixture.root, &["setup", "--json"]));
    let linked_setup = json_result(&jjk_success(&fixture, &linked, &["setup", "--json"]));
    assert_eq!(
        primary_setup["repository_id"],
        linked_setup["repository_id"]
    );
    assert_eq!(primary_setup["store"], linked_setup["store"]);

    fs::write(fixture.root.join("left.txt"), "primary capture\n").expect("modify primary");
    git_success(&fixture, &fixture.root, &["add", "left.txt"]);
    fs::write(linked.join("right.txt"), "linked capture\n").expect("modify linked");
    git_success(&fixture, &linked, &["add", "right.txt"]);

    let captures = concurrent_captures(
        &fixture,
        [
            (&fixture.root, "primary writer"),
            (&linked, "linked writer"),
        ],
    );
    for capture in &captures {
        assert_no_sqlite_busy(capture);
        assert!(
            capture.success(),
            "linked-worktree capture failed: {}",
            capture.diagnostic()
        );
    }

    let primary = receipt_result(&captures[0]);
    let sibling = receipt_result(&captures[1]);
    let primary_id = state_id(&primary);
    let sibling_id = state_id(&sibling);
    assert_ne!(
        primary_id, sibling_id,
        "separate captures reused one stable state ID"
    );
    assert_ne!(
        primary["commit"], sibling["commit"],
        "different worktree trees collapsed to one commit"
    );
    assert_eq!(
        git_text(
            &fixture,
            &fixture.root,
            &["show", &format!("{}:left.txt", string(&primary, "commit"))]
        ),
        "primary capture\n"
    );
    assert_eq!(
        git_text(
            &fixture,
            &fixture.root,
            &["show", &format!("{}:right.txt", string(&sibling, "commit"))]
        ),
        "linked capture\n"
    );
    assert_states_and_refs_survive(&fixture, &[primary_id, sibling_id]);
    assert_git_fsck(&fixture);
}

#[test]
fn same_worktree_conflicting_writers_are_serialized_or_return_typed_conflict() {
    let fixture = fixture();
    jjk_success(&fixture, &fixture.root, &["setup", "--json"]);
    fs::write(fixture.root.join("left.txt"), "shared staged tree\n").expect("modify shared tree");
    git_success(&fixture, &fixture.root, &["add", "left.txt"]);

    let captures = concurrent_captures(
        &fixture,
        [
            (&fixture.root, "same tree alpha"),
            (&fixture.root, "same tree beta"),
        ],
    );
    for capture in &captures {
        assert_no_sqlite_busy(capture);
    }

    let successful = captures
        .iter()
        .filter(|capture| capture.success())
        .collect::<Vec<_>>();
    assert!(!successful.is_empty(), "both conflicting writers failed");
    let successful_ids = successful
        .iter()
        .map(|capture| state_id(&receipt_result(capture)).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        successful_ids.iter().collect::<HashSet<_>>().len(),
        successful_ids.len(),
        "successful captures must have distinct stable IDs"
    );

    for capture in captures.iter().filter(|capture| !capture.success()) {
        assert_eq!(
            capture.code,
            Some(4),
            "writer did not return the stable conflict exit: {}",
            capture.diagnostic()
        );
        let envelope = receipt_json(capture);
        let error = envelope
            .get("error")
            .filter(|value| value.is_object())
            .unwrap_or_else(|| {
                panic!(
                    "conflict was not a typed JSON error: {}",
                    capture.diagnostic()
                )
            });
        assert!(
            !string(error, "code").is_empty(),
            "typed conflict code is empty"
        );
        assert!(
            !string(error, "message").is_empty(),
            "typed conflict message is empty"
        );
        assert_eq!(
            error["retryable"], true,
            "same-worktree writer conflict must be retryable"
        );
    }

    let successful_ids = successful_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_states_and_refs_survive(&fixture, &successful_ids);
    assert_git_fsck(&fixture);
}

#[test]
fn reopened_queries_advance_past_stale_observations_with_consistent_projections() {
    let fixture = fixture();
    let setup = json_result(&jjk_success(&fixture, &fixture.root, &["setup", "--json"]));
    let database = PathBuf::from(string(&setup, "store"));
    let imported = state_ids(&json_result(&jjk_success(
        &fixture,
        &fixture.root,
        &["see", "--json"],
    )));

    fs::write(fixture.root.join("left.txt"), "first projection\n").expect("first change");
    git_success(&fixture, &fixture.root, &["add", "left.txt"]);
    let first = json_result(&jjk_success(
        &fixture,
        &fixture.root,
        &["step", "--json", "--", "first projection"],
    ));
    let stale_view = json_result(&jjk_success(&fixture, &fixture.root, &["see", "--json"]));
    let mut expected_stale = imported.clone();
    expected_stale.insert(state_id(&first).to_owned());
    assert_eq!(state_ids(&stale_view), expected_stale);

    fs::write(fixture.root.join("right.txt"), "second projection\n").expect("second change");
    git_success(&fixture, &fixture.root, &["add", "right.txt"]);
    let second = json_result(&jjk_success(
        &fixture,
        &fixture.root,
        &["nice", "--json", "--", "second projection"],
    ));

    let reopened_output = jjk_success(&fixture, &fixture.root, &["see", "--json"]);
    let reopened_envelope = output_json(&reopened_output);
    let reopened = json_result(&reopened_output);
    let mut expected = imported;
    expected.insert(state_id(&first).to_owned());
    expected.insert(state_id(&second).to_owned());
    assert_eq!(
        state_ids(&reopened),
        expected,
        "fresh process served a stale state projection"
    );
    let current_output = jjk_success(&fixture, &fixture.root, &["current", "--json"]);
    let current_envelope = output_json(&current_output);
    let current = json_result(&current_output);
    assert_eq!(state_id(&current), state_id(&second));
    let doctor = json_result(&jjk_success(&fixture, &fixture.root, &["doctor", "--json"]));
    assert_eq!(doctor["healthy"], true);
    let head_seq = assert_projection_watermarks(&database);
    assert_eq!(
        projection_version(&reopened_envelope),
        head_seq,
        "graph was not bound to the reopened journal head"
    );
    assert_eq!(
        projection_version(&current_envelope),
        head_seq,
        "current was not bound to the same reopened journal head"
    );
    assert_git_fsck(&fixture);
}

#[cfg(unix)]
#[test]
fn reachable_capture_crash_is_durably_visible_to_doctor_after_reopen() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;

    let fixture = fixture();
    jjk_success(&fixture, &fixture.root, &["setup", "--json"]);
    fs::write(fixture.root.join("left.txt"), "crash boundary\n").expect("crash fixture change");
    git_success(&fixture, &fixture.root, &["add", "left.txt"]);

    let control = fixture
        .root
        .parent()
        .expect("fixture parent")
        .join("crash-control");
    let shim_dir = control.join("bin");
    fs::create_dir_all(&shim_dir).expect("shim directory");
    let ready = control.join("before-update-ref");
    let never_release = control.join("release-update-ref");
    let real_git = executable_on_path("git");
    let shim = shim_dir.join("git");
    fs::write(&shim, b"#!/bin/sh\nif [ \"$1\" = update-ref ] && [ -n \"${JJK_CRASH_READY:-}\" ]; then\n  : > \"$JJK_CRASH_READY\"\n  while [ ! -e \"$JJK_CRASH_RELEASE\" ]; do\n    kill -0 \"$PPID\" 2>/dev/null || exit 137\n  done\nfi\nexec \"$JJK_REAL_GIT\" \"$@\"\n").expect("write Git boundary shim");
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755))
        .expect("make Git shim executable");

    let inherited_path = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![shim_dir];
    paths.extend(env::split_paths(&inherited_path));
    let mut command = fixture_command(&fixture, &fixture.jjk);
    command
        .current_dir(&fixture.root)
        .args(["step", "--json", "--", "reachable crash"])
        .env("PATH", env::join_paths(paths).expect("shim PATH"))
        .env("JJK_REAL_GIT", real_git)
        .env("JJK_CRASH_READY", &ready)
        .env("JJK_CRASH_RELEASE", &never_release)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().expect("spawn crash-boundary capture");
    wait_for_path(&ready, BARRIER_DEADLINE, "Git update-ref boundary");
    kill_process_group(&mut child);
    let crashed = wait_bounded(child, PROCESS_DEADLINE, "crashed capture");
    assert!(
        !crashed.status.success(),
        "capture unexpectedly crossed the crash barrier"
    );

    let first = run_jjk(&fixture, &fixture.root, &["doctor", "--json"]);
    assert_no_sqlite_busy_output(&first);
    assert_eq!(
        first.status.code(),
        Some(5),
        "doctor did not return recovery-required after crash: {}",
        output_diagnostic(&first)
    );
    let first_pending = pending_operations(&output_json(&first));
    assert!(
        !first_pending.is_empty(),
        "doctor did not expose the durable nonterminal operation"
    );

    let second = run_jjk(&fixture, &fixture.root, &["doctor", "--json"]);
    assert_no_sqlite_busy_output(&second);
    assert_eq!(
        second.status.code(),
        Some(5),
        "reopened doctor lost recovery-required status: {}",
        output_diagnostic(&second)
    );
    let second_pending = pending_operations(&output_json(&second));
    assert_eq!(
        first_pending, second_pending,
        "reopen changed the identity or phase of pending recovery work"
    );
    assert_git_fsck(&fixture);
}

#[derive(Clone, Copy, Debug)]
struct FailpointCase {
    name: &'static str,
    exit: i32,
    error_code: &'static str,
    operation_status: Option<&'static str>,
    events: &'static [&'static str],
    state_ref_exists: bool,
    state_is_committed: bool,
    recovery_required: bool,
}

#[test]
fn capture_failpoints_preserve_exact_pre_or_discoverable_repair_state() {
    const CASES: &[FailpointCase] = &[
        FailpointCase {
            name: "FP-0-before-prepare",
            exit: 3,
            error_code: "UNAVAILABLE",
            operation_status: None,
            events: &[],
            state_ref_exists: false,
            state_is_committed: false,
            recovery_required: false,
        },
        FailpointCase {
            name: "FP-1-after-prepare-before-first-effect",
            exit: 3,
            error_code: "UNAVAILABLE",
            operation_status: Some("aborted"),
            events: &["OperationPrepared", "AbortStarted", "OperationAborted"],
            state_ref_exists: false,
            state_is_committed: false,
            recovery_required: false,
        },
        FailpointCase {
            name: "FP-2-after-each-effect",
            exit: 5,
            error_code: "RECOVERY_REQUIRED",
            operation_status: Some("repair_required"),
            events: &["OperationPrepared", "ApplyStarted", "RepairRequired"],
            state_ref_exists: true,
            state_is_committed: false,
            recovery_required: true,
        },
        FailpointCase {
            name: "FP-3-before-verify",
            exit: 5,
            error_code: "RECOVERY_REQUIRED",
            operation_status: Some("repair_required"),
            events: &[
                "OperationPrepared",
                "ApplyStarted",
                "VerificationStarted",
                "RepairRequired",
            ],
            state_ref_exists: true,
            state_is_committed: false,
            recovery_required: true,
        },
        FailpointCase {
            name: "FP-4-after-verify-before-commit",
            exit: 5,
            error_code: "RECOVERY_REQUIRED",
            operation_status: Some("repair_required"),
            events: &[
                "OperationPrepared",
                "ApplyStarted",
                "VerificationStarted",
                "RepairRequired",
            ],
            state_ref_exists: true,
            state_is_committed: false,
            recovery_required: true,
        },
        FailpointCase {
            name: "FP-5-commit-ambiguity",
            exit: 3,
            error_code: "UNAVAILABLE",
            operation_status: Some("committed"),
            events: &[
                "OperationPrepared",
                "ApplyStarted",
                "VerificationStarted",
                "StateCaptured",
                "OperationCommitted",
            ],
            state_ref_exists: true,
            state_is_committed: true,
            recovery_required: false,
        },
    ];

    for case in CASES {
        let fixture = fixture();
        let setup = json_result(&jjk_success(&fixture, &fixture.root, &["setup", "--json"]));
        let database = PathBuf::from(string(&setup, "store"));
        let imported_state_ids = state_ids(&json_result(&jjk_success(
            &fixture,
            &fixture.root,
            &["see", "--json"],
        )));
        fs::write(
            fixture.root.join("fault.txt"),
            format!("deterministic bytes for {}\n", case.name),
        )
        .expect("write failpoint fixture");
        git_success(&fixture, &fixture.root, &["add", "fault.txt"]);
        let head_before = git_text(&fixture, &fixture.root, &["rev-parse", "HEAD"]);
        let status_before = git_success(
            &fixture,
            &fixture.root,
            &["status", "--porcelain=v2", "--branch"],
        )
        .stdout;
        let refs_before = state_refs(&fixture);
        let database_before = journal_shape(&database);

        let output = fixture_command(&fixture, &fixture.jjk)
            .current_dir(&fixture.root)
            .env("JJK_FAILPOINT", case.name)
            .args(["step", "--json", "--", case.name])
            .output()
            .expect("run failpoint capture");
        assert_no_sqlite_busy_output(&output);
        assert_eq!(
            output.status.code(),
            Some(case.exit),
            "{} returned the wrong exit: {}",
            case.name,
            output_diagnostic(&output)
        );
        let error = error_json(&output);
        assert_eq!(
            error["error"]["code"], case.error_code,
            "{} returned the wrong typed failure: {error}",
            case.name
        );

        let connection = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open failpoint store read-only");
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("SQLite integrity check");
        assert_eq!(integrity, "ok", "{} damaged SQLite", case.name);
        let operations = operation_lifecycles(&connection);
        match case.operation_status {
            None => assert!(
                operations.is_empty(),
                "{} crossed the prepare boundary: {operations:?}",
                case.name
            ),
            Some(expected_status) => {
                assert_eq!(
                    operations.len(),
                    1,
                    "{} did not leave exactly one durable operation: {operations:?}",
                    case.name
                );
                let (operation_id, status, events) = &operations[0];
                assert_eq!(
                    status, expected_status,
                    "{} persisted the wrong lifecycle status",
                    case.name
                );
                assert_eq!(
                    events.iter().map(String::as_str).collect::<Vec<_>>(),
                    case.events,
                    "{} persisted the wrong event sequence",
                    case.name
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.as_str() == "OperationCommitted")
                        .count(),
                    usize::from(case.state_is_committed)
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.as_str() == "OperationAborted")
                        .count(),
                    usize::from(expected_status == "aborted")
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| event.as_str() == "StateCaptured")
                        .count(),
                    usize::from(case.state_is_committed)
                );
                assert_eq!(
                    operation_id.len(),
                    32,
                    "operation ID must be the full durable UUID bytes"
                );
                assert_operation_recovery_material(&connection, operation_id);
            }
        }

        let refs_after = state_refs(&fixture);
        assert_eq!(
            refs_after.len(),
            usize::from(case.state_ref_exists),
            "{} left the wrong number of owned state refs: {refs_after:?}",
            case.name
        );
        assert_eq!(
            git_text(&fixture, &fixture.root, &["rev-parse", "HEAD"]),
            head_before,
            "{} moved the user's HEAD",
            case.name
        );
        assert_eq!(
            git_success(
                &fixture,
                &fixture.root,
                &["status", "--porcelain=v2", "--branch"]
            )
            .stdout,
            status_before,
            "{} changed the user's index or worktree",
            case.name
        );
        if !case.state_ref_exists {
            assert_eq!(
                refs_after, refs_before,
                "{} left an external effect before its declared boundary",
                case.name
            );
        }

        let graph = json_result(&jjk_success(&fixture, &fixture.root, &["see", "--json"]));
        assert_eq!(
            state_ids(&graph).len(),
            imported_state_ids.len() + usize::from(case.state_is_committed),
            "{} leaked an uncommitted state into the graph",
            case.name
        );
        if case.state_is_committed {
            let only_state = graph["states"]
                .as_array()
                .expect("states array")
                .iter()
                .find(|state| !imported_state_ids.contains(state_id(state)))
                .expect("committed captured state");
            let state_ref = format!("refs/jjk/states/{}", state_id(only_state));
            assert!(
                refs_after
                    .iter()
                    .any(|(name, oid)| name == &state_ref && oid == string(only_state, "commit")),
                "committed graph state {only_state} expected {state_ref}, but Git refs are {refs_after:?}"
            );
        }

        for command in ["status", "doctor"] {
            let reopened = run_jjk(&fixture, &fixture.root, &[command, "--json"]);
            assert_no_sqlite_busy_output(&reopened);
            let expected_exit = if case.recovery_required { 5 } else { 0 };
            assert_eq!(
                reopened.status.code(),
                Some(expected_exit),
                "{command} did not report {} after reopen: {}",
                case.name,
                output_diagnostic(&reopened)
            );
            let value = output_json(&reopened);
            let result = value
                .get("result")
                .filter(|result| !result.is_null())
                .unwrap_or(&value);
            assert_eq!(
                result["recovery_required"].as_bool().unwrap_or(false),
                case.recovery_required,
                "{command} recovery flag disagrees for {}",
                case.name
            );
            let pending = pending_operations(result);
            if case.recovery_required {
                assert_eq!(
                    pending.len(),
                    1,
                    "{command} did not expose exactly one repair operation for {}: {value}",
                    case.name
                );
                assert_eq!(
                    pending.iter().next().expect("pending operation").1,
                    "repair_required"
                );
            } else {
                assert!(
                    pending.is_empty(),
                    "{command} exposed terminal work as pending for {}: {value}",
                    case.name
                );
            }
        }

        assert_eq!(
            operation_lifecycles(&connection),
            operations,
            "read-only reopen duplicated or rewrote lifecycle facts for {}",
            case.name
        );
        let database_after = journal_shape(&database);
        if case.operation_status.is_none() {
            assert_eq!(
                database_after, database_before,
                "{} changed the journal before prepare",
                case.name
            );
        }
        assert_git_fsck(&fixture);
    }
}

fn fixture() -> Fixture {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path().join("primary");
    let home = directory.path().join("home");
    let global_config = directory.path().join("isolated.gitconfig");
    fs::create_dir_all(&root).expect("repository directory");
    fs::create_dir_all(&home).expect("isolated home");
    fs::write(&global_config, b"").expect("isolated global config");
    let fixture = Fixture {
        _directory: directory,
        root,
        home,
        global_config,
        jjk: assert_cmd::cargo::cargo_bin!("jjk").to_path_buf(),
    };
    git_success(&fixture, &fixture.root, &["init", "-q", "-b", "main"]);
    fs::write(fixture.root.join("left.txt"), "base left\n").expect("left fixture");
    fs::write(fixture.root.join("right.txt"), "base right\n").expect("right fixture");
    git_success(&fixture, &fixture.root, &["add", "left.txt", "right.txt"]);
    git_success(
        &fixture,
        &fixture.root,
        &[
            "-c",
            "user.name=JJK Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    );
    fixture
}

fn configured_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("LC_ALL", "C")
        .env("LANG", "C");
    if let Some(home) = env::var_os("JJK_ISOLATED_HOME") {
        command.env("HOME", home);
    }
    if let Some(config) = env::var_os("JJK_ISOLATED_GIT_CONFIG") {
        command.env("GIT_CONFIG_GLOBAL", config);
    }
    command
}

fn fixture_command(fixture: &Fixture, program: &Path) -> Command {
    let mut command = configured_command(program);
    command
        .env("HOME", &fixture.home)
        .env("GIT_CONFIG_GLOBAL", &fixture.global_config)
        .env("JJK_ISOLATED_HOME", &fixture.home)
        .env("JJK_ISOLATED_GIT_CONFIG", &fixture.global_config);
    command
}

fn git_output(fixture: &Fixture, cwd: &Path, args: &[&str]) -> Output {
    fixture_command(fixture, Path::new("git"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("execute Git")
}

fn git_success(fixture: &Fixture, cwd: &Path, args: &[&str]) -> Output {
    let output = git_output(fixture, cwd, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        output_diagnostic(&output)
    );
    output
}

fn git_text(fixture: &Fixture, cwd: &Path, args: &[&str]) -> String {
    String::from_utf8(git_success(fixture, cwd, args).stdout).expect("UTF-8 Git output")
}

fn run_jjk(fixture: &Fixture, cwd: &Path, args: &[&str]) -> Output {
    fixture_command(fixture, &fixture.jjk)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("execute jjk")
}

fn jjk_success(fixture: &Fixture, cwd: &Path, args: &[&str]) -> Output {
    let output = run_jjk(fixture, cwd, args);
    assert_no_sqlite_busy_output(&output);
    assert!(
        output.status.success(),
        "jjk {args:?} failed: {}",
        output_diagnostic(&output)
    );
    output
}

fn concurrent_captures<'a>(
    fixture: &Fixture,
    workers: [(&'a Path, &'a str); 2],
) -> [CaptureReceipt; 2] {
    let control = fixture.root.parent().expect("fixture parent").join(format!(
        "capture-barrier-{}",
        workers[0].1.replace(' ', "-")
    ));
    fs::create_dir_all(&control).expect("barrier directory");
    let release = control.join("release");
    let mut children = Vec::with_capacity(2);
    let mut receipts = Vec::with_capacity(2);
    for (index, (worktree, label)) in workers.into_iter().enumerate() {
        let ready = control.join(format!("ready-{index}"));
        let receipt = control.join(format!("receipt-{index}.json"));
        let mut command = fixture_command(
            fixture,
            &env::current_exe().expect("current test executable"),
        );
        command
            .arg("--exact")
            .arg("concurrency_capture_worker")
            .arg("--nocapture")
            .env(WORKER_ENV, "1")
            .env("JJK_BIN", &fixture.jjk)
            .env("JJK_WORKTREE", worktree)
            .env("JJK_CAPTURE_LABEL", label)
            .env("JJK_CONCURRENCY_READY", &ready)
            .env("JJK_CONCURRENCY_RELEASE", &release)
            .env("JJK_CONCURRENCY_RECEIPT", &receipt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        children.push(command.spawn().expect("spawn gated capture worker"));
        receipts.push((ready, receipt));
    }
    for (ready, _) in &receipts {
        wait_for_path(ready, BARRIER_DEADLINE, "worker readiness");
    }
    fs::write(&release, b"go\n").expect("release capture workers");
    for child in children {
        let output = wait_bounded(child, PROCESS_DEADLINE, "capture worker");
        assert!(
            output.status.success(),
            "capture worker harness failed: {}",
            output_diagnostic(&output)
        );
    }
    let mut decoded = receipts.into_iter().map(|(_, receipt)| {
        serde_json::from_slice::<CaptureReceipt>(&fs::read(receipt).expect("read capture receipt"))
            .expect("decode capture receipt")
    });
    [
        decoded.next().expect("first capture"),
        decoded.next().expect("second capture"),
    ]
}

fn wait_for_path(path: &Path, timeout: Duration, description: &str) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_bounded(mut child: Child, timeout: Duration, description: &str) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().expect("poll child") {
            Some(_) => return child.wait_with_output().expect("collect child output"),
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                kill_process_group(&mut child);
                let output = child
                    .wait_with_output()
                    .expect("collect timed-out child output");
                panic!(
                    "{description} exceeded {timeout:?}: {}",
                    output_diagnostic(&output)
                );
            }
        }
    }
}

fn kill_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let killed = Command::new("/bin/kill")
            .args(["-KILL", &group])
            .status()
            .expect("kill child process group");
        assert!(
            killed.success(),
            "could not kill child process group {group}"
        );
    }
    #[cfg(not(unix))]
    child.kill().expect("kill child");
}

fn assert_states_and_refs_survive(fixture: &Fixture, ids: &[&str]) {
    let view = json_result(&jjk_success(fixture, &fixture.root, &["see", "--json"]));
    let visible = state_ids(&view);
    for id in ids {
        assert!(
            visible.contains(*id),
            "successful state {id} was lost from the graph"
        );
        git_success(
            fixture,
            &fixture.root,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/jjk/states/{id}"),
            ],
        );
    }
}

fn assert_projection_watermarks(database: &Path) -> u64 {
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("reopen shared SQLite store read-only");
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("SQLite integrity check");
    assert_eq!(integrity, "ok");
    let (head_seq, head_hash): (u64, Vec<u8>) = connection
        .query_row(
            "SELECT local_seq, event_hash FROM events ORDER BY local_seq DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("journal head");
    let mut statement = connection.prepare("SELECT projection_name, projected_through_seq, projected_through_hash FROM projection_meta ORDER BY projection_name").expect("projection watermarks");
    let projections = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .expect("query projections")
        .collect::<Result<Vec<_>, _>>()
        .expect("decode projections");
    let names = projections
        .iter()
        .map(|(name, _, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    assert!(
        names.is_superset(&BTreeSet::from([
            "runtime-control-history-v1",
            "runtime-navigation-v1",
            "runtime-records-v1"
        ])),
        "runtime projections were not durably registered: {names:?}"
    );
    for (name, projected_seq, projected_hash) in projections {
        assert_eq!(
            projected_seq, head_seq,
            "projection {name} is stale after reopen"
        );
        assert_eq!(
            projected_hash, head_hash,
            "projection {name} points at a different journal hash"
        );
    }
    head_seq
}

fn assert_git_fsck(fixture: &Fixture) {
    git_success(fixture, &fixture.root, &["fsck", "--full", "--strict"]);
}

fn assert_no_sqlite_busy(capture: &CaptureReceipt) {
    let diagnostic = capture.diagnostic().to_ascii_lowercase();
    for leaked in [
        "sqlite_busy",
        "sqlite busy",
        "database is locked",
        "database table is locked",
    ] {
        assert!(
            !diagnostic.contains(leaked),
            "raw SQLite contention leaked to the CLI: {diagnostic}"
        );
    }
}

fn assert_no_sqlite_busy_output(output: &Output) {
    let diagnostic = output_diagnostic(output).to_ascii_lowercase();
    for leaked in [
        "sqlite_busy",
        "sqlite busy",
        "database is locked",
        "database table is locked",
    ] {
        assert!(
            !diagnostic.contains(leaked),
            "raw SQLite contention leaked to the CLI: {diagnostic}"
        );
    }
}

fn receipt_json(receipt: &CaptureReceipt) -> Value {
    serde_json::from_slice(&receipt.stdout).unwrap_or_else(|error| {
        panic!(
            "capture did not produce JSON ({error}): {}",
            receipt.diagnostic()
        )
    })
}

fn receipt_result(receipt: &CaptureReceipt) -> Value {
    let value = receipt_json(receipt);
    value
        .get("result")
        .filter(|result| !result.is_null())
        .cloned()
        .unwrap_or(value)
}

fn output_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "command did not produce JSON ({error}): {}",
            output_diagnostic(output)
        )
    })
}

fn json_result(output: &Output) -> Value {
    let value = output_json(output);
    value
        .get("result")
        .filter(|result| !result.is_null())
        .cloned()
        .unwrap_or(value)
}

fn state_ids(value: &Value) -> BTreeSet<String> {
    value["states"]
        .as_array()
        .expect("states array")
        .iter()
        .map(|state| state_id(state).to_owned())
        .collect()
}
fn error_json(output: &Output) -> Value {
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!(
            "command did not produce a typed JSON error ({error}): {}",
            output_diagnostic(output)
        )
    })
}

fn state_id(value: &Value) -> &str {
    let id = string(value, "state_id");
    assert!(
        id.starts_with("st_"),
        "state identity is not a stable typed ID: {id}"
    );
    id
}

fn projection_version(value: &Value) -> u64 {
    value["projection_version"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing projection_version in {value}"))
}

fn string<'a>(value: &'a Value, field: &str) -> &'a str {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("missing string field `{field}` in {value}"))
}

fn pending_operations(value: &Value) -> BTreeSet<(String, String)> {
    fn visit(value: &Value, found: &mut BTreeSet<(String, String)>) {
        match value {
            Value::Object(object) => {
                let operation_id = object.get("operation_id").and_then(Value::as_str);
                let phase = object
                    .get("phase")
                    .or_else(|| object.get("status"))
                    .and_then(Value::as_str);
                if let (Some(operation_id), Some(phase)) = (operation_id, phase) {
                    if matches!(
                        phase,
                        "prepared"
                            | "applying"
                            | "awaiting_resolution"
                            | "verifying"
                            | "aborting"
                            | "repair_required"
                    ) {
                        found.insert((operation_id.to_owned(), phase.to_owned()));
                    }
                }
                for child in object.values() {
                    visit(child, found);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, found);
                }
            }
            _ => {}
        }
    }
    let mut found = BTreeSet::new();
    visit(value, &mut found);
    found
}

fn state_refs(fixture: &Fixture) -> BTreeSet<(String, String)> {
    let output = git_text(
        fixture,
        &fixture.root,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/jjk/states/",
        ],
    );
    output
        .lines()
        .map(|line| {
            let (name, oid) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("malformed state ref row: {line:?}"));
            (name.to_owned(), oid.to_owned())
        })
        .collect()
}

fn operation_lifecycles(connection: &Connection) -> Vec<(String, String, Vec<String>)> {
    let mut statement = connection
        .prepare(
            "SELECT hex(operation_id), status FROM operations ORDER BY prepared_seq, operation_id",
        )
        .expect("prepare operation lifecycle query");
    let operations = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("query operation lifecycles")
        .collect::<Result<Vec<_>, _>>()
        .expect("decode operation lifecycles");
    operations
        .into_iter()
        .map(|(operation_id, status)| {
            let mut events = connection
                .prepare(
                    "SELECT event_type FROM events WHERE hex(operation_id) = ?1 ORDER BY local_seq",
                )
                .expect("prepare operation event query");
            let events = events
                .query_map([&operation_id], |row| row.get::<_, String>(0))
                .expect("query operation events")
                .collect::<Result<Vec<_>, _>>()
                .expect("decode operation events");
            (operation_id, status, events)
        })
        .collect()
}

fn assert_operation_recovery_material(connection: &Connection, operation_id: &str) {
    let (request_hash, precondition, expected_effects, recovery_hash): (u64, u64, u64, Option<u64>) = connection
        .query_row(
            "SELECT length(request_hash), length(precondition_fingerprint), length(expected_effects), length(recovery_artifact_hash) FROM operations WHERE hex(operation_id) = ?1",
            [operation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("query operation recovery bindings");
    assert_eq!(request_hash, 32, "request hash is not complete");
    assert!(
        precondition > 0,
        "operation omitted its substrate precondition"
    );
    assert!(
        expected_effects > 0,
        "operation omitted its canonical effect plan"
    );
    assert_eq!(
        recovery_hash,
        Some(32),
        "operation omitted its bound recovery artifact digest"
    );
}

fn journal_shape(database: &Path) -> (u64, u64, String, u64, u64) {
    let connection = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open journal shape read-only");
    let (events, last_seq, last_hash): (u64, u64, String) = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(local_seq), 0), COALESCE(hex((SELECT event_hash FROM events ORDER BY local_seq DESC LIMIT 1)), '') FROM events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("query journal shape");
    let operations = connection
        .query_row("SELECT COUNT(*) FROM operations", [], |row| row.get(0))
        .expect("count operations");
    let states = connection
        .query_row("SELECT COUNT(*) FROM states", [], |row| row.get(0))
        .expect("count states");
    (events, last_seq, last_hash, operations, states)
}

fn output_diagnostic(output: &Output) -> String {
    format!(
        "status={}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn required_env_path(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("missing {name}")))
}

fn executable_on_path(name: &str) -> PathBuf {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("could not find {name} on PATH"))
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("fixture path is UTF-8")
}
