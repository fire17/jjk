use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

struct Sandbox {
    temporary: TempDir,
    home: PathBuf,
    xdg_config: PathBuf,
    global_config: PathBuf,
    askpass: PathBuf,
    jjk: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let temporary = TempDir::new().expect("create isolated repository sandbox");
        let home = temporary.path().join("home");
        let xdg_config = temporary.path().join("xdg/config");
        let global_config = temporary.path().join("config/global.gitconfig");
        let askpass = temporary.path().join("bin/fail-if-called");
        fs::create_dir_all(&home).expect("create isolated HOME");
        fs::create_dir_all(&xdg_config).expect("create isolated XDG_CONFIG_HOME");
        fs::create_dir_all(global_config.parent().expect("global config parent"))
            .expect("create isolated Git config directory");
        fs::create_dir_all(askpass.parent().expect("askpass parent"))
            .expect("create isolated helper directory");
        fs::write(&global_config, b"").expect("create empty isolated global Git config");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(
                &askpass,
                b"#!/bin/sh\necho 'credential prompt escaped repository fixture' >&2\nexit 97\n",
            )
            .expect("write failing askpass helper");
            fs::set_permissions(&askpass, fs::Permissions::from_mode(0o755))
                .expect("make askpass helper executable");
        }
        #[cfg(not(unix))]
        fs::write(&askpass, b"credential prompt escaped repository fixture")
            .expect("write askpass sentinel");

        Self {
            temporary,
            home,
            xdg_config,
            global_config,
            askpass,
            jjk: assert_cmd::cargo::cargo_bin!("jjk").to_path_buf(),
        }
    }

    fn root(&self) -> &Path {
        self.temporary.path()
    }

    fn run<I, S>(&self, program: impl AsRef<OsStr>, cwd: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg_config)
            .env("XDG_STATE_HOME", self.temporary.path().join("xdg/state"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", &self.askpass)
            .env("SSH_ASKPASS", &self.askpass)
            .env("GIT_AUTHOR_NAME", "JJK Repository Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "JJK Repository Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .env("GIT_AUTHOR_DATE", "2001-02-03T04:05:06Z")
            .env("GIT_COMMITTER_DATE", "2001-02-03T04:05:06Z")
            .env("GIT_EDITOR", "false")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env("NO_COLOR", "1");
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        #[cfg(windows)]
        if let Some(system_root) = std::env::var_os("SYSTEMROOT") {
            command.env("SYSTEMROOT", system_root);
        }
        command
            .output()
            .expect("execute repository fixture command")
    }

    fn git<I, S>(&self, cwd: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.run("git", cwd, args);
        assert!(
            output.status.success(),
            "Git command failed in {}: status={:?}\nstdout={}\nstderr={}",
            cwd.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }

    fn jjk<I, S>(&self, cwd: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.run(&self.jjk, cwd, args);
        assert!(
            output.status.success(),
            "JJK command failed in {}: status={:?}\nstdout={}\nstderr={}",
            cwd.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }

    fn init(&self, root: &Path) {
        fs::create_dir_all(root).expect("create repository root");
        self.git(root, ["init", "-q", "-b", "main"]);
    }

    fn commit(&self, root: &Path, message: &str) -> String {
        self.git(root, ["commit", "-qm", message]);
        text(&self.git(root, ["rev-parse", "HEAD"]))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileFact {
    path: String,
    kind: &'static str,
    mode: u32,
    content: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RepositoryFingerprint {
    head: Option<Vec<u8>>,
    symbolic_head: Option<Vec<u8>>,
    non_jjk_refs: Vec<u8>,
    index_entries: Option<Vec<u8>>,
    staged_patch: Option<Vec<u8>>,
    unstaged_patch: Option<Vec<u8>>,
    status: Option<Vec<u8>>,
    local_config: Vec<u8>,
    files: Vec<FileFact>,
}

fn output_or_none(output: Output) -> Option<Vec<u8>> {
    output.status.success().then_some(output.stdout)
}

fn git_output(sandbox: &Sandbox, cwd: &Path, args: &[&str]) -> Output {
    sandbox.run("git", cwd, args)
}

fn successful_bytes(sandbox: &Sandbox, cwd: &Path, args: &[&str]) -> Vec<u8> {
    let output = git_output(sandbox, cwd, args);
    assert!(
        output.status.success(),
        "Git observation {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    output.stdout
}

fn repository_fingerprint(sandbox: &Sandbox, cwd: &Path) -> RepositoryFingerprint {
    let bare = text(&sandbox.git(cwd, ["rev-parse", "--is-bare-repository"])) == "true";
    let worktree_root = if bare {
        None
    } else {
        Some(PathBuf::from(text(
            &sandbox.git(cwd, ["rev-parse", "--show-toplevel"]),
        )))
    };
    let refs = successful_bytes(
        sandbox,
        cwd,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)%00",
        ],
    );
    let non_jjk_refs = refs
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| !line.starts_with(b"refs/jjk/"))
        .flat_map(|line| line.iter().copied())
        .collect();
    let worktree_command = |args: &[&str]| {
        if bare {
            None
        } else {
            output_or_none(git_output(sandbox, cwd, args))
        }
    };
    let files = match worktree_root {
        Some(root) => filesystem_facts(&root, true),
        None => filesystem_facts(cwd, false),
    };

    RepositoryFingerprint {
        head: output_or_none(git_output(sandbox, cwd, &["rev-parse", "--verify", "HEAD"])),
        symbolic_head: output_or_none(git_output(sandbox, cwd, &["symbolic-ref", "-q", "HEAD"])),
        non_jjk_refs,
        index_entries: worktree_command(&["ls-files", "--stage", "-z", "--"]),
        staged_patch: worktree_command(&["diff", "--cached", "--binary", "--no-ext-diff"]),
        unstaged_patch: worktree_command(&["diff", "--binary", "--no-ext-diff"]),
        status: worktree_command(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--ignored=matching",
        ]),
        local_config: successful_bytes(
            sandbox,
            cwd,
            &["config", "--local", "--null", "--list", "--show-origin"],
        ),
        files,
    }
}

fn filesystem_facts(root: &Path, skip_dot_git: bool) -> Vec<FileFact> {
    let mut facts = Vec::new();
    collect_files(root, root, skip_dot_git, &mut facts);
    facts.sort_by(|left, right| left.path.cmp(&right.path));
    facts
}

fn collect_files(root: &Path, directory: &Path, skip_dot_git: bool, facts: &mut Vec<FileFact>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read fixture directory {}: {error}", directory.display()))
        .collect::<Result<Vec<_>, _>>()
        .expect("collect fixture directory entries");
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if skip_dot_git && entry.file_name() == OsStr::new(".git") {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).expect("inspect fixture path");
        if metadata.is_dir() {
            collect_files(root, &path, skip_dot_git, facts);
            continue;
        }
        let relative = path.strip_prefix(root).expect("fixture path beneath root");
        let (kind, content) = if metadata.file_type().is_symlink() {
            (
                "symlink",
                fs::read_link(&path)
                    .expect("read fixture symlink")
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec(),
            )
        } else if metadata.is_file() {
            ("file", fs::read(&path).expect("read fixture file"))
        } else {
            ("other", Vec::new())
        };
        facts.push(FileFact {
            path: relative.to_string_lossy().into_owned(),
            kind,
            mode: file_mode(&metadata),
            content,
        });
    }
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode() & 0o7777
}

#[cfg(not(unix))]
fn file_mode(metadata: &fs::Metadata) -> u32 {
    u32::from(metadata.permissions().readonly())
}

fn text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone())
        .expect("fixture command emits UTF-8")
        .trim()
        .to_owned()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "expected JSON output: {error}\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    })
}

