use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::OnceLock;

use jjk::cli::completion::generate_completion;
use jjk::cli::definition::command_descriptors;
use tempfile::TempDir;

static PACKAGE_FILES: OnceLock<Result<Vec<String>, String>> = OnceLock::new();

#[derive(Debug)]
struct Observation {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn jjk() -> PathBuf {
    assert_cmd::cargo::cargo_bin!("jjk").to_path_buf()
}
fn observe(cwd: &Path, program: &Path, args: &[&str]) -> Observation {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| {
            panic!("failed to execute {} {args:?}: {error}", program.display())
        });
    Observation {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn checked(cwd: &Path, program: &Path, args: &[&str]) -> Observation {
    let observation = observe(cwd, program, args);
    assert!(
        observation.status.success(),
        "{} {args:?} failed with {:?}\nstdout:\n{}\nstderr:\n{}",
        program.display(),
        observation.status.code(),
        String::from_utf8_lossy(&observation.stdout),
        String::from_utf8_lossy(&observation.stderr),
    );
    observation
}

fn package_files() -> Vec<String> {
    PACKAGE_FILES
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
            let output = Command::new(cargo)
                .args([
                    "package",
                    "--allow-dirty",
                    "--no-verify",
                    "--offline",
                    "--list",
                ])
                .current_dir(root)
                .output()
                .map_err(|error| format!("could not run cargo package: {error}"))?;
            if !output.status.success() {
                return Err(format!(
                    "cargo package --list failed with {:?}\nstdout:\n{}\nstderr:\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                ));
            }
            let files = String::from_utf8(output.stdout)
                .map_err(|error| format!("cargo package emitted non-UTF-8 paths: {error}"))?
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| line.replace('\\', "/"))
                .collect::<Vec<_>>();
            if files.is_empty() {
                return Err("cargo package produced an empty file list".to_owned());
            }
            Ok(files)
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("{error}"))
        .clone()
}

#[test]
fn help_and_version_are_derived_from_package_metadata() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = jjk();

    for argument in ["--version", "version"] {
        let version = checked(root, &binary, &[argument]);
        assert_eq!(
            String::from_utf8(version.stdout).expect("UTF-8 version output"),
            format!("{} {}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        );
        assert!(version.stderr.is_empty(), "version wrote stderr");
    }

    let help = checked(root, &binary, &["--help"]);
    let help = String::from_utf8(help.stdout).expect("UTF-8 help output");
    assert_eq!(
        help.lines().next(),
        Some(format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).as_str()),
    );
    assert!(
        help.contains(env!("CARGO_PKG_DESCRIPTION")),
        "help does not use package description\n{help}"
    );
}

#[test]
fn stable_help_is_registry_exact_and_every_claim_reaches_an_implementation() {
    let root = TempDir::new().expect("temporary repository");
    let git = Path::new("git");
    let binary = jjk();
    checked(root.path(), git, &["init", "-q", "-b", "main"]);
    fs::write(root.path().join("story.txt"), "base\n").expect("write repository fixture");
    checked(root.path(), git, &["add", "story.txt"]);
    checked(
        root.path(),
        git,
        &[
            "-c",
            "user.name=Distribution Test",
            "-c",
            "user.email=distribution@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    );
    checked(root.path(), &binary, &["setup", "--json"]);
    checked(
        root.path(),
        &binary,
        &["step", "--json", "--", "distribution baseline"],
    );

    let descriptors = command_descriptors();
    let claims = descriptors
        .iter()
        .map(|descriptor| descriptor.claim.name)
        .collect::<Vec<_>>();
    let unique = claims.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        claims.len(),
        "registry contains duplicate command claims"
    );

    let help = checked(root.path(), &binary, &["--help"]);
    let help = String::from_utf8(help.stdout).expect("UTF-8 help");
    let rows = help_command_rows(&help);
    assert_eq!(
        rows.len(),
        claims.len(),
        "help command table must contain only registry claims\n{help}"
    );
    for descriptor in &descriptors {
        let matching = rows
            .iter()
            .filter(|(name, _)| name == descriptor.claim.name)
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "{} must appear exactly once in native help",
            descriptor.claim.name
        );
        assert!(
            matching[0].1.contains(descriptor.summary),
            "help summary drifted for {}",
            descriptor.claim.name
        );
    }
    let help_names = rows
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        help_names, unique,
        "native help and routing registry disagree"
    );

    let git_help = checked(root.path(), git, &["help", "-a"]);
    let git_help_text = String::from_utf8_lossy(&git_help.stdout);
    let git_verbs = git_help_verbs(&git_help_text);
    let registry = unique.iter().copied().collect::<BTreeSet<_>>();
    for verb in git_verbs.difference(&registry) {
        assert!(
            !help_names.contains(verb),
            "unclaimed Git verb `{verb}` leaked into native help"
        );
    }

    let probes = command_probes();
    for name in claims {
        let args = probes
            .get(name)
            .unwrap_or_else(|| panic!("missing executable probe for registry command `{name}`"));
        let result = observe(root.path(), &binary, args);
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let placeholder = format!("command `{name}` is unavailable in this build");
        assert!(
            !stdout.contains(&placeholder) && !stderr.contains(&placeholder),
            "advertised command `{name}` reached generic unavailable fallback\nstdout:\n{stdout}\nstderr:\n{stderr}",
        );
        assert_ne!(result.status.code(), Some(126), "{name} failed to execute");
        assert_ne!(result.status.code(), Some(127), "{name} failed to execute");
    }
}

#[test]
fn completion_cli_emits_the_registry_script_for_every_supported_shell() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = jjk();
    for shell in ["bash", "zsh", "fish", "powershell", "pwsh"] {
        let result = checked(root, &binary, &["completion", shell]);
        assert!(result.stderr.is_empty(), "completion {shell} wrote stderr");
        let actual = String::from_utf8(result.stdout).expect("completion output must be UTF-8");
        let expected = generate_completion(shell).expect("supported shell");
        assert_eq!(
            actual, expected,
            "release CLI completion drifted for {shell}"
        );
        assert!(!actual.trim().is_empty(), "completion {shell} was empty");
    }
}

