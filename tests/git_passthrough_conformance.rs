#![cfg(unix)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const VISIBLE_ENV: &str = "value with spaces";
const PROBE_STDOUT: &[u8] = b"shim stdout \x01\x7f end";
const PROBE_STDERR: &[u8] = b"shim stderr \x02 end";
const PROBE_EXIT: i32 = 37;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    home: PathBuf,
    direct: PathBuf,
    wrapped: PathBuf,
    shim: PathBuf,
    real_git: PathBuf,
    path: OsString,
}

impl Fixture {
    fn empty_twins() -> Self {
        let temp = TempDir::new().expect("create passthrough fixture");
        let root = temp.path().to_path_buf();
        let home = root.join("home");
        let direct = root.join("direct");
        let wrapped = root.join("wrapped");
        fs::create_dir_all(&home).expect("create isolated home");
        fs::create_dir_all(direct.join("nested")).expect("create direct working directory");
        fs::create_dir_all(wrapped.join("nested")).expect("create wrapped working directory");
        fs::write(direct.join("nested/sentinel"), b"untouched\n").expect("write direct sentinel");
        fs::write(wrapped.join("nested/sentinel"), b"untouched\n").expect("write wrapped sentinel");

        let real_git = resolve_git();
        let shim = install_recording_shim(&root);
        let path = shim_path(&shim);
        Self {
            _temp: temp,
            root,
            home,
            direct,
            wrapped,
            shim,
            real_git,
            path,
        }
    }

    fn cloned_twins() -> Self {
        let fixture = Self::empty_twins();
        fs::remove_dir_all(&fixture.direct).expect("replace direct directory with clone");
        fs::remove_dir_all(&fixture.wrapped).expect("replace wrapped directory with clone");

        let seed = fixture.root.join("seed");
        fs::create_dir(&seed).expect("create seed repository");
        fixture.real_git(&seed, &["-c", "init.defaultBranch=main", "init", "-q"], &[]);
        fs::write(seed.join("base.txt"), b"base\n").expect("write seed content");
        fixture.real_git(&seed, &["add", "--", "base.txt"], &[]);
        fixture.real_git(
            &seed,
            &[
                "-c",
                "user.name=JJK Passthrough Test",
                "-c",
                "user.email=jjk-passthrough@example.invalid",
                "commit",
                "-qm",
                "seed",
            ],
            &[
                ("GIT_AUTHOR_DATE", "2001-02-03T04:05:06Z"),
                ("GIT_COMMITTER_DATE", "2001-02-03T04:05:06Z"),
            ],
        );

        let seed_text = seed.to_str().expect("temporary path is UTF-8");
        let direct_text = fixture.direct.to_str().expect("temporary path is UTF-8");
        let wrapped_text = fixture.wrapped.to_str().expect("temporary path is UTF-8");
        fixture.real_git(
            &fixture.root,
            &["clone", "-q", "--no-hardlinks", seed_text, direct_text],
            &[],
        );
        fixture.real_git(
            &fixture.root,
            &["clone", "-q", "--no-hardlinks", seed_text, wrapped_text],
            &[],
        );
        fixture
    }

    fn real_git(&self, cwd: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(&self.real_git);
        isolated_environment(&mut command, &self.home, original_path());
        command.current_dir(cwd).args(args);
        for (name, value) in extra_env {
            command.env(name, value);
        }
        let output = command
            .output()
            .expect("run real Git while constructing fixture");
        assert!(
            output.status.success(),
            "real Git fixture command failed: git {args:?}\nstdout: {:?}\nstderr: {:?}",
            output.stdout,
            output.stderr,
        );
        output
    }