fn graph_commits(graph: &Value) -> BTreeSet<String> {
    graph["states"]
        .as_array()
        .expect("graph states array")
        .iter()
        .map(|state| {
            state["commit"]
                .as_str()
                .expect("every imported state has a Git commit")
                .to_owned()
        })
        .collect()
}

fn reachable_commits(sandbox: &Sandbox, root: &Path) -> BTreeSet<String> {
    let mut commits = BTreeSet::new();
    for revision in ["--all", "HEAD"] {
        let output = sandbox.run("git", root, ["rev-list", revision]);
        if output.status.success() {
            commits.extend(
                String::from_utf8(output.stdout)
                    .expect("Git commit identifiers are UTF-8")
                    .lines()
                    .map(str::to_owned),
            );
        }
    }
    commits
}

fn assert_read_only(sandbox: &Sandbox, cwd: &Path, args: &[&str]) -> Output {
    let before = repository_fingerprint(sandbox, cwd);
    let output = sandbox.jjk(cwd, args);
    let after = repository_fingerprint(sandbox, cwd);
    assert_eq!(
        before,
        after,
        "read path `jjk {}` mutated repository facts",
        args.join(" ")
    );
    output
}

fn assert_global_config_untouched(sandbox: &Sandbox) {
    assert_eq!(
        fs::read(&sandbox.global_config).expect("read isolated global Git config"),
        b"",
        "JJK must never write user global Git configuration",
    );
}

