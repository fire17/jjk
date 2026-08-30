use std::fs;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn run(cwd: &std::path::Path, program: &std::path::Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run command")
}

fn successful(cwd: &std::path::Path, program: &std::path::Path, args: &[&str]) -> Output {
    let output = run(cwd, program, args);
    assert!(
        output.status.success(),
        "{} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn disable_line_ending_conversion(cwd: &std::path::Path, git: &std::path::Path) {
    successful(cwd, git, &["config", "core.autocrlf", "false"]);
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn canonical_state_engine_captures_graph_and_restores_content() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    fs::write(root.join("story.txt"), "one\n").expect("write fixture");
    successful(root, git, &["add", "story.txt"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );

    let first_setup = json(&successful(root, &jjk, &["setup", "--json"]));
    let second_setup = json(&successful(root, &jjk, &["setup", "--json"]));
    assert_eq!(first_setup["created"], true);
    assert_eq!(second_setup["created"], false);
    assert_eq!(first_setup["repository_id"], second_setup["repository_id"]);

    fs::write(root.join("story.txt"), "two\n").expect("write first state");
    successful(root, git, &["add", "story.txt"]);
    let first = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "first state"],
    ));
    fs::write(root.join("story.txt"), "three\n").expect("write second state");
    successful(root, git, &["add", "story.txt"]);
    let second = json(&successful(
        root,
        &jjk,
        &["nice", "--json", "--", "second state"],
    ));

    let current = json(&successful(root, &jjk, &["current", "--json"]));
    assert_eq!(current["state_id"], second["state_id"]);
    assert_eq!(current["logical_parent"], first["state_id"]);
    let graph = json(&successful(root, &jjk, &["see", "--json"]));
    assert_eq!(graph["states"].as_array().expect("states").len(), 3);

    successful(
        root,
        &jjk,
        &["return", first["state_id"].as_str().expect("state id")],
    );
    assert_eq!(
        fs::read_to_string(root.join("story.txt")).expect("restored fixture"),
        "two\n"
    );
    let doctor = json(&successful(root, &jjk, &["doctor", "--json"]));
    assert_eq!(doctor["healthy"], true);
    assert!(
        doctor["journal_events"]
            .as_u64()
            .is_some_and(|count| count >= 3)
    );
}

#[test]
fn star_marks_existing_states_without_creating_snapshots() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    fs::write(root.join("story.txt"), "base\n").expect("write fixture");
    successful(root, git, &["add", "story.txt"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);
    fs::write(root.join("story.txt"), "first\n").expect("write first state");
    successful(root, git, &["add", "story.txt"]);
    let first = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "first state"],
    ));
    fs::write(root.join("story.txt"), "second\n").expect("write second state");
    successful(root, git, &["add", "story.txt"]);
    let second = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "second state"],
    ));

    let before = json(&successful(root, &jjk, &["see", "--json"]));
    let before_count = before["states"].as_array().expect("states").len();
    let first_id = first["state_id"].as_str().expect("first state id");
    let second_id = second["state_id"].as_str().expect("second state id");

    let starred = json(&successful(root, &jjk, &["star", first_id, "--json"]));
    assert_eq!(starred["command"], "star");
    assert_eq!(starred["state_id"], first_id);
    assert_eq!(starred["starred"], true);
    assert_eq!(starred["changed"], true);
    let repeated = json(&successful(root, &jjk, &["star", first_id, "--json"]));
    assert_eq!(repeated["changed"], false);

    let graph = json(&successful(root, &jjk, &["see", "--json"]));
    assert_eq!(
        graph["states"].as_array().expect("states").len(),
        before_count
    );
    let first_row = graph["states"]
        .as_array()
        .expect("states")
        .iter()
        .find(|state| state["state_id"] == first_id)
        .expect("first state row");
    assert_eq!(first_row["starred"], true);
    assert_eq!(graph["current_state"], second_id);
    assert_eq!(
        json(&successful(root, &jjk, &["current", "--json"]))["starred"],
        false
    );

    let current_star = json(&successful(root, &jjk, &["star", "--json"]));
    assert_eq!(current_star["state_id"], second_id);
    assert_eq!(
        json(&successful(root, &jjk, &["current", "--json"]))["starred"],
        true
    );

    let story = json(&successful(root, &jjk, &["story", "--json"]));
    let story_ids = story["states"]
        .as_array()
        .expect("story states")
        .iter()
        .filter_map(|state| state["state_id"].as_str())
        .collect::<Vec<_>>();
    assert!(story_ids.contains(&first_id));
    assert!(story_ids.contains(&second_id));

    let unstarred = json(&successful(root, &jjk, &["unstar", first_id, "--json"]));
    assert_eq!(unstarred["starred"], false);
    assert_eq!(unstarred["changed"], true);
    let graph = json(&successful(root, &jjk, &["see", "--json"]));
    let first_row = graph["states"]
        .as_array()
        .expect("states")
        .iter()
        .find(|state| state["state_id"] == first_id)
        .expect("first state row");
    assert_eq!(first_row["starred"], false);
    assert_eq!(
        graph["states"].as_array().expect("states").len(),
        before_count
    );
}