#[test]
fn cargo_package_contains_release_assets_and_no_private_or_generated_trees() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = package_files();
    let packaged = files.iter().map(String::as_str).collect::<BTreeSet<_>>();

    for path in &files {
        let components = path.split('/').collect::<Vec<_>>();
        for forbidden in ["target", ".jjk", ".jj", ".worktrees"] {
            assert!(
                !components
                    .iter()
                    .any(|component| component.eq_ignore_ascii_case(forbidden)),
                "forbidden `{forbidden}` tree was packaged: {path}",
            );
        }
        assert!(
            !path.starts_with("legacy/"),
            "forbidden project legacy tree was packaged: {path}"
        );
        for component in components {
            let lower = component.to_ascii_lowercase();
            assert!(
                lower != ".env"
                    && !lower.starts_with(".env.")
                    && !lower.contains("credential")
                    && !lower.contains("secret"),
                "secret-bearing path was packaged: {path}",
            );
        }
    }

    for required in [
        "Cargo.toml",
        "Cargo.lock",
        "LICENSE",
        "CONTRACTS.md",
        "ARCHITECTURE.md",
        "README.md",
        "CHANGELOG.md",
        "assets/jjk-banner.svg",
        "scripts/install.sh",
        "scripts/uninstall.sh",
        "migrations/0001_initial.sql",
        "migrations/fixtures/legacy-v1-complete/repo.json",
    ] {
        assert!(
            packaged.contains(required),
            "required release asset absent from cargo package: {required}"
        );
    }

    for required in regular_files_below(root, "migrations") {
        assert!(
            packaged.contains(required.as_str()),
            "migration asset absent from cargo package: {required}"
        );
    }
}

#[test]
fn installer_and_release_workflow_share_one_archive_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let installer = fs::read_to_string(root.join("scripts/install.sh")).expect("read installer");
    let release = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("read release workflow");

    for asset in ["linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64"] {
        assert!(
            release.contains(&format!("asset: {asset}")),
            "release workflow does not build installer asset {asset}"
        );
    }
    assert!(
        installer.contains("jjk-${version}-${os}-${arch}.tar.gz"),
        "installer archive naming drifted"
    );
    assert!(
        release.contains("name=\"jjk-${TAG}-${ASSET}\""),
        "release archive directory naming drifted"
    );
    assert!(
        release.contains("\"dist/$name.tar.gz\""),
        "release no longer emits installer tarballs"
    );
    assert!(
        release.contains("$artifact.sha256"),
        "release no longer emits per-archive checksum files"
    );
    assert!(
        installer.contains("$base/$asset.sha256"),
        "installer no longer downloads the matching checksum"
    );
    assert!(
        installer.contains("$tmp/jjk-${version}-${os}-${arch}/jjk"),
        "installer extraction path differs from release archive root"
    );
}