#[test]
fn setup_creates_an_empty_git_safe_space_without_inventing_history() {
    let sandbox = Sandbox::new();
    let root = sandbox.root().join("empty-safe-space");
    fs::create_dir(&root).expect("create empty setup directory");
    let before_files = filesystem_facts(&root, true);

    let setup = json(&sandbox.jjk(&root, ["setup", "--json"]));
    assert_eq!(setup["created"], true);
    assert_eq!(
        fs::canonicalize(Path::new(
            setup["repository"].as_str().expect("repository path")
        ))
        .expect("canonical setup repository"),
        fs::canonicalize(&root).expect("canonical fixture root"),
    );
    assert_eq!(
        text(&sandbox.git(&root, ["rev-parse", "--is-inside-work-tree"])),
        "true"
    );
    assert!(
        sandbox
            .run("git", &root, ["rev-parse", "--verify", "HEAD"])
            .status
            .code()
            .is_some_and(|code| code != 0)
    );
    assert_eq!(filesystem_facts(&root, true), before_files);

    let graph = json(&assert_read_only(&sandbox, &root, &["see", "--json"]));
    assert_eq!(graph["states"].as_array().expect("states").len(), 0);
    assert_read_only(&sandbox, &root, &["status", "--json"]);
    assert_global_config_untouched(&sandbox);
}

#[test]
fn setup_preserves_an_unborn_repository_and_is_idempotent() {
    let sandbox = Sandbox::new();
    let root = sandbox.root().join("unborn");
    sandbox.init(&root);
    fs::write(root.join("untracked.txt"), b"unborn work\n").expect("write unborn worktree file");
    let before = repository_fingerprint(&sandbox, &root);

    let first = json(&sandbox.jjk(&root, ["setup", "--json"]));
    assert_eq!(first["created"], true);
    assert_eq!(repository_fingerprint(&sandbox, &root), before);
    let second = json(&sandbox.jjk(&root, ["setup", "--json"]));
    assert_eq!(second["created"], false);
    assert_eq!(first["repository_id"], second["repository_id"]);
    assert_eq!(repository_fingerprint(&sandbox, &root), before);
    assert!(
        sandbox
            .run("git", &root, ["rev-parse", "--verify", "HEAD"])
            .status
            .code()
            .is_some_and(|code| code != 0)
    );
    assert_eq!(
        graph_commits(&json(&assert_read_only(
            &sandbox,
            &root,
            &["see", "--json"]
        ))),
        BTreeSet::new()
    );
    assert_global_config_untouched(&sandbox);
}