#[test]
fn graph_navigation_fork_and_visibility_are_durable() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    fs::write(root.join("story.txt"), "base\n").expect("write fixture");
    successful(root, git, &["add", "story.txt"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);

    fs::write(root.join("story.txt"), "parent\n").expect("write parent");
    successful(root, git, &["add", "story.txt"]);
    let parent = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "parent state"],
    ));
    fs::write(root.join("story.txt"), "child\n").expect("write child");
    successful(root, git, &["add", "story.txt"]);
    let child = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "child state"],
    ));

    let up = json(&successful(root, &jjk, &["up", "--json"]));
    assert_eq!(up["state_id"], parent["state_id"]);
    assert_eq!(
        fs::read_to_string(root.join("story.txt")).expect("read parent"),
        "parent\n"
    );
    let down = json(&successful(root, &jjk, &["down", "--json"]));
    assert_eq!(down["state_id"], child["state_id"]);
    assert_eq!(
        fs::read_to_string(root.join("story.txt")).expect("read child"),
        "child\n"
    );

    let fork = json(&successful(
        root,
        &jjk,
        &["fork", "--json", "--", "alternate parser"],
    ));
    assert_eq!(fork["from_state"], child["state_id"]);
    assert_eq!(fork["source_checkout_mutated"], false);

    successful(root, &jjk, &["up", "--json"]);
    successful(
        root,
        &jjk,
        &[
            "archive",
            child["state_id"].as_str().expect("child id"),
            "--json",
        ],
    );
    let hidden = json(&successful(root, &jjk, &["see", "--json"]));
    assert_eq!(hidden["states"].as_array().expect("states").len(), 2);
    successful(
        root,
        &jjk,
        &[
            "recover",
            child["state_id"].as_str().expect("child id"),
            "--json",
        ],
    );
    let visible = json(&successful(root, &jjk, &["see", "--json"]));
    assert_eq!(visible["states"].as_array().expect("states").len(), 3);

    let doctor = json(&successful(root, &jjk, &["doctor", "--json"]));
    assert_eq!(doctor["healthy"], true);
    assert!(
        doctor["journal_events"]
            .as_u64()
            .is_some_and(|count| count >= 8)
    );
}

#[test]
fn human_output_is_readable_while_json_remains_machine_stable() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    fs::write(root.join("story.txt"), "one\n").expect("write fixture");
    successful(root, git, &["add", "story.txt"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    let setup = successful(root, &jjk, &["setup"]);
    assert!(String::from_utf8_lossy(&setup.stdout).starts_with("safe space:"));

    fs::write(root.join("story.txt"), "two\n").expect("write state");
    successful(root, git, &["add", "story.txt"]);
    let state = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "human checkpoint"],
    ));
    assert!(
        state["state_id"]
            .as_str()
            .expect("state id")
            .starts_with("st_")
    );
    let graph = successful(root, &jjk, &["see"]);
    let graph = String::from_utf8_lossy(&graph.stdout);
    assert!(graph.contains("state          kind     label"));
    assert!(graph.contains("human-checkpoint"));
    assert!(!graph.trim_start().starts_with('{'));
}

#[test]
fn worktree_fork_is_isolated_and_shares_repository_state() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    fs::write(root.join("story.txt"), "base\n").expect("write fixture");
    successful(root, git, &["add", "story.txt"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);
    fs::write(root.join("story.txt"), "shared state\n").expect("write state");
    successful(root, git, &["add", "story.txt"]);
    let state = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "shared base"],
    ));

    let fork = json(&successful(
        root,
        &jjk,
        &["fork", "--worktree", "--json", "--", "agent parser lane"],
    ));
    let path = std::path::PathBuf::from(fork["worktree"].as_str().expect("worktree path"));
    assert!(path.is_dir());
    assert_eq!(
        fs::read_to_string(path.join("story.txt")).expect("fork content"),
        "shared state\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("story.txt")).expect("source content"),
        "shared state\n"
    );

    let fork_current = json(&successful(
        &path,
        &jjk,
        &[
            "return",
            state["state_id"].as_str().expect("state id"),
            "--json",
        ],
    ));
    assert_eq!(fork_current["state_id"], state["state_id"]);
    let source_current = json(&successful(root, &jjk, &["current", "--json"]));
    assert_eq!(source_current["state_id"], state["state_id"]);
    assert_ne!(
        successful(root, git, &["rev-parse", "--abbrev-ref", "HEAD"]).stdout,
        successful(&path, git, &["rev-parse", "--abbrev-ref", "HEAD"]).stdout
    );
}