    fn compare_passthrough(
        &self,
        direct_args: &[OsString],
        jjk_args: &[OsString],
        mode: &str,
        exit: i32,
        extra_env: &[(&str, &str)],
    ) -> (Output, Output) {
        let direct_record = self.root.join("record-direct");
        let wrapped_record = self.root.join("record-wrapped");
        fs::create_dir_all(&direct_record).expect("create direct recording directory");
        fs::create_dir_all(&wrapped_record).expect("create wrapped recording directory");

        let mut direct = Command::new(&self.shim);
        isolated_environment(&mut direct, &self.home, self.path.clone());
        direct.current_dir(&self.direct).args(direct_args);
        configure_shim(
            &mut direct,
            &direct_record,
            &self.real_git,
            mode,
            exit,
            extra_env,
        );
        let direct_output = direct.output().expect("run direct recording Git shim");

        let mut wrapped = Command::new(assert_cmd::cargo::cargo_bin!("jjk"));
        isolated_environment(&mut wrapped, &self.home, self.path.clone());
        wrapped.current_dir(&self.wrapped).args(jjk_args);
        configure_shim(
            &mut wrapped,
            &wrapped_record,
            &self.real_git,
            mode,
            exit,
            extra_env,
        );
        let wrapped_output = wrapped
            .output()
            .expect("run jjk against recording Git shim");

        assert_eq!(
            recorded_args(&direct_record),
            os_bytes(direct_args),
            "direct Git argv recording"
        );
        assert_eq!(
            recorded_args(&wrapped_record),
            os_bytes(direct_args),
            "jjk must deliver original Git argv"
        );
        assert_recorded_context(&direct_record, &self.direct);
        assert_recorded_context(&wrapped_record, &self.wrapped);
        assert_eq!(
            fs::read(direct_record.join("env")).expect("read direct env record"),
            expected_env_record()
        );
        assert_eq!(
            fs::read(wrapped_record.join("env")).expect("read wrapped env record"),
            expected_env_record()
        );
        assert_outputs_equal(&direct_output, &wrapped_output);
        (direct_output, wrapped_output)
    }
}

#[test]
fn passthrough_preserves_native_argv_cwd_environment_output_and_exit_without_bootstrap() {
    let fixture = Fixture::empty_twins();
    let direct_before = tree_fingerprint(&fixture.direct);
    let wrapped_before = tree_fingerprint(&fixture.wrapped);
    let args = os_args(&[
        "-c",
        "alias.a-future-git-verb=!: this value is intentionally opaque",
        "a-future-git-verb",
        "",
        "argument with spaces",
        "--",
        "-c",
        "literal-after-separator",
    ]);

    let (direct, wrapped) = fixture.compare_passthrough(&args, &args, "probe", PROBE_EXIT, &[]);

    assert_eq!(direct.status.code(), Some(PROBE_EXIT));
    assert_eq!(wrapped.status.code(), Some(PROBE_EXIT));
    assert_eq!(direct.stdout, PROBE_STDOUT);
    assert_eq!(direct.stderr, PROBE_STDERR);
    assert_eq!(
        tree_fingerprint(&fixture.direct),
        direct_before,
        "direct shim changed its cwd"
    );
    assert_eq!(
        tree_fingerprint(&fixture.wrapped),
        wrapped_before,
        "jjk initialized, reconciled, or changed its cwd before passthrough"
    );
    assert!(!fixture.wrapped.join(".git").exists());
    assert!(!fixture.wrapped.join(".jj").exists());
    assert!(!fixture.wrapped.join(".jjk").exists());
}

#[test]
fn explicit_git_escape_removes_only_git_and_separator() {
    let fixture = Fixture::empty_twins();
    let git_args = os_args(&[
        "",
        "future-after-escape",
        "argument with spaces",
        "--",
        "tail",
    ]);
    let mut jjk_args = os_args(&["git", "--"]);
    jjk_args.extend(git_args.iter().cloned());

    fixture.compare_passthrough(&git_args, &jjk_args, "probe", PROBE_EXIT, &[]);

    fixture.compare_passthrough(&[], &os_args(&["git", "--"]), "probe", PROBE_EXIT, &[]);
}

#[test]
fn unowned_status_forms_are_not_stolen_by_enhanced_status() {
    let fixture = Fixture::empty_twins();
    for args in [
        os_args(&["status", "--short"]),
        os_args(&["status", "--porcelain=v2", "--branch"]),
        os_args(&["status", "--", "path with spaces"]),
    ] {
        fixture.compare_passthrough(&args, &args, "probe", PROBE_EXIT, &[]);
    }
}

#[test]
fn non_utf8_verb_and_arguments_reach_git_byte_for_byte() {
    let fixture = Fixture::empty_twins();
    let args = vec![
        OsString::from_vec(vec![b'f', b'u', b't', b'u', b'r', b'e', 0x80]),
        OsString::from_vec(vec![0xff, b' ', b'x']),
        OsString::new(),
        OsString::from("--"),
    ];

    fixture.compare_passthrough(&args, &args, "probe", PROBE_EXIT, &[]);
}