#[test]
fn setup_imports_existing_sha1_history_once_without_mutating_git_facts() {
    let sandbox = Sandbox::new();
    let root = sandbox.root().join("sha1-history");
    sandbox.init(&root);
    fs::write(root.join("story.txt"), b"root\n").expect("write root revision");
    sandbox.git(&root, ["add", "story.txt"]);
    let root_commit = sandbox.commit(&root, "root");
    fs::write(root.join("story.txt"), b"main two\n").expect("write main revision");
    sandbox.git(&root, ["add", "story.txt"]);
    sandbox.commit(&root, "main two");
    sandbox.git(&root, ["branch", "feature", &root_commit]);
    sandbox.git(&root, ["checkout", "-q", "feature"]);
    fs::write(root.join("feature.txt"), b"feature\n").expect("write feature revision");
    sandbox.git(&root, ["add", "feature.txt"]);
    sandbox.commit(&root, "feature");
    sandbox.git(&root, ["tag", "fixture-tag"]);
    sandbox.git(&root, ["checkout", "-q", "main"]);
    fs::write(root.join("untracked.txt"), b"do not absorb\n").expect("write untracked fixture");

    assert_eq!(
        text(&sandbox.git(&root, ["rev-parse", "--show-object-format"])),
        "sha1"
    );
    let expected = reachable_commits(&sandbox, &root);
    let before = repository_fingerprint(&sandbox, &root);
    let first_setup = json(&sandbox.jjk(&root, ["setup", "--json"]));
    assert_eq!(repository_fingerprint(&sandbox, &root), before);
    let first_graph = json(&assert_read_only(&sandbox, &root, &["see", "--json"]));
    assert_eq!(graph_commits(&first_graph), expected);
    assert_eq!(
        first_graph["states"].as_array().expect("states").len(),
        expected.len()
    );

    let second_setup = json(&sandbox.jjk(&root, ["setup", "--json"]));
    assert_eq!(second_setup["created"], false);
    assert_eq!(first_setup["repository_id"], second_setup["repository_id"]);
    assert_eq!(repository_fingerprint(&sandbox, &root), before);
    let second_graph = json(&assert_read_only(&sandbox, &root, &["see", "--json"]));
    assert_eq!(
        second_graph, first_graph,
        "repeated setup must not create new semantic facts"
    );
    assert_global_config_untouched(&sandbox);
}

#[test]
fn sha256_setup_is_required_when_the_installed_git_can_create_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.root().join("sha256-history");
    let probe = sandbox.run(
        "git",
        sandbox.root(),
        [
            OsString::from("init"),
            OsString::from("-q"),
            OsString::from("--object-format=sha256"),
            root.as_os_str().to_owned(),
        ],
    );
    if !probe.status.success() {
        let version = text(&sandbox.git(sandbox.root(), ["--version"]));
        eprintln!(
            "UNSUPPORTED repository-shape capability: installed {version} cannot create a SHA-256 repository; git init reported: {}",
            String::from_utf8_lossy(&probe.stderr).trim(),
        );
        return;
    }

    assert_eq!(
        text(&sandbox.git(&root, ["rev-parse", "--show-object-format"])),
        "sha256"
    );
    fs::write(root.join("sha256.txt"), b"64 hexadecimal digits\n").expect("write SHA-256 fixture");
    sandbox.git(&root, ["add", "sha256.txt"]);
    let commit = sandbox.commit(&root, "sha256 root");
    assert_eq!(commit.len(), 64);
    let before = repository_fingerprint(&sandbox, &root);

    let setup = sandbox.run(&sandbox.jjk, &root, ["setup", "--json"]);
    assert!(
        setup.status.success(),
        "Git advertised SHA-256 by successfully creating and committing to the repository, but JJK setup failed: status={:?}\nstdout={}\nstderr={}",
        setup.status.code(),
        String::from_utf8_lossy(&setup.stdout),
        String::from_utf8_lossy(&setup.stderr),
    );
    assert_eq!(repository_fingerprint(&sandbox, &root), before);
    let graph = json(&assert_read_only(&sandbox, &root, &["see", "--json"]));
    assert_eq!(graph_commits(&graph), BTreeSet::from([commit]));
    assert_global_config_untouched(&sandbox);
}

