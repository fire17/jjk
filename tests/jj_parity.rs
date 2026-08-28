//! Black-box proof for VAL-JJ-001.
//!
//! Runtime grammar assumed here is the stable CLI form `COMMAND --json`, capture/fork
//! messages after `--`, and `return STATE --json`. Doctor JSON must contain a complete
//! `jj` object with lifecycle state plus `git_only_complete`; workflow comparison removes
//! substrate identities (Git OIDs, JJ IDs, UUIDs, paths) and retains semantic topology.
//! Every environment exercises the full Git-only workflow. When `jj` is discoverable in
//! the invoking PATH, the same workflow also runs in a real colocated repository and must
//! match; otherwise the asserted `absent` report is itself the supported coverage mode.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

#[derive(Debug)]
struct Sandbox {
    _temporary: TempDir,
    root: PathBuf,
    bin: PathBuf,
    home: PathBuf,
    xdg: PathBuf,
    git: PathBuf,
    jjk: PathBuf,
}

impl Sandbox {
    fn new(git: &Path, jj: Option<&Path>) -> Self {
        let temporary = TempDir::new().expect("create parity sandbox");
        let root = temporary.path().join("repo");
        let bin = temporary.path().join("bin");
        let home = temporary.path().join("home");
        let xdg = temporary.path().join("xdg");
        fs::create_dir_all(&root).expect("create repository root");
        fs::create_dir_all(&bin).expect("create tool directory");
        fs::create_dir_all(&home).expect("create isolated home");
        fs::create_dir_all(&xdg).expect("create isolated XDG root");
        install_tool(git, &bin.join(tool_file_name("git")));
        if let Some(jj) = jj {
            install_tool(jj, &bin.join(tool_file_name("jj")));
        }
        Self {
            _temporary: temporary,
            root,
            bin,
            home,
            xdg,
            git: git.to_path_buf(),
            jjk: assert_cmd::cargo::cargo_bin!("jjk").to_path_buf(),
        }
    }

    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.xdg.join("config"))
            .env("XDG_STATE_HOME", self.xdg.join("state"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                self.root
                    .parent()
                    .expect("sandbox parent")
                    .join("gitconfig"),
            )
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env("NO_COLOR", "1")
            .env(
                "PATH",
                env::join_paths([&self.bin]).expect("single sandbox PATH"),
            );
        command
    }

    fn run(&self, program: &Path, args: &[&str]) -> Output {
        self.command(program)
            .args(args)
            .output()
            .expect("launch command")
    }

    fn successful(&self, program: &Path, args: &[&str]) -> Output {
        let output = self.run(program, args);
        assert!(
            output.status.success(),
            "{} {} failed with {:?}\nstdout:\n{}\nstderr:\n{}",
            program.display(),
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }

    fn git(&self, args: &[&str]) -> Output {
        self.successful(&self.git, args)
    }

    fn jj(&self, args: &[&str]) -> Output {
        self.successful(Path::new("jj"), args)
    }

    fn jjk_json(&self, args: &[&str]) -> Value {
        let output = self.successful(&self.jjk, args);
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "jjk {} did not emit JSON: {error}\nstdout:\n{}\nstderr:\n{}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            )
        })
    }

    fn initialize_git(&self) {
        self.git(&["init", "-q", "-b", "main"]);
        fs::write(self.root.join("story.txt"), "base\n").expect("write base fixture");
        self.git(&["add", "story.txt"]);
        self.git(&[
            "-c",
            "user.name=JJK parity fixture",
            "-c",
            "user.email=jjk-parity@example.invalid",
            "commit",
            "-qm",
            "base",
        ]);
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CanonicalState {
    kind: String,
    label: String,
    logical_parent_label: Option<String>,
    archived: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct SemanticOutcome {
    setup_created: bool,
    captures: Vec<(String, String)>,
    states: Vec<CanonicalState>,
    returned_to: String,
    current_after_return: String,
    fork_from: String,
    fork_objective: String,
    fork_source_checkout_mutated: bool,
    fork_materialized: bool,
    current_file: String,
}

fn execute_workflow(sandbox: &Sandbox, expected_jj_state: &str) -> (SemanticOutcome, Value) {
    let setup = sandbox.jjk_json(&["setup", "--json"]);
    assert_eq!(setup["command"], "setup");
    assert_eq!(setup["created"], true);
    assert!(
        setup["repository_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );

    // Capability observation deliberately precedes every semantic mutation in this fixture.
    let doctor = sandbox.jjk_json(&["doctor", "--json"]);
    assert_eq!(doctor["command"], "doctor");
    assert_eq!(doctor["healthy"], true);
    assert_jj_report(&doctor, expected_jj_state);
    assert_eq!(
        doctor["journal_events"], 0,
        "capability must be observed before the first semantic mutation"
    );

    fs::write(sandbox.root.join("story.txt"), "green\n").expect("write green state");
    sandbox.git(&["add", "story.txt"]);
    let green = sandbox.jjk_json(&["step", "--json", "--", "green state"]);
    assert_capture(&green, "step", "green-state", sandbox);

    fs::write(sandbox.root.join("story.txt"), "purple\n").expect("write purple state");
    sandbox.git(&["add", "story.txt"]);
    let purple = sandbox.jjk_json(&["nice", "--json", "--", "purple state"]);
    assert_capture(&purple, "nice", "purple-state", sandbox);

    let green_id = required_string(&green, "state_id");
    let returned = sandbox.jjk_json(&["return", green_id, "--json"]);
    assert_eq!(returned["command"], "return");
    assert_eq!(returned["state_id"], green["state_id"]);
    assert_eq!(
        fs::read_to_string(sandbox.root.join("story.txt")).expect("read returned tree"),
        "green\n"
    );

    let forked = sandbox.jjk_json(&["fork", "--json", "--", "try alternate"]);
    assert_eq!(forked["command"], "fork");
    assert_eq!(forked["from_state"], green["state_id"]);
    assert_eq!(forked["objective"], "try alternate");
    assert_eq!(forked["source_checkout_mutated"], false);
    assert!(
        forked["attempt_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );

    let current = sandbox.jjk_json(&["current", "--json"]);
    assert_eq!(current["state_id"], green["state_id"]);
    let graph = sandbox.jjk_json(&["see", "--json"]);
    let states = canonical_states(&graph);

    sandbox.git(&["fsck", "--full"]);
    let status = sandbox.git(&["status", "--porcelain=v2"]);
    assert!(
        status.stdout.is_empty(),
        "workflow left Git dirty: {}",
        String::from_utf8_lossy(&status.stdout)
    );

    (
        SemanticOutcome {
            setup_created: setup["created"].as_bool().expect("setup.created boolean"),
            captures: vec![
                (
                    required_string(&green, "command").to_owned(),
                    required_string(&green, "label").to_owned(),
                ),
                (
                    required_string(&purple, "command").to_owned(),
                    required_string(&purple, "label").to_owned(),
                ),
            ],
            states,
            returned_to: label_for_state(&graph, required_string(&returned, "state_id")),
            current_after_return: label_for_state(&graph, required_string(&current, "state_id")),
            fork_from: label_for_state(&graph, required_string(&forked, "from_state")),
            fork_objective: required_string(&forked, "objective").to_owned(),
            fork_source_checkout_mutated: forked["source_checkout_mutated"]
                .as_bool()
                .expect("fork mutation boolean"),
            fork_materialized: !forked["branch"].is_null() || !forked["worktree"].is_null(),
            current_file: fs::read_to_string(sandbox.root.join("story.txt"))
                .expect("read semantic result tree"),
        },
        doctor,
    )
}

fn assert_capture(value: &Value, command: &str, label: &str, sandbox: &Sandbox) {
    assert_eq!(value["command"], command);
    assert_eq!(value["label"], label);
    for key in ["state_id", "state_ref", "commit", "attempt_id"] {
        assert!(
            value[key].as_str().is_some_and(|field| !field.is_empty()),
            "missing capture field {key}: {value}"
        );
    }
    sandbox.git(&[
        "cat-file",
        "-e",
        &format!("{}^{{commit}}", required_string(value, "commit")),
    ]);
}

fn canonical_states(graph: &Value) -> Vec<CanonicalState> {
    let rows = graph["states"].as_array().expect("see.states array");
    let labels: BTreeMap<&str, &str> = rows
        .iter()
        .map(|state| {
            (
                required_string(state, "state_id"),
                required_string(state, "label"),
            )
        })
        .collect();
    let mut states: Vec<_> = rows
        .iter()
        .map(|state| CanonicalState {
            kind: required_string(state, "kind").to_owned(),
            label: required_string(state, "label").to_owned(),
            logical_parent_label: state["logical_parent"]
                .as_str()
                .map(|id| {
                    labels
                        .get(id)
                        .copied()
                        .unwrap_or_else(|| panic!("unknown logical parent {id}"))
                })
                .map(str::to_owned),
            archived: state["archived"].as_bool().expect("state.archived boolean"),
        })
        .collect();
    states.sort_by(|left, right| left.label.cmp(&right.label));
    states
}

fn label_for_state(graph: &Value, id: &str) -> String {
    graph["states"]
        .as_array()
        .expect("see.states array")
        .iter()
        .find(|state| state["state_id"] == id)
        .map(|state| required_string(state, "label").to_owned())
        .unwrap_or_else(|| panic!("state {id} absent from semantic graph"))
}

fn assert_jj_report(doctor: &Value, expected_state: &str) {
    let report = doctor.get("jj").expect("doctor.jj capability report");
    assert_eq!(report["state"], expected_state);
    assert_eq!(
        report["git_only_complete"], true,
        "Git-only completeness must be explicit in every JJ lifecycle state"
    );
    assert_eq!(report["executable"], "jj");
    assert!(
        report.get("version").is_some(),
        "jj.version must exist even when null"
    );
    assert!(
        report.get("colocated").and_then(Value::as_bool).is_some(),
        "jj.colocated boolean"
    );
    assert!(
        report.get("workspace_root").is_some(),
        "jj.workspace_root must exist even when null"
    );
    assert!(
        report.get("git_root").is_some(),
        "jj.git_root must exist even when null"
    );
    assert!(
        report
            .get("operation_log_readable")
            .and_then(Value::as_bool)
            .is_some(),
        "jj.operation_log_readable boolean"
    );
    assert!(
        report.get("operation_id").is_some(),
        "jj.operation_id must exist even when null"
    );
    assert!(
        report["diagnostic"]
            .as_str()
            .is_some_and(|diagnostic| !diagnostic.is_empty()),
        "JJ diagnostic must be loud and non-empty"
    );

    match expected_state {
        "absent" => {
            assert!(report["version"].is_null());
            assert_eq!(report["colocated"], false);
            assert_eq!(report["operation_log_readable"], false);
            assert!(
                required_string(report, "diagnostic")
                    .contains("Git-only operation is fully available")
            );
        }
        "degraded" => {
            assert_eq!(report["operation_log_readable"], false);
            assert!(
                required_string(report, "diagnostic").contains("Git-only operation is unaffected")
            );
        }
        "present" => {
            assert!(
                report["version"]
                    .as_str()
                    .is_some_and(|version| !version.is_empty())
            );
            assert_eq!(report["colocated"], true);
            assert_eq!(report["operation_log_readable"], true);
            assert!(
                report["workspace_root"]
                    .as_str()
                    .is_some_and(|path| !path.is_empty())
            );
            assert!(
                report["git_root"]
                    .as_str()
                    .is_some_and(|path| !path.is_empty())
            );
            assert!(
                report["operation_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
            );
        }
        other => panic!("unsupported expected JJ state {other}"),
    }
}

#[test]
fn git_only_is_an_explicit_complete_mode_and_real_jj_has_semantic_parity_when_available() {
    let git = resolve_program("git").expect("Git is a required test prerequisite");
    let git_only = Sandbox::new(&git, None);
    git_only.initialize_git();
    let (git_only_outcome, git_only_doctor) = execute_workflow(&git_only, "absent");
    assert_eq!(git_only_doctor["jj"]["git_only_complete"], true);

    if let Some(jj) = resolve_program("jj") {
        let colocated = Sandbox::new(&git, Some(&jj));
        colocated.jj(&["git", "init", "--colocate", "."]);
        // `jj git init` creates Git when needed; add the same base history before JJK setup.
        fs::write(colocated.root.join("story.txt"), "base\n")
            .expect("write colocated base fixture");
        colocated.git(&["add", "story.txt"]);
        colocated.git(&[
            "-c",
            "user.name=JJK parity fixture",
            "-c",
            "user.email=jjk-parity@example.invalid",
            "commit",
            "-qm",
            "base",
        ]);
        let (jj_outcome, jj_doctor) = execute_workflow(&colocated, "present");
        assert_eq!(jj_doctor["jj"]["colocated"], true);
        assert_eq!(jj_doctor["jj"]["operation_log_readable"], true);
        assert_eq!(
            git_only_outcome, jj_outcome,
            "Git-only and colocated-JJ storage may differ, but setup/capture/return/fork semantics may not"
        );
        eprintln!(
            "JJ parity coverage: exercised installed {}",
            required_string(&jj_doctor["jj"], "version")
        );
    } else {
        // This is not a skip: the supported absence mode completed the entire golden workflow.
        assert_eq!(git_only_doctor["jj"]["state"], "absent");
        assert_eq!(git_only_outcome.current_file, "green\n");
        eprintln!(
            "JJ parity coverage: JJ unavailable; exercised complete Git-only setup/capture/return/fork workflow"
        );
    }
}

#[test]
fn broken_jj_degrades_loudly_before_semantic_mutation_and_git_stays_usable() {
    let git = resolve_program("git").expect("Git is a required test prerequisite");
    let current_test = env::current_exe().expect("resolve deliberately broken executable shim");
    let sandbox = Sandbox::new(&git, Some(&current_test));
    sandbox.initialize_git();
    sandbox.jjk_json(&["setup", "--json"]);

    let before = git_fingerprint(&sandbox);
    let doctor = sandbox.jjk_json(&["doctor", "--json"]);
    assert_jj_report(&doctor, "degraded");
    assert_eq!(
        doctor["journal_events"], 0,
        "broken JJ must degrade before any semantic mutation"
    );
    assert_eq!(
        before,
        git_fingerprint(&sandbox),
        "read-only JJ degradation detection mutated Git"
    );

    fs::write(sandbox.root.join("story.txt"), "git-only after broken JJ\n")
        .expect("write post-degradation state");
    sandbox.git(&["add", "story.txt"]);
    let captured = sandbox.jjk_json(&["step", "--json", "--", "after broken jj"]);
    assert_capture(&captured, "step", "after-broken-jj", &sandbox);
    sandbox.git(&["fsck", "--full"]);
    sandbox.git(&[
        "-c",
        "user.name=JJK parity fixture",
        "-c",
        "user.email=jjk-parity@example.invalid",
        "commit",
        "-qm",
        "native Git remains usable",
    ]);
    let status = sandbox.git(&["status", "--porcelain=v2"]);
    assert!(
        status.stdout.is_empty(),
        "native Git commit left repository dirty: {}",
        String::from_utf8_lossy(&status.stdout)
    );
}

fn git_fingerprint(sandbox: &Sandbox) -> Vec<Vec<u8>> {
    [
        ["rev-parse", "HEAD"].as_slice(),
        ["write-tree"].as_slice(),
        ["status", "--porcelain=v2", "--branch"].as_slice(),
        ["for-each-ref", "--format=%(refname)%00%(objectname)"].as_slice(),
    ]
    .into_iter()
    .map(|args| sandbox.git(args).stdout)
    .collect()
}

fn required_string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} must be a string in {value}"))
}

fn resolve_program(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        for candidate in executable_candidates(name) {
            let candidate = directory.join(candidate);
            if candidate.is_file() {
                return fs::canonicalize(&candidate).ok().or(Some(candidate));
            }
        }
    }
    None
}

fn executable_candidates(name: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        let mut candidates = vec![OsString::from(format!("{name}.exe"))];
        candidates.push(OsString::from(name));
        candidates
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from(name)]
    }
}

fn tool_file_name(name: &str) -> OsString {
    #[cfg(windows)]
    {
        OsString::from(format!("{name}.exe"))
    }
    #[cfg(not(windows))]
    {
        OsString::from(name)
    }
}

fn install_tool(source: &Path, destination: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, destination).unwrap_or_else(|error| {
            panic!(
                "link {} as {}: {error}",
                source.display(),
                destination.display()
            )
        });
    }
    #[cfg(not(unix))]
    {
        fs::copy(source, destination).unwrap_or_else(|error| {
            panic!(
                "copy {} as {}: {error}",
                source.display(),
                destination.display()
            )
        });
    }
}