#[test]
fn backup_is_online_verified_and_load_refuses_overwrite() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path().join("source");
    fs::create_dir(&root).expect("source dir");
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    successful(&root, git, &["init", "-q", "-b", "main"]);
    fs::write(root.join("story.txt"), "base\n").expect("write fixture");
    successful(&root, git, &["add", "story.txt"]);
    successful(
        &root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(&root, &jjk, &["setup", "--json"]);
    fs::write(root.join("story.txt"), "saved\n").expect("write saved");
    successful(&root, git, &["add", "story.txt"]);
    let state = json(&successful(
        &root,
        &jjk,
        &["nice", "--json", "--", "recoverable state"],
    ));

    let backup = directory.path().join("state.sqlite3");
    let created = json(&successful(
        &root,
        &jjk,
        &["backup", "create", backup.to_str().expect("path"), "--json"],
    ));
    assert!(
        created["journal_events"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
    let verified = json(&successful(
        &root,
        &jjk,
        &["backup", "verify", backup.to_str().expect("path"), "--json"],
    ));
    assert_eq!(verified["healthy"], true);
    assert_eq!(verified["journal_events"], created["journal_events"]);

    let target = directory.path().join("restored");
    successful(
        &root,
        &jjk,
        &[
            "load",
            backup.to_str().expect("path"),
            "--into",
            target.to_str().expect("target"),
            "--json",
        ],
    );
    let restored = json(&successful(&target, &jjk, &["see", "--json"]));
    assert_eq!(restored["states"][0]["state_id"], state["state_id"]);
    fs::write(target.join("keep"), "untouched").expect("sentinel");
    let refused = run(
        &root,
        &jjk,
        &[
            "load",
            backup.to_str().expect("path"),
            "--into",
            target.to_str().expect("target"),
            "--json",
        ],
    );
    assert_eq!(refused.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(target.join("keep")).expect("sentinel"),
        "untouched"
    );
}

#[test]
fn destructive_navigation_refuses_unrepresented_workspace_changes() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    fs::write(root.join("story.txt"), "base\n").expect("write fixture");
    successful(root, git, &["add", "story.txt"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);
    fs::write(root.join("story.txt"), "saved\n").expect("write state");
    successful(root, git, &["add", "story.txt"]);
    let state = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "safe state"],
    ));
    fs::write(root.join("story.txt"), "unique unsaved work\n").expect("dirty work");

    let refused = run(
        root,
        &jjk,
        &[
            "return",
            state["state_id"].as_str().expect("state"),
            "--json",
        ],
    );
    assert_eq!(refused.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(root.join("story.txt")).expect("preserved work"),
        "unique unsaved work\n"
    );
    let error = String::from_utf8_lossy(&refused.stderr);
    assert!(error.contains("differs from the current JJK state"));
}

#[test]
fn corrupted_backup_fails_without_creating_restore_target() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path().join("source");
    fs::create_dir(&root).expect("source");
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    successful(&root, git, &["init", "-q", "-b", "main"]);
    fs::write(root.join("story.txt"), "base\n").expect("fixture");
    successful(&root, git, &["add", "story.txt"]);
    successful(
        &root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(&root, &jjk, &["setup", "--json"]);
    let backup = directory.path().join("corrupt.sqlite3");
    successful(
        &root,
        &jjk,
        &[
            "backup",
            "create",
            backup.to_str().expect("backup"),
            "--json",
        ],
    );
    fs::write(&backup, b"not sqlite").expect("corrupt backup");
    let target = directory.path().join("must-not-exist");
    let refused = run(
        &root,
        &jjk,
        &[
            "load",
            backup.to_str().expect("backup"),
            "--into",
            target.to_str().expect("target"),
            "--json",
        ],
    );
    assert_eq!(refused.status.code(), Some(70));
    assert!(!target.exists());
}