#[test]
fn setup_imports_detached_head_without_attaching_or_moving_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.root().join("detached");
    sandbox.init(&root);
    fs::write(root.join("story.txt"), b"one\n").expect("write first revision");
    sandbox.git(&root, ["add", "story.txt"]);
    let first = sandbox.commit(&root, "first");
    fs::write(root.join("story.txt"), b"two\n").expect("write second revision");
    sandbox.git(&root, ["add", "story.txt"]);
    sandbox.commit(&root, "second");
    sandbox.git(&root, ["checkout", "-q", "--detach", &first]);
    let expected = reachable_commits(&sandbox, &root);
    let before = repository_fingerprint(&sandbox, &root);
    assert!(
        before.symbolic_head.is_none(),
        "fixture must start detached"
    );

    sandbox.jjk(&root, ["setup", "--json"]);
    assert_eq!(repository_fingerprint(&sandbox, &root), before);
    assert!(
        sandbox
            .run("git", &root, ["symbolic-ref", "-q", "HEAD"])
            .status
            .code()
            .is_some_and(|code| code != 0)
    );
    assert_eq!(text(&sandbox.git(&root, ["rev-parse", "HEAD"])), first);
    assert_eq!(
        graph_commits(&json(&assert_read_only(
            &sandbox,
            &root,
            &["see", "--json"]
        ))),
        expected,
    );
    assert_global_config_untouched(&sandbox);
}

#[test]
fn bare_repository_allows_passthrough_and_read_only_status_but_refuses_setup() {
    let sandbox = Sandbox::new();
    let bare = sandbox.root().join("archive.git");
    sandbox.git(
        sandbox.root(),
        [
            OsString::from("init"),
            OsString::from("--bare"),
            OsString::from("-q"),
            bare.as_os_str().to_owned(),
        ],
    );
    let before = repository_fingerprint(&sandbox, &bare);

    let direct = sandbox.git(&bare, ["rev-parse", "--is-bare-repository"]);
    let passthrough = sandbox.run(&sandbox.jjk, &bare, ["rev-parse", "--is-bare-repository"]);
    assert_eq!(passthrough.status.code(), direct.status.code());
    assert_eq!(passthrough.stdout, direct.stdout);
    assert_eq!(passthrough.stderr, direct.stderr);
    assert_eq!(repository_fingerprint(&sandbox, &bare), before);

    let status = json(&assert_read_only(&sandbox, &bare, &["status", "--json"]));
    assert_eq!(status["initialized"], false);
    let refused = sandbox.run(&sandbox.jjk, &bare, ["setup", "--json"]);
    assert_eq!(refused.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("non-bare Git worktree"),
        "bare setup refusal must diagnose the unsupported mutation exactly: {}",
        String::from_utf8_lossy(&refused.stderr),
    );
    assert_eq!(repository_fingerprint(&sandbox, &bare), before);
    assert!(
        !bare.join("jjk").exists(),
        "refused bare setup must not leave metadata behind"
    );
    assert_global_config_untouched(&sandbox);
}

#[test]
fn deep_subdirectory_invocation_discovers_the_repository_root_without_mutation() {
    let sandbox = Sandbox::new();
    let root = sandbox.root().join("monorepo");
    sandbox.init(&root);
    let deep = root.join("packages/alpha/src/internal/deeper");
    fs::create_dir_all(&deep).expect("create deep invocation path");
    fs::write(root.join("root.txt"), b"root\n").expect("write root fixture");
    fs::write(deep.join("module.txt"), b"module\n").expect("write nested fixture");
    sandbox.git(&root, ["add", "."]);
    sandbox.commit(&root, "monorepo root");
    fs::write(deep.join("untracked.txt"), b"local only\n").expect("write nested untracked file");
    let before = repository_fingerprint(&sandbox, &deep);

    let setup = json(&sandbox.jjk(&deep, ["setup", "--json"]));
    assert_eq!(
        fs::canonicalize(Path::new(
            setup["repository"].as_str().expect("repository path")
        ))
        .expect("canonical reported root"),
        fs::canonicalize(&root).expect("canonical fixture root"),
    );
    assert_eq!(repository_fingerprint(&sandbox, &deep), before);
    assert_read_only(&sandbox, &deep, &["status", "--json"]);
    let graph = json(&assert_read_only(&sandbox, &deep, &["see", "--json"]));
    assert_eq!(graph["states"].as_array().expect("states").len(), 1);
    assert_global_config_untouched(&sandbox);
}

