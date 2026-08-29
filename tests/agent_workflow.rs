use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

fn run(cwd: &Path, program: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run command")
}

fn successful(cwd: &Path, program: &Path, args: &[&str]) -> Output {
    let output = run(cwd, program, args);
    assert!(
        output.status.success(),
        "{} failed\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn checked(args: &[&str], output: Output) -> Output {
    assert!(
        output.status.success(),
        "{} failed\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("command emits JSON")
}

fn text_output(cwd: &Path, program: &Path, args: &[&str]) -> String {
    String::from_utf8(successful(cwd, program, args).stdout)
        .expect("command output is UTF-8")
        .trim()
        .to_owned()
}

fn assert_typed_id(value: &Value, prefix: &str) {
    let value = value.as_str().expect("typed ID is a string");
    assert!(
        value.starts_with(prefix),
        "{value:?} must start with {prefix:?}"
    );
    assert!(
        value.len() > prefix.len(),
        "typed ID must contain an identity"
    );
}

fn git(cwd: &Path, args: &[&str]) -> Output {
    successful(cwd, Path::new("git"), args)
}

fn git_worktrees(root: &Path) -> Vec<(PathBuf, String)> {
    let output = git(root, &["worktree", "list", "--porcelain"]);
    let text = String::from_utf8(output.stdout).expect("worktree list is UTF-8");
    text.split("\n\n")
        .filter_map(|record| {
            let mut path = None;
            let mut head = None;
            for line in record.lines() {
                if let Some(value) = line.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(value));
                } else if let Some(value) = line.strip_prefix("HEAD ") {
                    head = Some(value.to_owned());
                }
            }
            path.zip(head)
        })
        .collect()
}

