use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};

struct IsolatedGit {
    home: PathBuf,
    xdg: PathBuf,
    global_config: PathBuf,
}

impl IsolatedGit {
    fn new(base: &Path) -> Self {
        let home = base.join("isolated-home");
        let xdg = base.join("isolated-xdg");
        fs::create_dir_all(&home).expect("isolated home");
        fs::create_dir_all(xdg.join("git")).expect("isolated XDG config");
        let global_config = home.join(".gitconfig");
        fs::write(
            &global_config,
            b"# isolated from the user's Git configuration\n",
        )
        .expect("isolated global config");
        fs::write(
            xdg.join("git/config"),
            b"# isolated XDG Git configuration\n",
        )
        .expect("isolated XDG config file");
        Self {
            home,
            xdg,
            global_config,
        }
    }

    fn command(&self, cwd: &Path, program: &Path, args: &[OsString]) -> Command {
        let mut command = Command::new(program);
        command.args(args).current_dir(cwd);
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_INDEX_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "GIT_CONFIG_VALUE_0",
            "GIT_CONFIG_SYSTEM",
            "GIT_EXTERNAL_DIFF",
        ] {
            command.env_remove(key);
        }
        command
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }

    fn run(&self, cwd: &Path, program: &Path, args: &[OsString]) -> Output {
        self.command(cwd, program, args)
            .output()
            .expect("execute command")
    }

    fn run_with_env(
        &self,
        cwd: &Path,
        program: &Path,
        args: &[OsString],
        extra: &[(&str, &OsStr)],
    ) -> Output {
        let mut command = self.command(cwd, program, args);
        for (key, value) in extra {
            command.env(key, value);
        }
        command.output().expect("execute command")
    }
}