#[test]
fn linked_worktrees_share_one_safe_space_without_mutating_either_checkout() {
    let sandbox = Sandbox::new();
    let primary = sandbox.root().join("primary");
    let linked = sandbox.root().join("linked worktree λ");
    sandbox.init(&primary);
    fs::write(primary.join("story.txt"), b"shared\n").expect("write shared fixture");
    sandbox.git(&primary, ["add", "story.txt"]);
    sandbox.commit(&primary, "shared base");
    sandbox.git(
        &primary,
        [
            OsString::from("worktree"),
            OsString::from("add"),
            OsString::from("-q"),
            OsString::from("-b"),
            OsString::from("linked/fixture"),
            linked.as_os_str().to_owned(),
        ],
    );
    let deep = linked.join("space dir/深い");
    fs::create_dir_all(&deep).expect("create linked-worktree invocation directory");
    let primary_before = repository_fingerprint(&sandbox, &primary);
    let linked_before = repository_fingerprint(&sandbox, &linked);

    let linked_setup = json(&sandbox.jjk(&deep, ["setup", "--json"]));
    assert_eq!(repository_fingerprint(&sandbox, &primary), primary_before);
    assert_eq!(repository_fingerprint(&sandbox, &linked), linked_before);
    let primary_setup = json(&sandbox.jjk(&primary, ["setup", "--json"]));
    assert_eq!(primary_setup["created"], false);
    assert_eq!(
        linked_setup["repository_id"],
        primary_setup["repository_id"]
    );
    assert_eq!(linked_setup["store"], primary_setup["store"]);
    assert_eq!(repository_fingerprint(&sandbox, &primary), primary_before);
    assert_eq!(repository_fingerprint(&sandbox, &linked), linked_before);
    assert_read_only(&sandbox, &primary, &["status", "--json"]);
    assert_read_only(&sandbox, &deep, &["see", "--json"]);
    assert_global_config_untouched(&sandbox);
}

#[test]
fn nested_repository_discovery_stops_at_the_nearest_repository_boundary() {
    let sandbox = Sandbox::new();
    let outer = sandbox.root().join("outer");
    sandbox.init(&outer);
    fs::write(outer.join("outer.txt"), b"outer\n").expect("write outer fixture");
    sandbox.git(&outer, ["add", "outer.txt"]);
    sandbox.commit(&outer, "outer root");

    let inner = outer.join("vendor/nested");
    sandbox.init(&inner);
    fs::write(inner.join("inner.txt"), b"inner\n").expect("write inner fixture");
    sandbox.git(&inner, ["add", "inner.txt"]);
    sandbox.commit(&inner, "inner root");
    let deep_inner = inner.join("src/deep");
    fs::create_dir_all(&deep_inner).expect("create nested invocation path");
    let outer_before = repository_fingerprint(&sandbox, &outer);
    let inner_before = repository_fingerprint(&sandbox, &inner);

    let inner_setup = json(&sandbox.jjk(&deep_inner, ["setup", "--json"]));
    assert_eq!(
        fs::canonicalize(Path::new(
            inner_setup["repository"]
                .as_str()
                .expect("inner repository")
        ))
        .expect("canonical reported inner repository"),
        fs::canonicalize(&inner).expect("canonical inner fixture"),
    );
    assert_eq!(repository_fingerprint(&sandbox, &outer), outer_before);
    assert_eq!(repository_fingerprint(&sandbox, &inner), inner_before);
    assert!(!outer.join(".git/jjk/state.sqlite3").exists());
    assert!(inner.join(".git/jjk/state.sqlite3").is_file());

    let outer_setup = json(&sandbox.jjk(&outer, ["setup", "--json"]));
    assert_ne!(inner_setup["repository_id"], outer_setup["repository_id"]);
    assert_ne!(inner_setup["store"], outer_setup["store"]);
    assert_eq!(repository_fingerprint(&sandbox, &outer), outer_before);
    assert_eq!(repository_fingerprint(&sandbox, &inner), inner_before);
    assert_read_only(&sandbox, &deep_inner, &["status", "--json"]);
    assert_read_only(&sandbox, &outer, &["see", "--json"]);
    assert_global_config_untouched(&sandbox);
}

