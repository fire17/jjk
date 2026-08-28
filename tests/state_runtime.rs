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
    assert_eq!(graph["states"].as_array().expect("states").len(), 2);

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
    assert_eq!(doctor["journal_events"], 3);
}

#[test]
fn graph_navigation_fork_and_visibility_are_durable() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
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
    assert_eq!(hidden["states"].as_array().expect("states").len(), 1);
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
    assert_eq!(visible["states"].as_array().expect("states").len(), 2);

    let doctor = json(&successful(root, &jjk, &["doctor", "--json"]));
    assert_eq!(doctor["healthy"], true);
    assert_eq!(doctor["journal_events"], 8);
}

#[test]
fn human_output_is_readable_while_json_remains_machine_stable() {
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    let git = std::path::Path::new("git");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    successful(root, git, &["init", "-q", "-b", "main"]);
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
    assert_eq!(created["journal_events"], 1);
    let verified = json(&successful(
        &root,
        &jjk,
        &["backup", "verify", backup.to_str().expect("path"), "--json"],
    ));
    assert_eq!(verified["healthy"], true);

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
