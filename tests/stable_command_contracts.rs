use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

struct Repo {
    _directory: TempDir,
    root: PathBuf,
    jjk: PathBuf,
}

impl Repo {
    fn new(files: &[(&str, &[u8])]) -> Self {
        let directory = TempDir::new().expect("temporary repository");
        let root = directory.path().to_path_buf();
        let jjk = assert_cmd::cargo::cargo_bin!("jjk").to_path_buf();
        successful(&root, "git", ["init", "-q", "-b", "main"]);
        successful(
            &root,
            "git",
            ["config", "user.name", "JJK Contract Fixture"],
        );
        successful(
            &root,
            "git",
            ["config", "user.email", "jjk-contract@example.invalid"],
        );
        for (path, bytes) in files {
            let destination = root.join(path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).expect("fixture parent");
            }
            fs::write(destination, bytes).expect("fixture file");
        }
        successful(&root, "git", ["add", "-A"]);
        successful(&root, "git", ["commit", "-qm", "fixture base"]);
        let repository = Self {
            _directory: directory,
            root,
            jjk,
        };
        repository.jjk_json(["setup", "--json"]);
        repository
    }

    fn jjk_json<const N: usize>(&self, args: [&str; N]) -> Value {
        json(&successful(&self.root, &self.jjk, args))
    }
}

fn run<I, S>(cwd: &Path, program: impl AsRef<OsStr>, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run command")
}

fn run_with_path<I, S>(cwd: &Path, program: impl AsRef<OsStr>, args: I, path: &Path) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("PATH", path)
        .output()
        .expect("run command with isolated PATH")
}

fn successful_with_path<I, S>(
    cwd: &Path,
    program: impl AsRef<OsStr>,
    args: I,
    path: &Path,
) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = run_with_path(cwd, program, args, path);
    assert!(
        output.status.success(),
        "command failed with {:?}: stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn successful<I, S>(cwd: &Path, program: impl AsRef<OsStr>, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = run(cwd, program, args);
    assert!(
        output.status.success(),
        "command failed with {:?}: stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected JSON output ({error}): stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_typed_id(value: &Value, prefix: &str) -> String {
    let id = value.as_str().expect("typed id string");
    assert!(
        id.starts_with(prefix),
        "expected {prefix} typed id, got {id}"
    );
    assert!(
        id.len() > prefix.len() + 8,
        "typed id must not be a short display token: {id}"
    );
    id.to_owned()
}

fn filesystem_manifest(root: &Path) -> BTreeSet<(PathBuf, [u8; 32])> {
    fn collect(root: &Path, path: &Path, entries: &mut BTreeSet<(PathBuf, [u8; 32])>) {
        let mut children = fs::read_dir(path)
            .expect("read manifest directory")
            .map(|entry| entry.expect("manifest entry").path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            let metadata = fs::symlink_metadata(&child).expect("manifest metadata");
            if metadata.is_dir() {
                collect(root, &child, entries);
            } else {
                let relative = child
                    .strip_prefix(root)
                    .expect("manifest relative path")
                    .to_owned();
                let bytes = if metadata.file_type().is_symlink() {
                    fs::read_link(&child)
                        .expect("manifest symlink")
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else {
                    fs::read(&child).expect("manifest file")
                };
                entries.insert((relative, Sha256::digest(bytes).into()));
            }
        }
    }

    let mut entries = BTreeSet::new();
    collect(root, root, &mut entries);
    entries
}

fn initialize_git(root: &Path) -> String {
    successful(root, "git", ["init", "-q", "-b", "main"]);
    successful(root, "git", ["config", "user.name", "JJK Contract Fixture"]);
    successful(
        root,
        "git",
        ["config", "user.email", "jjk-contract@example.invalid"],
    );
    fs::write(root.join("story.txt"), b"legacy state\n").expect("legacy fixture content");
    successful(root, "git", ["add", "story.txt"]);
    successful(root, "git", ["commit", "-qm", "legacy fixture"]);
    String::from_utf8(successful(root, "git", ["rev-parse", "HEAD"]).stdout)
        .expect("HEAD UTF-8")
        .trim()
        .to_owned()
}

fn legacy_v1(head: &str) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1,
        "safeSpaceId": "safe-contract-fixture",
        "createdAt": "2025-01-01T00:00:00Z",
        "updatedAt": "2025-01-02T00:00:00Z",
        "settings": {
            "watchDebounceMs": 100,
            "autoStatePrefix": "auto",
            "showWorkspaceSnapshotsInGit": false
        },
        "states": [{
            "id": "green001",
            "kind": "save",
            "label": "green",
            "description": "known good",
            "createdAt": "2025-01-01T01:00:00Z",
            "branch": "main",
            "lane": "main",
            "continuationBranch": null,
            "commit": head,
            "parentCommit": null,
            "parentStateId": null,
            "tags": ["star"],
            "stats": { "changedFiles": 1, "insertedLines": 1, "deletedLines": 0 },
            "metadata": { "gitCommit": head, "message": "green" }
        }],
        "lanes": {
            "main": {
                "name": "main",
                "branch": "main",
                "baseRef": "main",
                "createdAt": "2025-01-01T00:00:00Z",
                "updatedAt": "2025-01-02T00:00:00Z",
                "currentStateId": "green001"
            }
        },
        "branchLaneMap": { "main": "main" },
        "allowMainBranchSave": true,
        "returnContext": { "stateId": "green001", "sourceBranch": "main", "sourceLane": "main" },
        "currentStateHistory": { "entries": ["green001", "green001"], "index": 1 },
        "timeshifts": [{
            "id": "time0001",
            "label": "shell",
            "createdAt": "2025-01-02T00:00:00Z",
            "branch": "main",
            "lane": "main",
            "stateId": "green001",
            "relativeCwd": ".",
            "env": { "TERM": "xterm" }
        }],
        "freezes": []
    }))
    .expect("serialize legacy fixture")
}