#[test]
fn representative_real_git_commands_have_identical_observable_side_effects() {
    let fixture = Fixture::cloned_twins();
    fs::write(
        fixture.direct.join("new file.txt"),
        b"passthrough content\n",
    )
    .expect("write direct change");
    fs::write(
        fixture.wrapped.join("new file.txt"),
        b"passthrough content\n",
    )
    .expect("write wrapped change");

    for args in [
        os_args(&[
            "config",
            "--local",
            "passthrough.marker",
            "value with spaces",
        ]),
        os_args(&["add", "--", "new file.txt"]),
    ] {
        fixture.compare_passthrough(&args, &args, "delegate", 0, &[]);
    }

    let commit = os_args(&[
        "-c",
        "user.name=JJK Passthrough Test",
        "-c",
        "user.email=jjk-passthrough@example.invalid",
        "commit",
        "-qm",
        "passthrough commit",
    ]);
    fixture.compare_passthrough(
        &commit,
        &commit,
        "delegate",
        0,
        &[
            ("GIT_AUTHOR_DATE", "2002-03-04T05:06:07Z"),
            ("GIT_COMMITTER_DATE", "2002-03-04T05:06:07Z"),
        ],
    );

    for args in [
        os_args(&["branch", "future/topic"]),
        os_args(&["tag", "passthrough-v1"]),
    ] {
        fixture.compare_passthrough(&args, &args, "delegate", 0, &[]);
    }

    for query in [
        ["rev-parse", "HEAD"].as_slice(),
        ["show-ref"].as_slice(),
        ["status", "--porcelain=v2", "--untracked-files=all"].as_slice(),
        ["config", "--local", "--get", "passthrough.marker"].as_slice(),
        ["show", "--format=", "--no-renames", "HEAD"].as_slice(),
    ] {
        let direct = fixture.real_git(&fixture.direct, query, &[]);
        let wrapped = fixture.real_git(&fixture.wrapped, query, &[]);
        assert_outputs_equal(&direct, &wrapped);
    }

    assert_eq!(
        fs::read(fixture.direct.join("new file.txt")).unwrap(),
        fs::read(fixture.wrapped.join("new file.txt")).unwrap()
    );
    assert!(
        !fixture.wrapped.join(".jj").exists(),
        "passthrough must not initialize JJ"
    );
    assert!(
        !fixture.wrapped.join(".jjk").exists(),
        "passthrough must not initialize or reconcile JJK metadata"
    );
}

fn configure_shim(
    command: &mut Command,
    record: &Path,
    real_git: &Path,
    mode: &str,
    exit: i32,
    extra_env: &[(&str, &str)],
) {
    command
        .env("JJK_RECORD_ARGS", record.join("args"))
        .env("JJK_RECORD_CWD", record.join("cwd"))
        .env("JJK_RECORD_ENV", record.join("env"))
        .env("JJK_REAL_GIT", real_git)
        .env("JJK_SHIM_MODE", mode)
        .env("JJK_SHIM_EXIT", exit.to_string())
        .env("JJK_VISIBLE", VISIBLE_ENV)
        .env("JJK_EMPTY", "");
    for (name, value) in extra_env {
        command.env(name, value);
    }
}

fn isolated_environment(command: &mut Command, home: &Path, path: OsString) {
    command
        .env_clear()
        .env("PATH", path)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("TZ", "UTC");
}