#[test]
fn return_works_when_captured_files_were_never_staged() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    successful(root, &jjk, &["setup", "--json"]);

    // Fresh directory: files exist only in the worktree; the index stays empty.
    fs::write(root.join("color.txt"), "green\n").expect("write green");
    let green = json(&successful(root, &jjk, &["save", "--json", "--", "green"]));
    fs::write(root.join("color.txt"), "purple\n").expect("write purple");
    let purple = json(&successful(root, &jjk, &["step", "--json", "--", "purple"]));
    assert_eq!(purple["logical_parent"], green["state_id"]);

    let restored = successful(root, &jjk, &["return", "green", "--json"]);
    assert_eq!(json(&restored)["state_id"], green["state_id"]);
    assert_eq!(
        fs::read_to_string(root.join("color.txt")).expect("restored"),
        "green\n"
    );

    // A worktree edit of a captured file after the capture still blocks navigation.
    fs::write(root.join("color.txt"), "dirty\n").expect("dirty");
    let refused = run(root, &jjk, &["return", "purple", "--json"]);
    assert!(
        !refused.status.success(),
        "dirty worktree must refuse return"
    );
}

#[test]
fn state_queries_accept_messages_and_unnormalized_labels() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
    successful(root, &jjk, &["setup", "--json"]);
    fs::write(root.join("color.txt"), "green\n").expect("write green");
    successful(root, git, &["add", "color.txt"]);
    let green = json(&successful(root, &jjk, &["save", "--json", "--", "green"]));
    fs::write(root.join("color.txt"), "purple\n").expect("write purple");
    successful(root, git, &["add", "color.txt"]);
    let purple = json(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "Fast_Purple mode!"],
    ));
    assert_eq!(purple["label"], "fast-purple-mode");

    for query in ["fast-purple-mode", "Fast_Purple mode!", "fast_purple mode"] {
        let starred = json(&successful(root, &jjk, &["star", query, "--json"]));
        assert_eq!(starred["state_id"], purple["state_id"], "query {query}");
    }
    let restored = json(&successful(root, &jjk, &["return", "green", "--json"]));
    assert_eq!(restored["state_id"], green["state_id"]);
}

#[test]
fn command_help_prints_exact_grammar() {
    let directory = TempDir::new().expect("tempdir");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    for (command, fragment) in [
        ("fork", "jjk fork [--worktree] [--json] -- <objective…>"),
        ("validate", "-- <program> [args…]"),
        ("handoff", "handoff create --request <handoff.json>"),
        ("load", "--into <new-destination>"),
        ("see", "jjk see [--json] [--width <columns>]"),
    ] {
        let help =
            String::from_utf8(successful(directory.path(), &jjk, &[command, "--help"]).stdout)
                .expect("help UTF-8");
        assert!(help.contains(fragment), "{command} help:\n{help}");
        assert!(
            !help.contains("[arguments]"),
            "{command} help still generic:\n{help}"
        );
    }
}

fn init_repository(root: &std::path::Path, git: &std::path::Path) {
    successful(root, git, &["init", "-q", "-b", "main"]);
    disable_line_ending_conversion(root, git);
}

fn control_database_bytes(root: &std::path::Path) -> u64 {
    ["state.sqlite3", "state.sqlite3-wal"]
        .iter()
        .map(|name| {
            fs::metadata(root.join(".git").join("jjk").join(name))
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        })
        .sum()
}

#[test]
fn navigation_never_deletes_uncaptured_files() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    init_repository(root, git);
    fs::write(root.join(".gitignore"), "build/\n").expect("write ignore rules");
    fs::write(root.join("color.txt"), "base\n").expect("write base");
    successful(root, git, &["add", "-A"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);

    fs::write(root.join("color.txt"), "green\n").expect("write green");
    successful(root, git, &["add", "-A"]);
    let green = json(&successful(root, &jjk, &["save", "--json", "--", "green"]));
    fs::write(root.join("color.txt"), "purple\n").expect("write purple");
    fs::write(root.join("fast.txt"), "fast\n").expect("write captured extra");
    successful(root, git, &["add", "-A"]);
    successful(root, &jjk, &["step", "--json", "--", "fast purple"]);

    // Work that JJK never captured: an untracked note and ignored build output.
    fs::write(root.join("notes.md"), "scratch\n").expect("write untracked extra");
    fs::create_dir_all(root.join("build")).expect("create build dir");
    fs::write(root.join("build").join("out.bin"), "artifact\n").expect("write ignored extra");

    let restored = json(&successful(root, &jjk, &["return", "green", "--json"]));
    assert_eq!(restored["state_id"], green["state_id"]);
    assert_eq!(
        fs::read_to_string(root.join("color.txt")).expect("restored"),
        "green\n"
    );
    assert!(
        !root.join("fast.txt").exists(),
        "a file captured by the state being left must be removed"
    );
    assert_eq!(
        fs::read_to_string(root.join("notes.md")).expect("untracked extra survives"),
        "scratch\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("build").join("out.bin")).expect("ignored extra survives"),
        "artifact\n"
    );
    let doctor = json(&successful(root, &jjk, &["doctor", "--json"]));
    assert_eq!(doctor["healthy"], true);
}