fn argv(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn checked(git_env: &IsolatedGit, cwd: &Path, program: &Path, args: &[OsString]) -> Output {
    let output = git_env.run(cwd, program, args);
    assert_success(&output, &display_invocation(program, args));
    output
}

fn assert_success(output: &Output, invocation: &str) {
    assert!(
        output.status.success(),
        "{invocation} failed with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn assert_refused(output: &Output, operation: &str) {
    assert!(
        !output.status.success(),
        "{operation} unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn display_invocation(program: &Path, args: &[OsString]) -> String {
    format!(
        "{} {}",
        program.display(),
        args.iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("machine-readable JSON output")
}

fn init_repository(git_env: &IsolatedGit, root: &Path) {
    fs::create_dir_all(root).expect("repository directory");
    checked(
        git_env,
        root,
        Path::new("git"),
        &argv(&["init", "-q", "-b", "main"]),
    );
    fs::write(root.join("tracked.txt"), b"initial\n").expect("initial tracked file");
    checked(
        git_env,
        root,
        Path::new("git"),
        &argv(&["add", "tracked.txt"]),
    );
    checked(
        git_env,
        root,
        Path::new("git"),
        &argv(&[
            "-c",
            "user.name=JJK Security Fixture",
            "-c",
            "user.email=security-fixture@example.invalid",
            "commit",
            "-qm",
            "initial",
        ]),
    );
}

fn setup_repository(git_env: &IsolatedGit, root: &Path, jjk: &Path) -> Value {
    init_repository(git_env, root);
    json(&checked(git_env, root, jjk, &argv(&["setup", "--json"])))
}

fn capture(git_env: &IsolatedGit, root: &Path, jjk: &Path, contents: &str, message: &str) -> Value {
    fs::write(root.join("tracked.txt"), contents).expect("updated tracked file");
    checked(
        git_env,
        root,
        Path::new("git"),
        &argv(&["add", "tracked.txt"]),
    );
    json(&checked(
        git_env,
        root,
        jjk,
        &[
            OsString::from("step"),
            OsString::from("--json"),
            OsString::from("--"),
            OsString::from(message),
        ],
    ))
}

fn create_backup(git_env: &IsolatedGit, root: &Path, jjk: &Path, destination: &Path) -> Output {
    checked(
        git_env,
        root,
        jjk,
        &[
            OsString::from("backup"),
            OsString::from("create"),
            destination.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    )
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn assert_canaries_absent(surface: &str, bytes: &[u8], canaries: &[&str]) {
    for canary in canaries {
        assert!(
            !contains_bytes(bytes, canary.as_bytes()),
            "{surface} disclosed secret canary `{canary}`"
        );
    }
}

fn combined_output(output: &Output) -> Vec<u8> {
    let mut bytes = output.stdout.clone();
    bytes.extend_from_slice(&output.stderr);
    bytes
}

#[test]
fn backup_and_load_refuse_existing_targets_without_changing_them() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    setup_repository(&git_env, &root, &jjk);
    capture(&git_env, &root, &jjk, "recoverable\n", "recoverable state");

    let occupied_backup = directory.path().join("occupied.sqlite3");
    let backup_sentinel = b"existing backup target must survive byte-for-byte\n";
    fs::write(&occupied_backup, backup_sentinel).expect("occupied backup target");
    let refused_backup = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("backup"),
            OsString::from("create"),
            occupied_backup.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&refused_backup, "backup to an occupied target");
    assert_eq!(
        fs::read(&occupied_backup).expect("preserved backup target"),
        backup_sentinel
    );

    let valid_backup = directory.path().join("valid.sqlite3");
    create_backup(&git_env, &root, &jjk, &valid_backup);
    let backup_before_load = fs::read(&valid_backup).expect("valid backup bytes");
    let occupied_load = directory.path().join("occupied-load");
    fs::create_dir(&occupied_load).expect("occupied load target");
    let load_sentinel = occupied_load.join("unique-user-data");
    fs::write(&load_sentinel, b"never replace me\n").expect("load sentinel");
    let refused_load = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("load"),
            valid_backup.as_os_str().to_owned(),
            OsString::from("--into"),
            occupied_load.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&refused_load, "load into an occupied target");
    assert_eq!(
        fs::read(&load_sentinel).expect("preserved load sentinel"),
        b"never replace me\n"
    );
    assert_eq!(
        fs::read(&valid_backup).expect("backup after refused load"),
        backup_before_load
    );
}

#[cfg(unix)]
#[test]
fn backup_refuses_dangling_symlink_and_symlinked_parent_escapes() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    setup_repository(&git_env, &root, &jjk);

    let outside = directory.path().join("outside-declared-root");
    fs::create_dir(&outside).expect("outside directory");
    let escaped_via_final_link = outside.join("escaped-final.sqlite3");
    let declared_link = directory.path().join("declared-backup.sqlite3");
    symlink(&escaped_via_final_link, &declared_link).expect("dangling backup symlink");
    let final_link_attempt = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("backup"),
            OsString::from("create"),
            declared_link.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&final_link_attempt, "backup through a dangling symlink");
    assert!(
        fs::symlink_metadata(&declared_link)
            .expect("declared symlink remains")
            .file_type()
            .is_symlink()
    );
    assert!(
        !escaped_via_final_link.exists(),
        "backup followed a dangling symlink outside the declared target"
    );

    let escaped_via_parent = outside.join("escaped-parent.sqlite3");
    let declared_parent = directory.path().join("declared-parent");
    symlink(&outside, &declared_parent).expect("symlinked backup parent");
    let parent_attempt = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("backup"),
            OsString::from("create"),
            declared_parent
                .join("escaped-parent.sqlite3")
                .as_os_str()
                .to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&parent_attempt, "backup below a symlinked parent");
    assert!(
        !escaped_via_parent.exists(),
        "backup escaped through a symlinked parent directory"
    );
}