#[test]
fn val_agent_001_002_isolated_handoff_validation_and_explicit_pick() {
    let directory = TempDir::new().expect("temporary fixture");
    let root = directory.path();
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");

    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "JJK Agent Fixture"]);
    git(root, &["config", "user.email", "agents@example.test"]);
    fs::write(root.join("base.txt"), b"deterministic base\n").expect("write base");
    git(root, &["add", "base.txt"]);
    git(root, &["commit", "-qm", "fixture base"]);
    let object_format = text_output(
        root,
        Path::new("git"),
        &["rev-parse", "--show-object-format"],
    );
    successful(root, &jjk, &["setup", "--json"]);
    let base = json_output(&successful(
        root,
        &jjk,
        &["step", "--json", "--", "shared agent base"],
    ));
    let base_state = base["state_id"].as_str().expect("base state").to_owned();
    let source_head_before = text_output(root, Path::new("git"), &["rev-parse", "HEAD"]);
    let source_status_before = successful(root, Path::new("git"), &["status", "--porcelain"]);
    assert!(source_status_before.stdout.is_empty());

    let fork_alpha = json_output(&successful(
        root,
        &jjk,
        &["fork", "--worktree", "--json", "--", "agent alpha"],
    ));
    let fork_beta = json_output(&successful(
        root,
        &jjk,
        &["fork", "--worktree", "--json", "--", "agent beta"],
    ));
    for fork in [&fork_alpha, &fork_beta] {
        assert_eq!(fork["from_state"], base_state);
        assert_eq!(fork["source_checkout_mutated"], false);
        assert_typed_id(&fork["attempt_id"], "at_");
        assert_typed_id(&fork["workspace_id"], "ws_");
    }
    assert_ne!(fork_alpha["attempt_id"], fork_beta["attempt_id"]);
    assert_ne!(fork_alpha["workspace_id"], fork_beta["workspace_id"]);
    assert_ne!(fork_alpha["branch"], fork_beta["branch"]);
    assert_ne!(fork_alpha["worktree"], fork_beta["worktree"]);

    let alpha_path = PathBuf::from(fork_alpha["worktree"].as_str().expect("alpha path"))
        .canonicalize()
        .expect("canonical alpha path");
    let beta_path = PathBuf::from(fork_beta["worktree"].as_str().expect("beta path"))
        .canonicalize()
        .expect("canonical beta path");
    assert!(alpha_path.is_dir());
    assert!(beta_path.is_dir());
    let worktrees_after_fork = git_worktrees(root)
        .into_iter()
        .map(|(path, head)| (path.canonicalize().expect("canonical worktree path"), head))
        .collect::<Vec<_>>();
    assert_eq!(worktrees_after_fork.len(), 3);
    assert!(
        worktrees_after_fork
            .iter()
            .any(|(path, _)| path == &alpha_path)
    );
    assert!(
        worktrees_after_fork
            .iter()
            .any(|(path, _)| path == &beta_path)
    );

    let alpha_initial = json_output(&successful(&alpha_path, &jjk, &["current", "--json"]));
    let beta_initial = json_output(&successful(&beta_path, &jjk, &["current", "--json"]));
    assert_eq!(alpha_initial["state_id"], base_state);
    assert_eq!(beta_initial["state_id"], base_state);
    assert_eq!(alpha_initial["workspace_id"], fork_alpha["workspace_id"]);
    assert_eq!(beta_initial["workspace_id"], fork_beta["workspace_id"]);

    fs::write(alpha_path.join("alpha.txt"), b"alpha contribution\n").expect("alpha change");
    git(&alpha_path, &["add", "alpha.txt"]);
    fs::write(beta_path.join("beta.txt"), b"beta contribution\n").expect("beta change");
    git(&beta_path, &["add", "beta.txt"]);

    let alpha_capture_args = ["step", "--json", "--", "alpha contribution"];
    let beta_capture_args = ["nice", "--json", "--", "beta contribution"];
    let alpha_child = Command::new(&jjk)
        .args(alpha_capture_args)
        .current_dir(&alpha_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn alpha agent capture");
    let beta_child = Command::new(&jjk)
        .args(beta_capture_args)
        .current_dir(&beta_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn beta agent capture");
    let alpha = json_output(&checked(
        &alpha_capture_args,
        alpha_child
            .wait_with_output()
            .expect("wait for alpha agent"),
    ));
    let beta = json_output(&checked(
        &beta_capture_args,
        beta_child.wait_with_output().expect("wait for beta agent"),
    ));

    assert_eq!(alpha["logical_parent"], base_state);
    assert_eq!(beta["logical_parent"], base_state);
    assert_eq!(alpha["attempt_id"], fork_alpha["attempt_id"]);
    assert_eq!(beta["attempt_id"], fork_beta["attempt_id"]);
    assert_eq!(alpha["workspace_id"], fork_alpha["workspace_id"]);
    assert_eq!(beta["workspace_id"], fork_beta["workspace_id"]);
    assert_ne!(alpha["state_id"], beta["state_id"]);
    assert_ne!(alpha["commit"], beta["commit"]);
    assert!(!beta_path.join("alpha.txt").exists());
    assert!(!alpha_path.join("beta.txt").exists());
    assert_eq!(
        fs::read(root.join("base.txt")).expect("source bytes"),
        b"deterministic base\n"
    );
    assert!(!root.join("alpha.txt").exists());
    assert!(!root.join("beta.txt").exists());
    assert_eq!(
        text_output(root, Path::new("git"), &["rev-parse", "HEAD"]),
        source_head_before
    );
    assert!(git(root, &["status", "--porcelain"]).stdout.is_empty());

    let alpha_state = alpha["state_id"].as_str().expect("alpha state");
    let beta_state = beta["state_id"].as_str().expect("beta state");
    let alpha_validation = json_output(&successful(
        &alpha_path,
        &jjk,
        &[
            "validate",
            "--json",
            alpha_state,
            "--suite",
            "alpha-content",
            "--",
            "git",
            "cat-file",
            "-e",
            &format!(
                "{}^{{tree}}",
                alpha["commit"].as_str().expect("alpha commit")
            ),
        ],
    ));
    let beta_validation = json_output(&successful(
        &beta_path,
        &jjk,
        &[
            "validate",
            "--json",
            beta_state,
            "--suite",
            "beta-content",
            "--",
            "git",
            "cat-file",
            "-e",
            &format!("{}^{{tree}}", beta["commit"].as_str().expect("beta commit")),
        ],
    ));

    for (validation, state, suite, commit) in [
        (
            &alpha_validation,
            alpha_state,
            "alpha-content",
            alpha["commit"].as_str().expect("commit"),
        ),
        (
            &beta_validation,
            beta_state,
            "beta-content",
            beta["commit"].as_str().expect("commit"),
        ),
    ] {
        assert_typed_id(&validation["validation_id"], "ver_");
        assert_eq!(validation["subject_state"], state);
        assert_eq!(validation["suite"], suite);
        assert_eq!(validation["outcome"], "pass");
        assert_eq!(validation["exit_code"], 0);
        let expected_tree = text_output(
            root,
            Path::new("git"),
            &["rev-parse", &format!("{commit}^{{tree}}")],
        );
        assert_eq!(
            validation["content_digest"],
            format!("{object_format}:{expected_tree}")
        );
        let evidence = validation["evidence_digest"]
            .as_str()
            .expect("evidence digest");
        assert_eq!(evidence.len(), 64);
        assert!(
            evidence
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert!(
            validation["argv"]
                .as_array()
                .is_some_and(|argv| !argv.is_empty())
        );
    }
    assert_ne!(
        alpha_validation["validation_id"],
        beta_validation["validation_id"]
    );
    assert_ne!(
        alpha_validation["content_digest"],
        beta_validation["content_digest"]
    );
    assert_ne!(
        alpha_validation["evidence_digest"],
        beta_validation["evidence_digest"]
    );

    let handoff_request = root.join("alpha-handoff.json");
    let canonical_root = fs::canonicalize(root).expect("canonical fixture root");
    let canonical_alpha = fs::canonicalize(&alpha_path).expect("canonical alpha worktree");
    let alpha_relative = canonical_alpha
        .strip_prefix(&canonical_root)
        .expect("worktree under fixture root");
    fs::write(
        &handoff_request,
        serde_json::to_vec(&json!({
            "owner": {
                "actor_id": "actor_01m1562dmcf6ga4j3d5czttv87",
                "worker_id": "worker_01m1562dmcf6ga4j3d5czttv88"
            },
            "objective": "Deliver the isolated alpha contribution for explicit integration",
            "base_state": base_state,
            "produced_state": alpha_state,
            "validation_ids": [alpha_validation["validation_id"]],
            "remaining_risks": [],
            "resume": {
                "program": "sh",
                "args": ["-c", "printf resumed > resume-command-must-not-run"],
                "cwd": alpha_relative.to_string_lossy()
            }
        }))
        .expect("serialize handoff"),
    )
    .expect("write handoff request");
    let handoff = json_output(&successful(
        root,
        &jjk,
        &[
            "handoff",
            "create",
            "--request",
            handoff_request.to_str().expect("request path"),
            "--json",
        ],
    ));
    assert_eq!(handoff["command"], "handoff");
    assert_eq!(handoff["action"], "create");
    assert_typed_id(&handoff["handoff_id"], "handoff_");
    assert_eq!(
        handoff["owner"]["actor_id"],
        "actor_01m1562dmcf6ga4j3d5czttv87"
    );
    assert_eq!(
        handoff["owner"]["worker_id"],
        "worker_01m1562dmcf6ga4j3d5czttv88"
    );
    assert_eq!(
        handoff["objective"],
        "Deliver the isolated alpha contribution for explicit integration"
    );
    assert_eq!(handoff["base_state"], base_state);
    assert_eq!(handoff["produced_state"], alpha_state);
    assert_eq!(
        handoff["validation_ids"],
        json!([alpha_validation["validation_id"]])
    );
    assert_eq!(handoff["remaining_risks"], json!([]));
    assert_eq!(handoff["resume"]["program"], "sh");
    assert_eq!(
        handoff["resume"]["args"],
        json!(["-c", "printf resumed > resume-command-must-not-run"])
    );
    assert_eq!(
        handoff["resume"]["cwd"],
        alpha_relative.to_string_lossy().as_ref()
    );
    assert_eq!(handoff["status"], "offered");
    assert!(handoff["created_at"].is_string());
    assert!(
        handoff["version"]
            .as_u64()
            .is_some_and(|version| version > 0)
    );

    let handoff_id = handoff["handoff_id"].as_str().expect("handoff ID");
    let shown = json_output(&successful(
        root,
        &jjk,
        &["handoff", "show", handoff_id, "--json"],
    ));
    for field in [
        "handoff_id",
        "owner",
        "objective",
        "base_state",
        "produced_state",
        "validation_ids",
        "remaining_risks",
        "resume",
        "status",
        "created_at",
        "version",
    ] {
        assert_eq!(shown[field], handoff[field], "show changed field {field}");
    }

    let resume_sentinel = alpha_path.join("resume-command-must-not-run");
    let consume = json_output(&successful(
        root,
        &jjk,
        &["handoff", "consume", handoff_id, "--json"],
    ));
    assert_eq!(consume["command"], "handoff");
    assert_eq!(consume["action"], "consume");
    assert_eq!(consume["handoff_id"], handoff_id);
    assert_eq!(consume["status"], "accepted");
    assert_eq!(consume["resume"], handoff["resume"]);
    assert!(
        !resume_sentinel.exists(),
        "consuming a handoff must return, not execute, its recipe"
    );
    let accepted = json_output(&successful(
        root,
        &jjk,
        &["handoff", "show", handoff_id, "--json"],
    ));
    assert_eq!(accepted["status"], "accepted");
    assert!(accepted["accepted_at"].is_string());
    for field in [
        "owner",
        "objective",
        "base_state",
        "produced_state",
        "validation_ids",
        "remaining_risks",
        "resume",
    ] {
        assert_eq!(
            accepted[field], handoff[field],
            "consume changed field {field}"
        );
    }

    assert_eq!(
        git_worktrees(root).len(),
        3,
        "handoff must not delete source worktrees"
    );
    assert!(alpha_path.is_dir());
    assert!(beta_path.is_dir());
    assert_eq!(
        fs::read(alpha_path.join("alpha.txt")).expect("alpha retained"),
        b"alpha contribution\n"
    );
    assert_eq!(
        fs::read(beta_path.join("beta.txt")).expect("beta retained"),
        b"beta contribution\n"
    );

    successful(root, &jjk, &["return", beta_state, "--json"]);
    let pick = json_output(&successful(root, &jjk, &["pick", alpha_state, "--json"]));
    assert_eq!(pick["kind"], "cherry");
    assert_eq!(pick["source_state"], alpha_state);
    assert_eq!(pick["source_parent"], base_state);
    assert_eq!(pick["target_base"], beta_state);
    assert_eq!(pick["conflicted"], false);
    assert_typed_id(&pick["state_id"], "st_");
    assert_typed_id(&pick["attempt_id"], "at_");
    assert_typed_id(&pick["provenance_id"], "prov_");
    assert_ne!(pick["state_id"], alpha["state_id"]);
    assert_ne!(pick["state_id"], beta["state_id"]);
    assert!(
        pick["patch_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(
        fs::read(root.join("alpha.txt")).expect("integrated alpha"),
        b"alpha contribution\n"
    );
    assert_eq!(
        fs::read(root.join("beta.txt")).expect("retained beta"),
        b"beta contribution\n"
    );

    let graph = json_output(&successful(root, &jjk, &["see", "--json"]));
    let states = graph["states"].as_array().expect("state graph");
    for state in [
        &base["state_id"],
        &alpha["state_id"],
        &beta["state_id"],
        &pick["state_id"],
    ] {
        assert!(
            states
                .iter()
                .any(|candidate| candidate["state_id"] == *state),
            "graph lost state {state}"
        );
    }
    let alpha_graph = states
        .iter()
        .find(|state| state["state_id"] == alpha["state_id"])
        .expect("alpha future");
    let beta_graph = states
        .iter()
        .find(|state| state["state_id"] == beta["state_id"])
        .expect("beta future");
    let pick_graph = states
        .iter()
        .find(|state| state["state_id"] == pick["state_id"])
        .expect("pick future");
    assert_eq!(alpha_graph["logical_parent"], base_state);
    assert_eq!(beta_graph["logical_parent"], base_state);
    assert_eq!(pick_graph["logical_parent"], beta_state);
    assert_eq!(alpha_graph["attempt_id"], fork_alpha["attempt_id"]);
    assert_eq!(beta_graph["attempt_id"], fork_beta["attempt_id"]);

    assert_eq!(
        git_worktrees(root).len(),
        3,
        "explicit integration must retain both source worktrees"
    );
    let alpha_commit_after = text_output(
        root,
        Path::new("git"),
        &["rev-parse", alpha["commit"].as_str().expect("alpha commit")],
    );
    let beta_commit_after = text_output(
        root,
        Path::new("git"),
        &["rev-parse", beta["commit"].as_str().expect("beta commit")],
    );
    assert_eq!(alpha_commit_after, alpha["commit"]);
    assert_eq!(beta_commit_after, beta["commit"]);
    assert!(git(root, &["fsck", "--full"]).status.success());
    let native_worktree_check = git(root, &["worktree", "list", "--porcelain"]);
    assert!(native_worktree_check.status.success());
    assert_eq!(git_worktrees(root).len(), 3);

    let final_current = json_output(&successful(root, &jjk, &["current", "--json"]));
    assert_eq!(final_current["state_id"], pick["state_id"]);
    assert_eq!(final_current["logical_parent"], beta_state);
    let doctor = json_output(&successful(root, &jjk, &["doctor", "--json"]));
    assert_eq!(doctor["healthy"], true);
}