#[test]
fn navigation_never_deletes_uncaptured_files_in_unstaged_repositories() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    init_repository(root, git);
    successful(root, &jjk, &["setup", "--json"]);
    fs::write(root.join("color.txt"), "green\n").expect("write green");
    successful(root, &jjk, &["save", "--json", "--", "green"]);
    fs::write(root.join("color.txt"), "purple\n").expect("write purple");
    fs::write(root.join("fast.txt"), "fast\n").expect("write captured extra");
    let purple = json(&successful(root, &jjk, &["step", "--json", "--", "purple"]));
    fs::write(root.join("notes.md"), "scratch\n").expect("write untracked extra");

    successful(root, &jjk, &["return", "green", "--json"]);
    assert_eq!(
        fs::read_to_string(root.join("color.txt")).expect("restored"),
        "green\n"
    );
    assert!(!root.join("fast.txt").exists(), "captured file removed");
    assert!(root.join("notes.md").is_file(), "uncaptured extra survives");

    let forward = json(&successful(root, &jjk, &["return", "purple", "--json"]));
    assert_eq!(forward["state_id"], purple["state_id"]);
    assert_eq!(
        fs::read_to_string(root.join("fast.txt")).expect("captured file restored"),
        "fast\n"
    );
    assert!(
        root.join("notes.md").is_file(),
        "extra survives the second navigation"
    );
}

#[test]
fn snapshots_exclude_ignored_content_and_stay_small() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    init_repository(root, git);
    fs::write(root.join(".gitignore"), "build/\n").expect("write ignore rules");
    fs::write(root.join("a.txt"), "a\n").expect("write source");
    fs::create_dir_all(root.join("build")).expect("create build dir");
    let ignored_bytes = 20_000_000usize;
    fs::write(
        root.join("build").join("blob.bin"),
        vec![0x5au8; ignored_bytes],
    )
    .expect("write large ignored artifact");
    successful(root, git, &["add", "-A"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);
    for step in ["one", "two"] {
        fs::write(root.join("a.txt"), format!("{step}\n")).expect("write step");
        successful(root, git, &["add", "-A"]);
        successful(root, &jjk, &["step", "--json", "--", step]);
    }
    let database_bytes = control_database_bytes(root);
    assert!(
        database_bytes < (ignored_bytes / 4) as u64,
        "control database must not embed ignored content: {database_bytes} bytes"
    );
    successful(root, &jjk, &["return", "one", "--json"]);
    assert_eq!(
        fs::metadata(root.join("build").join("blob.bin"))
            .expect("ignored artifact survives navigation")
            .len(),
        ignored_bytes as u64
    );
}

#[test]
fn navigation_is_not_blocked_by_stale_index_stat_after_a_restore() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    init_repository(root, git);
    fs::write(root.join("a.txt"), "base\n").expect("write base");
    successful(root, git, &["add", "-A"]);
    successful(
        root,
        git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
    successful(root, &jjk, &["setup", "--json"]);
    fs::write(root.join("a.txt"), "green\n").expect("write green");
    successful(root, git, &["add", "-A"]);
    let green = json(&successful(root, &jjk, &["save", "--json", "--", "green"]));
    fs::write(root.join("a.txt"), "purple\n").expect("write purple");
    successful(root, git, &["add", "-A"]);
    let purple = json(&successful(root, &jjk, &["step", "--json", "--", "purple"]));

    // No porcelain Git command runs between these navigations, so the index stat cache is
    // exactly as the restore left it.
    let restored = json(&successful(root, &jjk, &["return", "green", "--json"]));
    assert_eq!(restored["state_id"], green["state_id"]);
    let undone = json(&successful(root, &jjk, &["undo", "--json"]));
    assert_eq!(
        json(&successful(root, &jjk, &["current", "--json"]))["state_id"],
        undone["state_id"]
    );
    let redone = json(&successful(root, &jjk, &["redo", "--json"]));
    assert_eq!(
        json(&successful(root, &jjk, &["current", "--json"]))["state_id"],
        redone["state_id"]
    );
    let forward = json(&successful(root, &jjk, &["return", "purple", "--json"]));
    assert_eq!(forward["state_id"], purple["state_id"]);
    assert_eq!(
        fs::read_to_string(root.join("a.txt")).expect("restored"),
        "purple\n"
    );
}