fn assert_jj_report(report: &Value, state: &str) {
    assert_eq!(report["state"], state);
    assert_eq!(report["executable"], "jj");
    assert_eq!(report["git_only_complete"], true);
    for field in [
        "version",
        "colocated",
        "workspace_root",
        "git_root",
        "operation_log_readable",
        "operation_id",
        "diagnostic",
    ] {
        assert!(
            report.get(field).is_some(),
            "JJ capability report omitted {field}"
        );
    }
}

#[cfg(unix)]
fn isolated_tool_path(root: &Path, jj_script: Option<&str>) -> TempDir {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = TempDir::new_in(root).expect("isolated tools directory");
    let git = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .expect("Git executable on PATH");
    symlink(git, directory.path().join("git")).expect("Git tool link");
    if let Some(script) = jj_script {
        let path = directory.path().join("jj");
        fs::write(&path, script).expect("JJ fixture executable");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("JJ fixture permissions");
    }
    directory
}

#[test]
fn setup_legacy_migration_is_previewed_preserved_idempotent_and_rollback_safe() {
    let directory = TempDir::new().expect("legacy repository");
    let root = directory.path();
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    let head = initialize_git(root);
    let legacy = legacy_v1(&head);
    fs::create_dir(root.join(".jjk")).expect("legacy control directory");
    let source = root.join(".jjk/repo.json");
    fs::write(&source, &legacy).expect("legacy source");
    let source_sha256 = hex::encode(Sha256::digest(&legacy));
    let database = root.join(".git/jjk/state.sqlite3");
    let before_plain = filesystem_manifest(root);

    let plain = run(root, &jjk, ["setup", "--json"]);
    assert_eq!(
        plain.status.code(),
        Some(3),
        "plain setup must refuse detected legacy metadata"
    );
    assert_eq!(
        fs::read(&source).expect("legacy bytes after refusal"),
        legacy
    );
    assert_eq!(
        filesystem_manifest(root),
        before_plain,
        "plain setup refusal must not mutate the legacy repository"
    );
    assert!(!database.exists());
    let before_check = filesystem_manifest(root);

    let check = json(&successful(
        root,
        &jjk,
        ["setup", "--migration=check", "--json"],
    ));
    assert_eq!(check["command"], "setup");
    assert_eq!(check["migration"]["action"], "check");
    assert!(check["migration"]["migration_id"].is_null());
    assert!(
        check["migration"]["source_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    let input_sha256 = check["migration"]["input_sha256"]
        .as_str()
        .expect("migration input checksum");
    assert_eq!(
        hex::decode(input_sha256)
            .expect("hex migration checksum")
            .len(),
        32
    );
    assert_eq!(
        hex::encode(Sha256::digest(
            fs::read(&source).expect("source checksum bytes")
        )),
        source_sha256
    );
    let entity_counts = check["migration"]["entities"]
        .as_object()
        .expect("migration entity counts");
    for kind in ["repository", "state", "attempt", "branch", "timeshift"] {
        assert!(
            entity_counts
                .get(kind)
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
            "migration omitted {kind} count"
        );
    }
    assert_eq!(check["migration"]["quarantined"], 0);
    assert!(check["migration"]["rollback_capsule"].is_null());
    assert_eq!(check["migration"]["already_imported"], false);
    assert_eq!(fs::read(&source).expect("legacy bytes after check"), legacy);
    assert_eq!(
        filesystem_manifest(root),
        before_check,
        "migration check must perform zero filesystem writes"
    );
    assert!(
        !database.exists(),
        "migration check must not activate a database"
    );

    let applied = json(&successful(
        root,
        &jjk,
        ["setup", "--migration=apply", "--json"],
    ));
    assert_eq!(applied["migration"]["action"], "apply");
    let migration_id = assert_typed_id(&applied["migration"]["migration_id"], "mig_");
    assert_eq!(applied["migration"]["input_sha256"], input_sha256);
    assert_eq!(applied["migration"]["already_imported"], false);
    let capsule_text = applied["migration"]["rollback_capsule"]
        .as_str()
        .expect("rollback capsule");
    let capsule = PathBuf::from(capsule_text);
    let capsule_path = if capsule.is_absolute() {
        capsule.clone()
    } else {
        root.join(&capsule)
    };
    assert!(
        capsule_path.exists(),
        "apply must preserve a rollback capsule before activation"
    );
    assert!(database.is_file());
    assert_eq!(fs::read(&source).expect("legacy bytes after apply"), legacy);
    let events_after_apply =
        json(&successful(root, &jjk, ["doctor", "--json"]))["journal_events"].clone();

    let repeated = json(&successful(
        root,
        &jjk,
        ["setup", "--migration=apply", "--json"],
    ));
    assert_eq!(repeated["migration"]["migration_id"], migration_id);
    assert_eq!(repeated["migration"]["already_imported"], true);
    assert_eq!(
        json(&successful(root, &jjk, ["doctor", "--json"]))["journal_events"],
        events_after_apply
    );
    assert_eq!(
        fs::read(&source).expect("legacy bytes after repeat"),
        legacy
    );

    fs::remove_file(&source).expect("simulate missing legacy source");
    let rolled_back = json(&successful(
        root,
        &jjk,
        ["setup", "--migration=rollback", "--json"],
    ));
    assert_eq!(rolled_back["migration"]["action"], "rollback");
    assert_eq!(rolled_back["migration"]["migration_id"], migration_id);
    assert_eq!(rolled_back["migration"]["rollback_capsule"], capsule_text);
    assert_eq!(fs::read(&source).expect("restored legacy source"), legacy);
    assert!(
        database.is_file(),
        "rollback must not delete the current database"
    );
}

#[cfg(unix)]
#[test]
fn doctor_and_status_report_explicit_git_only_and_optional_jj_capabilities() {
    let repo = Repo::new(&[("story.txt", b"capabilities\n")]);

    let absent_tools = isolated_tool_path(&repo.root, None);
    let absent_doctor = json(&successful_with_path(
        &repo.root,
        &repo.jjk,
        ["doctor", "--format", "json", "--no-color", "--width=80"],
        absent_tools.path(),
    ));
    assert_jj_report(&absent_doctor["jj"], "absent");
    let absent_status = json(&successful_with_path(
        &repo.root,
        &repo.jjk,
        ["status", "--format=json", "--no-color", "--width", "80"],
        absent_tools.path(),
    ));
    assert_eq!(absent_status["jj"], absent_doctor["jj"]);

    let degraded_tools = isolated_tool_path(
        &repo.root,
        Some("#!/bin/sh\necho 'broken jj fixture' >&2\nexit 42\n"),
    );
    let degraded = json(&successful_with_path(
        &repo.root,
        &repo.jjk,
        ["doctor", "--json"],
        degraded_tools.path(),
    ));
    assert_jj_report(&degraded["jj"], "degraded");
    assert!(
        degraded["jj"]["diagnostic"]
            .as_str()
            .is_some_and(|value| value.contains("broken jj fixture"))
    );

    fs::create_dir(repo.root.join(".jj")).expect("colocated JJ marker");
    let root = repo.root.to_string_lossy();
    let git_root =
        fs::canonicalize(repo.root.join(".git")).expect("canonical Git common directory");
    let git_root = git_root.to_string_lossy();
    let present_script = format!(
        "#!/bin/sh\ncase \"$1\" in\n  --version) [ \"$#\" -eq 1 ] || exit 64; echo 'jj 0.32.0' ;;\n  --ignore-working-copy)\n    if [ \"$#\" -eq 2 ] && [ \"$2\" = root ]; then printf '%s\\n' '{root}'\n    elif [ \"$#\" -eq 3 ] && [ \"$2\" = git ] && [ \"$3\" = root ]; then printf '%s\\n' '{git_root}'\n    else exit 64; fi ;;\n  --at-operation=@)\n    [ \"$#\" -eq 9 ] && [ \"$2\" = --ignore-working-copy ] && [ \"$3\" = op ] && [ \"$4\" = log ] && [ \"$5\" = --limit ] && [ \"$6\" = 1 ] && [ \"$7\" = --no-graph ] && [ \"$8\" = -T ] || exit 64\n    [ \"$9\" = 'id ++ \"\\0\"' ] || exit 64\n    printf 'op-contract\\0' ;;\n  *) echo \"unexpected jj argv: $*\" >&2; exit 64 ;;\nesac\n"
    );
    let present_tools = isolated_tool_path(&repo.root, Some(&present_script));
    let present = json(&successful_with_path(
        &repo.root,
        &repo.jjk,
        ["doctor", "--json"],
        present_tools.path(),
    ));
    assert_jj_report(&present["jj"], "present");
    assert_eq!(present["jj"]["version"], "jj 0.32.0");
    assert_eq!(present["jj"]["colocated"], true);
    assert_eq!(present["jj"]["operation_log_readable"], true);
    assert_eq!(present["jj"]["operation_id"], "op-contract");
}

#[test]
fn completion_is_deterministic_registry_derived_and_has_no_repository_side_effects() {
    let directory = TempDir::new().expect("completion directory");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    let help = String::from_utf8(successful(directory.path(), &jjk, ["--help"]).stdout)
        .expect("help UTF-8");
    let commands = help
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let command = fields.next()?;
            matches!(fields.next(), Some("native" | "enhanced")).then(|| command.to_owned())
        })
        .collect::<Vec<_>>();
    let required = [
        "setup",
        "save",
        "step",
        "nice",
        "star",
        "unstar",
        "see",
        "return",
        "pick",
        "fork",
        "freeze",
        "current",
        "story",
        "back",
        "forward",
        "up",
        "down",
        "archive",
        "recover",
        "undo",
        "redo",
        "backup",
        "load",
        "handoff",
        "validate",
        "doctor",
        "completion",
        "status",
    ];
    assert_eq!(commands, required);

    let shell_markers = [
        ("bash", "complete"),
        ("zsh", "compdef"),
        ("fish", "complete -c jjk"),
        ("powershell", "Register-ArgumentCompleter"),
    ];
    for (shell, marker) in shell_markers {
        let first = successful(directory.path(), &jjk, ["completion", shell]);
        let second = successful(directory.path(), &jjk, ["completion", shell]);
        assert_eq!(
            first.stdout, second.stdout,
            "{shell} completion is not deterministic"
        );
        assert!(
            first.stderr.is_empty(),
            "{shell} completion wrote diagnostics on success"
        );
        let script = String::from_utf8(first.stdout).expect("completion UTF-8");
        assert!(
            script.contains(marker),
            "{shell} completion lacks its registration primitive"
        );
        for command in &commands {
            assert!(
                script.contains(command),
                "{shell} completion omitted registered command {command}"
            );
        }
        assert!(
            script.contains("git"),
            "{shell} completion must delegate unowned Git arguments"
        );
    }
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("completion directory")
            .count(),
        0,
        "completion initialized or mutated its cwd"
    );

    let invalid = run(
        directory.path(),
        &jjk,
        ["completion", "definitely-not-a-shell"],
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
}