#[test]
fn installer_rejects_non_release_tag_suffixes_before_network_access() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let installer = root.join("scripts/install.sh");
    for invalid in [
        "v0.1.1-stable",
        "v1.2.3.4",
        "v1.2",
        "1.2.3",
        "v1.2.3rc1",
        "v1..3",
    ] {
        let output = Command::new("sh")
            .arg(&installer)
            .env("JJK_VERSION", invalid)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("run installer validation");
        assert!(
            !output.status.success(),
            "installer accepted non-release tag {invalid}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("vMAJOR.MINOR.PATCH"),
            "installer emitted an unrelated failure for {invalid}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn homebrew_formula_installs_binary_from_release_archive_root() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let formula = fs::read_to_string(root.join("packaging/homebrew/jjk.rb.template"))
        .expect("read Homebrew formula template");
    assert!(
        formula.contains("bin.install Dir[\"jjk-v#{version}-*/jjk\"].fetch(0)"),
        "Homebrew must install the binary inside the versioned release archive root"
    );
}

fn help_command_rows(help: &str) -> Vec<(String, String)> {
    let mut lines = help.lines();
    assert!(
        lines.any(|line| line == "COMMANDS:"),
        "help has no command table\n{help}"
    );
    lines
        .take_while(|line| !line.trim().is_empty())
        .map(|line| {
            let trimmed = line.trim_start();
            let name = trimmed
                .split_whitespace()
                .next()
                .expect("command name")
                .to_owned();
            (name, trimmed.to_owned())
        })
        .collect()
}

fn git_help_verbs(help: &str) -> BTreeSet<&str> {
    help.lines()
        .filter(|line| line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|token| {
            token
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
        .collect()
}

fn command_probes() -> BTreeMap<&'static str, &'static [&'static str]> {
    BTreeMap::from([
        ("setup", &["setup", "--json"] as &[_]),
        ("save", &["save", "--json", "--", "save probe"] as &[_]),
        ("step", &["step", "--json", "--", "step probe"] as &[_]),
        ("nice", &["nice", "--json", "--", "nice probe"] as &[_]),
        ("star", &["star", "--json"] as &[_]),
        ("unstar", &["unstar", "--json"] as &[_]),
        ("see", &["see", "--json"] as &[_]),
        ("return", &["return", "missing-state", "--json"] as &[_]),
        ("pick", &["pick", "missing-state", "--json"] as &[_]),
        ("fork", &["fork", "--json", "--", "fork probe"] as &[_]),
        ("freeze", &["freeze", "--json"] as &[_]),
        ("current", &["current", "--json"] as &[_]),
        ("story", &["story", "--json"] as &[_]),
        ("back", &["back", "--json"] as &[_]),
        ("forward", &["forward", "--json"] as &[_]),
        ("up", &["up", "--json"] as &[_]),
        ("down", &["down", "--json"] as &[_]),
        ("archive", &["archive", "missing-state", "--json"] as &[_]),
        ("recover", &["recover", "missing-state", "--json"] as &[_]),
        ("undo", &["undo", "--json"] as &[_]),
        ("redo", &["redo", "--json"] as &[_]),
        (
            "backup",
            &["backup", "verify", "missing.sqlite3", "--json"] as &[_],
        ),
        ("load", &["load", "--json"] as &[_]),
        ("handoff", &["handoff", "--json"] as &[_]),
        ("validate", &["validate", "--json"] as &[_]),
        ("doctor", &["doctor", "--json"] as &[_]),
        ("completion", &["completion", "bash"] as &[_]),
        ("status", &["status", "--json"] as &[_]),
    ])
}

fn regular_files_below(root: &Path, relative: &str) -> Vec<String> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<String>) {
        let mut entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", directory.display()))
            .collect::<Result<Vec<_>, _>>()
            .expect("read directory entries");
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, files);
            } else if path.is_file() {
                files.push(
                    path.strip_prefix(root)
                        .expect("file below package root")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut files = Vec::new();
    visit(root, &root.join(relative), &mut files);
    files
}
