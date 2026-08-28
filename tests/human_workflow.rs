use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

const STABLE_COMMANDS: &[&str] = &[
    "setup",
    "save",
    "step",
    "nice",
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
const BEGINNER_LOOP: &[&str] = &["setup", "save", "nice", "see", "return", "pick"];
const UNSTABLE_COMMANDS: &[&str] = &[
    "show",
    "diff",
    "worktree",
    "timeshift",
    "star",
    "repair",
    "reconcile",
    "promote",
];

struct Repository {
    _directory: TempDir,
    root: PathBuf,
    home: PathBuf,
    jjk: PathBuf,
}

impl Repository {
    fn git_only() -> Self {
        let directory = TempDir::new().expect("temporary repository root");
        let root = directory.path().join("repo");
        let home = directory.path().join("home");
        fs::create_dir_all(&root).expect("create repository");
        fs::create_dir_all(home.join("xdg/config")).expect("create isolated configuration root");
        let repository = Self {
            _directory: directory,
            root,
            home,
            jjk: assert_cmd::cargo::cargo_bin!("jjk").to_path_buf(),
        };

        repository.git_success(&["init", "-q", "-b", "main"]);
        repository.git_success(&["config", "user.name", "JJK UX Fixture"]);
        repository.git_success(&["config", "user.email", "fixture@example.test"]);
        fs::write(repository.root.join("story.txt"), b"base\n")
            .expect("write deterministic base bytes");
        repository.git_success(&["add", "--", "story.txt"]);
        repository.git_success(&["commit", "-qm", "deterministic base"]);
        repository
    }

    fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("xdg/config"))
            .env("XDG_STATE_HOME", self.home.join("xdg/state"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.home.join("gitconfig"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat")
            .env("LC_ALL", "C")
            .env("TZ", "UTC")
            .env("TERM", "dumb")
            .env("NO_COLOR", "1")
            .env("GIT_AUTHOR_DATE", "2001-02-03T04:05:06Z")
            .env("GIT_COMMITTER_DATE", "2001-02-03T04:05:06Z");
        command
    }

    fn git(&self, args: &[&str]) -> Output {
        self.command(Path::new("git"))
            .args(args)
            .output()
            .expect("run real Git")
    }

    fn git_success(&self, args: &[&str]) -> Output {
        let output = self.git(args);
        assert_success(&output, &format!("git {}", args.join(" ")));
        output
    }

    fn jjk(&self, args: &[&str]) -> Output {
        self.command(&self.jjk)
            .args(args)
            .output()
            .expect("run compiled jjk")
    }

    fn jjk_with_width(&self, args: &[&str], width: usize) -> Output {
        self.command(&self.jjk)
            .env("COLUMNS", width.to_string())
            .args(args)
            .output()
            .expect("run compiled jjk with terminal width")
    }

    fn write_and_stage(&self, bytes: &[u8]) {
        fs::write(self.root.join("story.txt"), bytes).expect("write deterministic state bytes");
        self.git_success(&["add", "--", "story.txt"]);
    }
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

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("human output is UTF-8")
}

fn parse_json(output: &Output, invocation: &str) -> Value {
    assert_success(output, invocation);
    assert!(
        output.stderr.is_empty(),
        "{invocation} wrote stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{invocation} did not emit one JSON document: {error}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn state_id_from_capture(output: &Output, command: &str) -> String {
    let rendered = stdout(output);
    let prefix = format!("{command}:");
    let id = rendered
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|value| value.split_whitespace().next())
        .unwrap_or_else(|| {
            panic!("{command} output did not expose the saved state identity:\n{rendered}")
        });
    assert!(
        id.starts_with("st_"),
        "state identity is not recognizable: {id}"
    );
    id.to_owned()
}

fn short_state_id(id: &str) -> &str {
    &id[..id.len().min(14)]
}

fn assert_plain_bounded(output: &Output, invocation: &str, width: usize) -> String {
    assert_success(output, invocation);
    assert!(
        !output.stdout.contains(&0x1b),
        "{invocation} emitted ANSI on stdout under NO_COLOR"
    );
    assert!(
        !output.stderr.contains(&0x1b),
        "{invocation} emitted ANSI on stderr under NO_COLOR"
    );
    let rendered = stdout(output);
    for (index, line) in rendered.lines().enumerate() {
        let cells = line.chars().count();
        assert!(
            cells <= width,
            "{invocation} line {} is {cells} cells at width {width}: {line:?}",
            index + 1
        );
    }
    rendered
}

fn line_marks_current(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    normalized.contains("current")
        || normalized.contains("[here]")
        || normalized.contains("(here)")
        || line.trim_start().starts_with("* ")
        || line.trim_start().starts_with("*> ")
}

fn assert_graph_marks_only(output: &str, current: &str, other: &str) {
    let current_line = output
        .lines()
        .find(|line| line.contains(short_state_id(current)))
        .unwrap_or_else(|| panic!("graph omitted current state {current}:\n{output}"));
    let other_line = output
        .lines()
        .find(|line| line.contains(short_state_id(other)))
        .unwrap_or_else(|| panic!("graph omitted other state {other}:\n{output}"));
    assert!(
        line_marks_current(current_line),
        "current state has no non-color marker: {current_line:?}"
    );
    assert!(
        !line_marks_current(other_line),
        "non-current state uses the current marker: {other_line:?}"
    );
}

fn object_state_id(value: &Value) -> Option<&str> {
    value
        .get("state_id")
        .and_then(Value::as_str)
        .or_else(|| value.get("id").and_then(Value::as_str))
}

fn marker_names(value: &Value) -> Vec<&str> {
    value
        .get("markers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|marker| {
            marker
                .as_str()
                .or_else(|| marker.get("name").and_then(Value::as_str))
        })
        .collect()
}

fn graph_json_marks_current(value: &Value, state_id: &str) -> bool {
    match value {
        Value::Object(object) => {
            let direct_pointer = object.iter().any(|(key, child)| {
                key.to_ascii_lowercase().contains("current") && child.as_str() == Some(state_id)
            });
            let marked_node = object_state_id(value) == Some(state_id)
                && (value.get("current").and_then(Value::as_bool) == Some(true)
                    || marker_names(value).into_iter().any(|marker| {
                        marker.eq_ignore_ascii_case("current")
                            || marker.eq_ignore_ascii_case("here")
                    }));
            direct_pointer
                || marked_node
                || object
                    .values()
                    .any(|child| graph_json_marks_current(child, state_id))
        }
        Value::Array(values) => values
            .iter()
            .any(|child| graph_json_marks_current(child, state_id)),
        _ => false,
    }
}

fn current_json_identifies(value: &Value, state_id: &str) -> bool {
    if value.get("command").and_then(Value::as_str) == Some("current")
        && object_state_id(value) == Some(state_id)
    {
        return true;
    }
    value
        .get("result")
        .is_some_and(|result| current_json_identifies(result, state_id))
}

fn create_two_states(repository: &Repository) -> (String, String) {
    let setup = repository.jjk(&["setup"]);
    assert_success(&setup, "jjk setup");
    let setup_text = stdout(&setup).to_ascii_lowercase();
    assert!(
        setup_text.contains("safe space"),
        "setup did not explain the resulting safe space"
    );
    assert!(
        setup_text.contains("created") || setup_text.contains("ready"),
        "setup did not report readiness"
    );

    repository.write_and_stage(b"green\n");
    let green = repository.jjk(&[
        "step",
        "--",
        "green parser checkpoint with deterministic readable label",
    ]);
    assert_success(&green, "jjk step green");
    let green_id = state_id_from_capture(&green, "step");

    repository.write_and_stage(b"purple\n");
    let purple = repository.jjk(&[
        "step",
        "--",
        "purple parser checkpoint with deterministic readable label",
    ]);
    assert_success(&purple, "jjk step purple");
    let purple_id = state_id_from_capture(&purple, "step");
    assert_ne!(
        green_id, purple_id,
        "two captures reused one semantic identity"
    );
    (green_id, purple_id)
}

#[test]
fn scripted_fresh_user_can_orient_capture_inspect_return_and_diagnose() {
    let repository = Repository::git_only();
    let (green, purple) = create_two_states(&repository);

    let see = repository.jjk(&["see"]);
    assert_success(&see, "jjk see");
    let see_text = stdout(&see);
    assert!(
        see_text.contains("green-parser-checkpoint"),
        "see omitted the first remembered meaning"
    );
    assert!(
        see_text.contains("purple-parser-checkpoint"),
        "see omitted the second remembered meaning"
    );
    assert_graph_marks_only(&see_text, &purple, &green);

    let returned = repository.jjk(&["return", &green]);
    assert_success(&returned, "jjk return");
    let return_text = stdout(&returned);
    assert!(
        return_text.contains(&green),
        "return did not identify its exact destination"
    );
    assert_eq!(
        fs::read(repository.root.join("story.txt")).expect("read restored bytes"),
        b"green\n"
    );

    let current = repository.jjk(&["current"]);
    assert_success(&current, "jjk current");
    let current_text = stdout(&current).to_ascii_lowercase();
    assert!(
        current_text.contains("current"),
        "current output did not name the current concept"
    );
    assert!(
        current_text.contains(&green),
        "current output did not identify the returned state"
    );

    let status = repository.jjk(&["status"]);
    assert_success(&status, "jjk status");
    let status_text = stdout(&status).to_ascii_lowercase();
    assert!(
        status_text.contains("main"),
        "enhanced status suppressed native Git branch truth"
    );
    assert!(
        status_text.contains("current"),
        "enhanced status omitted the JJK current-state label"
    );
    assert!(
        status_text.contains(short_state_id(&green)),
        "enhanced status omitted the JJK current identity"
    );

    let doctor = repository.jjk(&["doctor"]);
    assert_success(&doctor, "jjk doctor");
    let doctor_text = stdout(&doctor).to_ascii_lowercase();
    assert!(
        doctor_text.contains("healthy"),
        "doctor did not provide a plain-language health verdict"
    );
    assert!(
        doctor_text.contains("journal") || doctor_text.contains("events"),
        "doctor omitted durable-state evidence"
    );
}

#[test]
fn human_output_is_plain_semantic_and_bounded_at_supported_widths() {
    let repository = Repository::git_only();
    let (green, purple) = create_two_states(&repository);
    let returned = repository.jjk(&["return", &green]);
    assert_success(&returned, "jjk return before width checks");

    for width in [40, 80, 120] {
        let see = assert_plain_bounded(
            &repository
                .jjk_with_width(&["see", "--width", &width.to_string(), "--no-color"], width),
            "jjk see",
            width,
        );
        assert_graph_marks_only(&see, &green, &purple);

        let current = assert_plain_bounded(
            &repository.jjk_with_width(
                &["current", "--width", &width.to_string(), "--no-color"],
                width,
            ),
            "jjk current",
            width,
        );
        assert!(
            current.to_ascii_lowercase().contains("current"),
            "width {width} lost current-state meaning"
        );

        let status = assert_plain_bounded(
            &repository.jjk_with_width(
                &["status", "--width", &width.to_string(), "--no-color"],
                width,
            ),
            "jjk status",
            width,
        );
        let status = status.to_ascii_lowercase();
        assert!(
            status.contains("current"),
            "width {width} status lost current-state meaning"
        );
        assert!(
            status.contains("main"),
            "width {width} status lost native Git branch truth"
        );

        let doctor = assert_plain_bounded(
            &repository.jjk_with_width(
                &["doctor", "--width", &width.to_string(), "--no-color"],
                width,
            ),
            "jjk doctor",
            width,
        );
        assert!(
            doctor.to_ascii_lowercase().contains("healthy"),
            "width {width} lost doctor verdict"
        );
    }
}

#[test]
fn piped_reads_are_deterministic_noninteractive_and_machine_json_marks_current() {
    let repository = Repository::git_only();
    let (green, _) = create_two_states(&repository);
    let returned = repository.jjk(&["return", &green]);
    assert_success(&returned, "jjk return before pipe checks");

    for args in [
        &["see"][..],
        &["current"][..],
        &["status"][..],
        &["doctor"][..],
    ] {
        let first = repository.jjk_with_width(args, 80);
        let second = repository.jjk_with_width(args, 80);
        assert_success(&first, &format!("jjk {}", args.join(" ")));
        assert_success(&second, &format!("jjk {} repeated", args.join(" ")));
        assert_eq!(
            first.stdout,
            second.stdout,
            "piped stdout changed across identical reads: {}",
            args.join(" ")
        );
        assert_eq!(
            first.stderr,
            second.stderr,
            "piped stderr changed across identical reads: {}",
            args.join(" ")
        );
        assert!(
            !first.stdout.contains(&0x1b),
            "piped output contained terminal control bytes: {}",
            args.join(" ")
        );
    }

    let current = parse_json(
        &repository.jjk(&["current", "--json"]),
        "jjk current --json",
    );
    assert!(
        current_json_identifies(&current, &green),
        "current JSON did not identify the current state textually: {current}"
    );

    let graph = parse_json(&repository.jjk(&["see", "--json"]), "jjk see --json");
    assert!(
        graph_json_marks_current(&graph, &green),
        "graph JSON did not distinguish the current state without color: {graph}"
    );

    let status = parse_json(&repository.jjk(&["status", "--json"]), "jjk status --json");
    assert!(
        graph_json_marks_current(&status, &green),
        "status JSON did not identify the current JJK state: {status}"
    );

    let doctor = parse_json(&repository.jjk(&["doctor", "--json"]), "jjk doctor --json");
    let serialized = serde_json::to_string(&doctor).expect("serialize parsed doctor response");
    assert!(
        serialized.contains("healthy"),
        "doctor JSON omitted its health verdict: {doctor}"
    );
    assert!(
        !serialized.contains('\u{1b}'),
        "JSON contained ANSI escape data"
    );
}

#[test]
fn uninitialized_error_gives_the_exact_executable_recovery_action() {
    let repository = Repository::git_only();
    let human = repository.jjk(&["current"]);
    assert!(
        !human.status.success(),
        "current unexpectedly succeeded before setup"
    );
    assert!(
        human.stdout.is_empty(),
        "human error polluted stdout: {}",
        String::from_utf8_lossy(&human.stdout)
    );
    assert_eq!(
        String::from_utf8(human.stderr).expect("human diagnostic UTF-8"),
        "jjk: error: JJK is not initialized; run `jjk setup`\n",
    );

    let machine = repository.jjk(&["current", "--json"]);
    assert!(
        !machine.status.success(),
        "JSON current unexpectedly succeeded before setup"
    );
    assert!(
        machine.stdout.is_empty(),
        "JSON failure must leave stdout empty"
    );
    let envelope: Value = serde_json::from_slice(&machine.stderr).unwrap_or_else(|error| {
        panic!(
            "machine error was not one stderr JSON document: {error}\n{}",
            String::from_utf8_lossy(&machine.stderr)
        )
    });
    let error = envelope.get("error").expect("machine envelope error");
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some("NOT_INITIALIZED")
    );
    assert_eq!(
        error.get("message").and_then(Value::as_str),
        Some("JJK is not initialized; run `jjk setup`"),
    );
    assert_eq!(
        error.get("recovery_commands"),
        Some(&serde_json::json!([["jjk", "setup"]])),
        "machine error did not expose the exact argv recovery action",
    );
}