fn install_recording_shim(root: &Path) -> PathBuf {
    let bin = root.join("shim-bin");
    fs::create_dir(&bin).expect("create shim directory");
    let shim = bin.join("git");
    fs::write(
        &shim,
        br#"#!/bin/sh
: "${JJK_RECORD_ARGS:?}"
: "${JJK_RECORD_CWD:?}"
: "${JJK_RECORD_ENV:?}"
: > "$JJK_RECORD_ARGS"
for arg in "$@"; do
    printf '%s\0' "$arg" >> "$JJK_RECORD_ARGS"
done
pwd -P > "$JJK_RECORD_CWD"
printf '%s\0%s\0%s\0%s\0' "$JJK_VISIBLE" "$JJK_EMPTY" "${JJK_ABSENT-unset}" "$LC_ALL" > "$JJK_RECORD_ENV"
if [ "${JJK_SHIM_MODE-}" = delegate ]; then
    exec "$JJK_REAL_GIT" "$@"
fi
printf 'shim stdout \001\177 end'
printf 'shim stderr \002 end' >&2
exit "${JJK_SHIM_EXIT-0}"
"#,
    )
    .expect("write recording Git shim");
    let mut permissions = fs::metadata(&shim)
        .expect("stat recording Git shim")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&shim, permissions).expect("make recording Git shim executable");
    shim
}

fn resolve_git() -> PathBuf {
    for directory in env::split_paths(&original_path()) {
        let candidate = directory.join("git");
        if fs::metadata(&candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        {
            return fs::canonicalize(&candidate).unwrap_or(candidate);
        }
    }
    panic!("real Git executable not found on PATH");
}

fn original_path() -> OsString {
    env::var_os("PATH").unwrap_or_else(|| OsString::from("/usr/bin:/bin"))
}

fn shim_path(shim: &Path) -> OsString {
    let mut entries = vec![shim.parent().expect("shim has parent").to_path_buf()];
    entries.extend(env::split_paths(&original_path()));
    env::join_paths(entries).expect("construct shim PATH")
}

fn os_args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn os_bytes(values: &[OsString]) -> Vec<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_os_str().as_bytes().to_vec())
        .collect()
}

fn recorded_args(record: &Path) -> Vec<Vec<u8>> {
    let bytes = fs::read(record.join("args")).expect("read argv record");
    if bytes.is_empty() {
        return Vec::new();
    }
    assert_eq!(bytes.last(), Some(&0), "argv record must be NUL terminated");
    bytes[..bytes.len() - 1]
        .split(|byte| *byte == 0)
        .map(<[u8]>::to_vec)
        .collect()
}

fn assert_recorded_context(record: &Path, expected_cwd: &Path) {
    let actual = fs::read(record.join("cwd")).expect("read cwd record");
    let expected = fs::canonicalize(expected_cwd).expect("canonicalize expected cwd");
    let mut expected_bytes = expected.as_os_str().as_bytes().to_vec();
    expected_bytes.push(b'\n');
    assert_eq!(actual, expected_bytes, "Git child cwd changed");
}

fn expected_env_record() -> Vec<u8> {
    let mut expected = Vec::new();
    for value in [
        VISIBLE_ENV.as_bytes(),
        b"".as_slice(),
        b"unset".as_slice(),
        b"C".as_slice(),
    ] {
        expected.extend_from_slice(value);
        expected.push(0);
    }
    expected
}

fn assert_outputs_equal(direct: &Output, wrapped: &Output) {
    assert_eq!(
        wrapped.status.code(),
        direct.status.code(),
        "jjk changed Git's exit status"
    );
    assert_eq!(
        wrapped.stdout, direct.stdout,
        "jjk changed Git's stdout bytes"
    );
    assert_eq!(
        wrapped.stderr, direct.stderr,
        "jjk changed Git's stderr bytes"
    );
}

fn tree_fingerprint(root: &Path) -> Vec<(PathBuf, u8, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, entries: &mut Vec<(PathBuf, u8, Vec<u8>)>) {
        let mut children: Vec<_> = fs::read_dir(current)
            .expect("read fingerprint directory")
            .map(|entry| entry.expect("read fingerprint entry"))
            .collect();
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            let relative = path
                .strip_prefix(root)
                .expect("entry is beneath fingerprint root")
                .to_path_buf();
            let file_type = child.file_type().expect("read fingerprint file type");
            if file_type.is_dir() {
                entries.push((relative, b'd', Vec::new()));
                visit(root, &path, entries);
            } else if file_type.is_symlink() {
                entries.push((
                    relative,
                    b'l',
                    fs::read_link(&path)
                        .expect("read fingerprint symlink")
                        .as_os_str()
                        .as_bytes()
                        .to_vec(),
                ));
            } else {
                entries.push((
                    relative,
                    b'f',
                    fs::read(&path).expect("read fingerprint file"),
                ));
            }
        }
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries
}