#[cfg(unix)]
#[test]
fn load_refuses_symlinked_parent_escape_without_publishing_outside_it() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    setup_repository(&git_env, &root, &jjk);
    let backup = directory.path().join("valid.sqlite3");
    create_backup(&git_env, &root, &jjk, &backup);
    let backup_before = fs::read(&backup).expect("backup before escape attempt");

    let outside = directory.path().join("outside-declared-root");
    fs::create_dir(&outside).expect("outside directory");
    let declared_parent = directory.path().join("declared-load-parent");
    symlink(&outside, &declared_parent).expect("symlinked load parent");
    let declared_target = declared_parent.join("restored");
    let actual_outside_target = outside.join("restored");
    let attempted = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("load"),
            backup.as_os_str().to_owned(),
            OsString::from("--into"),
            declared_target.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&attempted, "load below a symlinked parent");
    assert!(
        !actual_outside_target.exists(),
        "load published a repository outside the declared target path"
    );
    assert_eq!(
        fs::read(&backup).expect("backup after escape attempt"),
        backup_before
    );
}

#[test]
fn corrupt_and_wrong_application_backups_are_read_only_refusals() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    setup_repository(&git_env, &root, &jjk);

    let corrupt = directory.path().join("corrupt.sqlite3");
    create_backup(&git_env, &root, &jjk, &corrupt);
    fs::write(&corrupt, b"not a SQLite database; preserve this evidence\n")
        .expect("corrupt backup fixture");
    let corrupt_before = fs::read(&corrupt).expect("corrupt bytes");
    let corrupt_verify = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("backup"),
            OsString::from("verify"),
            corrupt.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&corrupt_verify, "verification of a corrupt backup");
    assert_eq!(
        fs::read(&corrupt).expect("corrupt backup after verification"),
        corrupt_before
    );
    let corrupt_target = directory.path().join("corrupt-target");
    let corrupt_load = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("load"),
            corrupt.as_os_str().to_owned(),
            OsString::from("--into"),
            corrupt_target.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&corrupt_load, "load of a corrupt backup");
    assert!(!corrupt_target.exists());
    assert_eq!(
        fs::read(&corrupt).expect("corrupt backup after load refusal"),
        corrupt_before
    );

    let foreign = directory.path().join("foreign-application.sqlite3");
    create_backup(&git_env, &root, &jjk, &foreign);
    let connection = rusqlite::Connection::open(&foreign).expect("open foreign fixture");
    connection
        .pragma_update(None, "application_id", 0x1357_2468_i32)
        .expect("mark as another SQLite application");
    drop(connection);
    let foreign_before = fs::read(&foreign).expect("foreign application bytes");
    let foreign_verify = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("backup"),
            OsString::from("verify"),
            foreign.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(
        &foreign_verify,
        "verification of another application's database",
    );
    assert!(
        String::from_utf8_lossy(&combined_output(&foreign_verify))
            .to_ascii_lowercase()
            .contains("application"),
        "wrong-application refusal did not explain the incompatibility"
    );
    assert_eq!(
        fs::read(&foreign).expect("foreign backup after verification"),
        foreign_before
    );
    let foreign_target = directory.path().join("foreign-target");
    let foreign_load = git_env.run(
        &root,
        &jjk,
        &[
            OsString::from("load"),
            foreign.as_os_str().to_owned(),
            OsString::from("--into"),
            foreign_target.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );
    assert_refused(&foreign_load, "load of another application's database");
    assert!(!foreign_target.exists());
    assert_eq!(
        fs::read(&foreign).expect("foreign backup after load refusal"),
        foreign_before
    );
}