fn top_level_command_rows(help: &str) -> BTreeSet<String> {
    let mut in_commands = false;
    let mut commands = BTreeSet::new();
    for line in help.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("commands:") || trimmed.eq_ignore_ascii_case("commands") {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        if trimmed.is_empty() && !commands.is_empty() {
            break;
        }
        if !line.chars().next().is_some_and(char::is_whitespace) || trimmed.ends_with(':') {
            continue;
        }
        let token = trimmed
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches(',')
            .to_ascii_lowercase();
        if !token.is_empty()
            && token
                .chars()
                .all(|character| character.is_ascii_lowercase() || character == '-')
        {
            commands.insert(token);
        }
    }
    commands
}

#[test]
fn help_is_progressive_and_never_advertises_outside_the_stable_registry() {
    let repository = Repository::git_only();
    let overview = repository.jjk(&["--help"]);
    assert_success(&overview, "jjk --help");
    let help = stdout(&overview);
    let lower = help.to_ascii_lowercase();
    let rows = top_level_command_rows(&help);

    for command in BEGINNER_LOOP {
        assert!(
            rows.contains(*command),
            "default help omitted beginner verb `{command}`:\n{help}"
        );
    }
    let stable = STABLE_COMMANDS.iter().copied().collect::<BTreeSet<_>>();
    for command in rows
        .iter()
        .map(String::as_str)
        .filter(|command| !matches!(*command, "help" | "version"))
    {
        assert!(
            stable.contains(command),
            "default help advertised non-stable command `{command}`:\n{help}"
        );
    }
    for command in UNSTABLE_COMMANDS {
        assert!(
            !rows.contains(*command),
            "default help advertised experimental or unimplemented command `{command}`"
        );
    }
    assert!(
        lower.contains("help <command>")
            || lower.contains("help <owned>")
            || lower.contains("more")
            || lower.contains("next"),
        "default help gave no progressive path to command detail:\n{help}",
    );

    for command in STABLE_COMMANDS {
        let detail = repository.jjk(&["help", command]);
        assert_success(&detail, &format!("jjk help {command}"));
        assert!(
            detail.stderr.is_empty(),
            "help for `{command}` wrote stderr"
        );
        let detail = stdout(&detail);
        let normalized = detail.to_ascii_lowercase();
        assert!(
            normalized.contains(command),
            "help detail did not name `{command}`:\n{detail}"
        );
        assert!(
            normalized.contains("usage"),
            "help detail did not show executable usage for `{command}`:\n{detail}"
        );
        assert!(
            !normalized.contains("unavailable in this build"),
            "stable registry advertised unavailable `{command}`"
        );
        let detail_rows = top_level_command_rows(&detail);
        for unstable in UNSTABLE_COMMANDS {
            assert!(
                !detail_rows.contains(*unstable),
                "help for `{command}` advertised non-stable `{unstable}`",
            );
        }
    }
}