#[cfg(unix)]
#[test]
fn setup_capture_reads_and_navigation_preserve_all_workspace_shapes() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let sandbox = Sandbox::new();
    let root = sandbox.root().join("workspace-shapes");
    sandbox.init(&root);
    fs::write(root.join(".gitignore"), b"*.ignored\n").expect("write ignore rules");
    fs::write(root.join("old-name.txt"), b"first name\n").expect("write rename source");
    fs::write(root.join("tool.sh"), b"#!/bin/sh\necho one\n").expect("write executable");
    fs::set_permissions(root.join("tool.sh"), fs::Permissions::from_mode(0o755))
        .expect("mark executable");
    symlink("old-name.txt", root.join("current-link")).expect("create tracked symlink");
    sandbox.git(&root, ["add", "."]);
    sandbox.commit(&root, "workspace shape base");
    fs::write(root.join("notes.local"), b"untracked evidence\n").expect("write untracked evidence");
    fs::write(root.join("cache.ignored"), b"ignored evidence\n").expect("write ignored evidence");
    let before_setup = repository_fingerprint(&sandbox, &root);

    sandbox.jjk(&root, ["setup", "--json"]);
    assert_eq!(repository_fingerprint(&sandbox, &root), before_setup);

    sandbox.git(&root, ["mv", "old-name.txt", "renamed.txt"]);
    fs::write(root.join("tool.sh"), b"#!/bin/sh\necho two\n").expect("update executable");
    fs::remove_file(root.join("current-link")).expect("replace tracked symlink");
    symlink("renamed.txt", root.join("current-link")).expect("retarget tracked symlink");
    sandbox.git(&root, ["add", "-A"]);
    let before_capture = repository_fingerprint(&sandbox, &root);
    let first = json(&sandbox.jjk(&root, ["step", "--json", "--", "renamed executable state"]));
    assert_eq!(
        repository_fingerprint(&sandbox, &root),
        before_capture,
        "capture may add JJK objects and refs but must not alter HEAD, index, worktree, or config facts",
    );
    assert_read_only(&sandbox, &root, &["see", "--json"]);
    let first_state_fingerprint = repository_fingerprint(&sandbox, &root);

    sandbox.git(&root, ["mv", "renamed.txt", "final-name.txt"]);
    fs::write(root.join("tool.sh"), b"#!/bin/sh\necho three\n").expect("update executable again");
    fs::remove_file(root.join("current-link")).expect("replace tracked symlink again");
    symlink("final-name.txt", root.join("current-link")).expect("retarget tracked symlink again");
    sandbox.git(&root, ["add", "-A"]);
    sandbox.jjk(&root, ["step", "--json", "--", "later workspace state"]);

    let state_id = first["state_id"].as_str().expect("captured state id");
    sandbox.jjk(
        &root,
        [
            OsString::from("return"),
            OsString::from(state_id),
            OsString::from("--json"),
        ],
    );
    assert_eq!(
        repository_fingerprint(&sandbox, &root),
        first_state_fingerprint,
        "return must restore the exact target index/worktree facts while retaining untracked and ignored bytes",
    );
    assert!(!root.join("old-name.txt").exists());
    assert!(!root.join("final-name.txt").exists());
    assert_eq!(
        fs::read(root.join("renamed.txt")).expect("read restored rename"),
        b"first name\n"
    );
    assert_eq!(
        fs::read_link(root.join("current-link")).expect("read restored symlink"),
        Path::new("renamed.txt")
    );
    assert_ne!(
        fs::metadata(root.join("tool.sh"))
            .expect("inspect restored executable")
            .permissions()
            .mode()
            & 0o111,
        0,
    );
    assert_eq!(
        fs::read(root.join("tool.sh")).expect("read restored executable"),
        b"#!/bin/sh\necho two\n"
    );
    assert_eq!(
        fs::read(root.join("notes.local")).expect("read untracked evidence"),
        b"untracked evidence\n"
    );
    assert_eq!(
        fs::read(root.join("cache.ignored")).expect("read ignored evidence"),
        b"ignored evidence\n"
    );
    assert_global_config_untouched(&sandbox);
}