#[test]
fn doctor_and_backup_surfaces_do_not_disclose_secret_canaries() {
    const WORKTREE_SECRET: &str = "JJK_SECRET_WORKTREE_4b6c3f07";
    const CONFIG_SECRET: &str = "JJK_SECRET_CONFIG_87b17d22";
    const CREDENTIAL_SECRET: &str = "JJK_SECRET_CREDENTIAL_a8802e31";
    const ENV_SECRET: &str = "JJK_SECRET_ENV_9fd04d5a";
    const CANARIES: &[&str] = &[
        WORKTREE_SECRET,
        CONFIG_SECRET,
        CREDENTIAL_SECRET,
        ENV_SECRET,
    ];

    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    init_repository(&git_env, &root);
    fs::write(root.join("private-untracked.txt"), WORKTREE_SECRET)
        .expect("untracked secret fixture");
    checked(
        &git_env,
        &root,
        Path::new("git"),
        &[
            OsString::from("config"),
            OsString::from("--local"),
            OsString::from("remote.origin.url"),
            OsString::from(format!(
                "https://{CONFIG_SECRET}@example.invalid/private.git"
            )),
        ],
    );
    fs::write(
        git_env.home.join(".git-credentials"),
        format!("https://user:{CREDENTIAL_SECRET}@example.invalid\n"),
    )
    .expect("isolated credential fixture");

    let extra = [("JJK_TEST_SECRET", OsStr::new(ENV_SECRET))];
    let setup = git_env.run_with_env(&root, &jjk, &argv(&["setup", "--json"]), &extra);
    assert_success(&setup, "jjk setup --json with secret canaries present");
    assert_canaries_absent("setup JSON output", &combined_output(&setup), CANARIES);

    let backup_human = directory.path().join("backup-human.sqlite3");
    let backup_json = directory.path().join("backup-json.sqlite3");
    let invocations = [
        argv(&["doctor"]),
        argv(&["doctor", "--json"]),
        vec![
            OsString::from("backup"),
            OsString::from("create"),
            backup_human.as_os_str().to_owned(),
        ],
        vec![
            OsString::from("backup"),
            OsString::from("verify"),
            backup_human.as_os_str().to_owned(),
        ],
        vec![
            OsString::from("backup"),
            OsString::from("create"),
            backup_json.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
        vec![
            OsString::from("backup"),
            OsString::from("verify"),
            backup_json.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    ];
    for invocation in invocations {
        let output = git_env.run_with_env(&root, &jjk, &invocation, &extra);
        let rendered = display_invocation(&jjk, &invocation);
        assert_success(&output, &rendered);
        assert_canaries_absent(&rendered, &combined_output(&output), CANARIES);
    }
    assert_canaries_absent(
        "human-mode backup artifact",
        &fs::read(&backup_human).expect("human-mode backup"),
        CANARIES,
    );
    assert_canaries_absent(
        "JSON-mode backup artifact",
        &fs::read(&backup_json).expect("JSON-mode backup"),
        CANARIES,
    );
}

#[cfg(unix)]
fn executable_script(path: &Path, body: &str) {
    fs::write(path, body).expect("malicious fixture script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("executable fixture script");
}

#[cfg(unix)]
#[test]
fn read_only_stable_commands_do_not_invoke_git_hooks_helpers_or_external_programs() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    setup_repository(&git_env, &root, &jjk);
    capture(&git_env, &root, &jjk, "state\n", "safe state");

    let hostile = directory.path().join("hostile-git-programs");
    let hooks = hostile.join("hooks");
    fs::create_dir_all(&hooks).expect("hostile hooks directory");
    let fsmonitor = hostile.join("fsmonitor");
    let external_diff = hostile.join("external-diff");
    let credential_helper = hostile.join("credential-helper");
    let askpass = hostile.join("askpass");
    let post_index_change = hooks.join("post-index-change");
    executable_script(
        &fsmonitor,
        "#!/bin/sh\nprintf invoked >> \"${0%/*}/fsmonitor-invoked\"\nprintf '\\0'\n",
    );
    executable_script(
        &external_diff,
        "#!/bin/sh\nprintf invoked >> \"${0%/*}/external-diff-invoked\"\nexit 97\n",
    );
    executable_script(
        &credential_helper,
        "#!/bin/sh\nprintf invoked >> \"${0%/*}/credential-helper-invoked\"\nexit 97\n",
    );
    executable_script(
        &askpass,
        "#!/bin/sh\nprintf invoked >> \"${0%/*}/askpass-invoked\"\nexit 97\n",
    );
    executable_script(
        &post_index_change,
        "#!/bin/sh\nprintf invoked >> \"${0%/*}/../hook-invoked\"\nexit 97\n",
    );

    for (key, value) in [
        ("core.hooksPath", hooks.as_os_str()),
        ("credential.helper", credential_helper.as_os_str()),
        ("diff.external", external_diff.as_os_str()),
    ] {
        checked(
            &git_env,
            &root,
            Path::new("git"),
            &[
                OsString::from("config"),
                OsString::from("--local"),
                OsString::from(key),
                value.to_owned(),
            ],
        );
    }
    checked(
        &git_env,
        &root,
        Path::new("git"),
        &[
            OsString::from("config"),
            OsString::from("--local"),
            OsString::from("core.fsmonitor"),
            fsmonitor.as_os_str().to_owned(),
        ],
    );

    for marker in [
        hostile.join("fsmonitor-invoked"),
        hostile.join("external-diff-invoked"),
        hostile.join("credential-helper-invoked"),
        hostile.join("askpass-invoked"),
        hostile.join("hook-invoked"),
    ] {
        if marker.exists() {
            fs::remove_file(&marker).expect("clear fixture setup marker before read-only commands");
        }
    }
    let local_config_before = fs::read(root.join(".git/config")).expect("local config");

    let read_only = [
        argv(&["current", "--json"]),
        argv(&["see", "--json"]),
        argv(&["story", "--json"]),
        argv(&["doctor"]),
        argv(&["doctor", "--json"]),
        argv(&["status", "--json"]),
    ];
    let extra = [("GIT_ASKPASS", askpass.as_os_str())];
    for invocation in read_only {
        let output = git_env.run_with_env(&root, &jjk, &invocation, &extra);
        assert_success(&output, &display_invocation(&jjk, &invocation));
        let hook_marker = hostile.join("hook-invoked");
        assert!(
            !hook_marker.exists(),
            "{} invoked post-index-change hook",
            display_invocation(&jjk, &invocation)
        );
    }

    for marker in [
        hostile.join("fsmonitor-invoked"),
        hostile.join("external-diff-invoked"),
        hostile.join("credential-helper-invoked"),
        hostile.join("askpass-invoked"),
        hostile.join("hook-invoked"),
    ] {
        assert!(
            !marker.exists(),
            "read-only JJK command invoked malicious Git integration: {}",
            marker.display()
        );
    }
    assert_eq!(
        fs::read(root.join(".git/config")).expect("local config after reads"),
        local_config_before,
        "read-only JJK commands changed repository Git configuration"
    );
}

#[test]
fn deep_subdirectories_stop_at_nested_repository_boundaries() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    let outer = directory.path().join("outer");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    let outer_setup = setup_repository(&git_env, &outer, &jjk);
    let outer_state = capture(
        &git_env,
        &outer,
        &jjk,
        "outer state\n",
        "OUTER_BOUNDARY_CANARY",
    );

    let outer_deep = outer.join("ordinary/deep/subdirectory");
    fs::create_dir_all(&outer_deep).expect("outer deep directory");
    let nested = outer.join("vendor/nested");
    let nested_setup = setup_repository(&git_env, &nested, &jjk);
    let nested_state = capture(
        &git_env,
        &nested,
        &jjk,
        "nested state\n",
        "NESTED_BOUNDARY_CANARY",
    );
    let nested_deep = nested.join("deep/inside/repository");
    fs::create_dir_all(&nested_deep).expect("nested deep directory");

    assert_ne!(
        outer_setup["repository_id"], nested_setup["repository_id"],
        "nested repository reused the outer safe-space identity"
    );
    let outer_current = json(&checked(
        &git_env,
        &outer_deep,
        &jjk,
        &argv(&["current", "--json"]),
    ));
    let nested_current = json(&checked(
        &git_env,
        &nested_deep,
        &jjk,
        &argv(&["current", "--json"]),
    ));
    assert_eq!(outer_current["state_id"], outer_state["state_id"]);
    assert_eq!(nested_current["state_id"], nested_state["state_id"]);

    let outer_graph = json(&checked(
        &git_env,
        &outer_deep,
        &jjk,
        &argv(&["see", "--json"]),
    ));
    let nested_graph = json(&checked(
        &git_env,
        &nested_deep,
        &jjk,
        &argv(&["see", "--json"]),
    ));
    let outer_rendered = serde_json::to_string(&outer_graph).expect("outer graph JSON");
    let nested_rendered = serde_json::to_string(&nested_graph).expect("nested graph JSON");
    assert!(outer_rendered.contains("OUTER_BOUNDARY_CANARY"));
    assert!(!outer_rendered.contains("NESTED_BOUNDARY_CANARY"));
    assert!(nested_rendered.contains("NESTED_BOUNDARY_CANARY"));
    assert!(!nested_rendered.contains("OUTER_BOUNDARY_CANARY"));

    let nested_doctor = json(&checked(
        &git_env,
        &nested_deep,
        &jjk,
        &argv(&["doctor", "--json"]),
    ));
    assert_eq!(nested_doctor["healthy"], true);
}

#[derive(Debug, Eq, PartialEq)]
enum TreeEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    fn visit(root: &Path, current: &Path, snapshot: &mut BTreeMap<PathBuf, TreeEntry>) {
        let mut entries = fs::read_dir(current)
            .expect("snapshot directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("snapshot entries");
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("relative snapshot path")
                .to_owned();
            let metadata = fs::symlink_metadata(&path).expect("snapshot metadata");
            if metadata.file_type().is_symlink() {
                snapshot.insert(
                    relative,
                    TreeEntry::Symlink(fs::read_link(&path).expect("snapshot symlink")),
                );
            } else if metadata.is_dir() {
                snapshot.insert(relative, TreeEntry::Directory);
                visit(root, &path, snapshot);
            } else {
                snapshot.insert(
                    relative,
                    TreeEntry::File(fs::read(&path).expect("snapshot file")),
                );
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

#[test]
fn stable_workflow_never_mutates_isolated_global_git_configuration() {
    let directory = TempDir::new().expect("tempdir");
    let git_env = IsolatedGit::new(directory.path());
    fs::write(
        &git_env.global_config,
        b"# GLOBAL_CONFIG_MUTATION_CANARY\n[user]\n\tname = Global Sentinel\n\temail = global@example.invalid\n",
    )
    .expect("global config canary");
    fs::write(
        git_env.home.join(".git-credentials"),
        b"https://global:GLOBAL_CREDENTIAL_MUTATION_CANARY@example.invalid\n",
    )
    .expect("global credentials canary");
    fs::write(
        git_env.xdg.join("git/config"),
        b"# XDG_CONFIG_MUTATION_CANARY\n[credential]\n\tuseHttpPath = true\n",
    )
    .expect("XDG config canary");

    let home_before = snapshot_tree(&git_env.home);
    let xdg_before = snapshot_tree(&git_env.xdg);
    let root = directory.path().join("source");
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    init_repository(&git_env, &root);
    let local_config_before = fs::read(root.join(".git/config")).expect("local config before JJK");

    checked(&git_env, &root, &jjk, &argv(&["setup", "--json"]));
    checked(&git_env, &root, &jjk, &argv(&["setup", "--json"]));
    capture(
        &git_env,
        &root,
        &jjk,
        "configuration-safe state\n",
        "configuration safe state",
    );
    checked(&git_env, &root, &jjk, &argv(&["current", "--json"]));
    checked(&git_env, &root, &jjk, &argv(&["doctor", "--json"]));
    let backup = directory.path().join("configuration-safe.sqlite3");
    create_backup(&git_env, &root, &jjk, &backup);
    checked(
        &git_env,
        &root,
        &jjk,
        &[
            OsString::from("backup"),
            OsString::from("verify"),
            backup.as_os_str().to_owned(),
            OsString::from("--json"),
        ],
    );

    assert_eq!(
        snapshot_tree(&git_env.home),
        home_before,
        "JJK workflow changed or created a file in isolated HOME"
    );
    assert_eq!(
        snapshot_tree(&git_env.xdg),
        xdg_before,
        "JJK workflow changed or created an XDG Git configuration file"
    );
    assert_eq!(
        fs::read(root.join(".git/config")).expect("local config after JJK"),
        local_config_before,
        "JJK workflow changed repository Git configuration"
    );
}
