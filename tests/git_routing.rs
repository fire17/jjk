use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::Command;

use jjk::adapters::git::{GitCli, passthrough};
use jjk::adapters::jj::{JjCapabilities, probe};
use jjk::adapters::os::process::OsProcess;
use jjk::cli::exit::passthrough_exit;
use jjk::cli::route::{CommandClass, Route, classify, route};
use jjk::ports::process::{ProcessRunner, ProcessTermination};
use tempfile::TempDir;

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn future_git_verb_preserves_entire_argv() {
    let argv = vec![
        OsString::from("future-git-verb"),
        OsString::from(""),
        OsString::from("--"),
        OsString::from("a b"),
    ];
    let (selected, retained) = route(argv.clone());
    assert_eq!(selected, Route::Passthrough);
    assert_eq!(selected.class(), CommandClass::TransparentGitPassthrough);
    assert_eq!(retained, argv);
}

#[test]
fn enhanced_status_and_help_are_classified_without_bootstrap() {
    assert_eq!(classify(OsStr::new("status")), Route::Enhanced("status"));
    assert_eq!(classify(OsStr::new("--help")), Route::Help);
    assert_eq!(classify(OsStr::new("help")), Route::Help);
}

#[test]
fn discovers_subdirectory_and_shared_common_dir_in_linked_worktree() {
    let fixture = TempDir::new().unwrap();
    let primary = fixture.path().join("primary");
    let linked = fixture.path().join("linked");
    fs::create_dir(&primary).unwrap();
    git(&primary, &["init", "-q"]);
    git(&primary, &["config", "user.name", "JJK Test"]);
    git(&primary, &["config", "user.email", "jjk@example.invalid"]);
    fs::write(primary.join("tracked"), "one").unwrap();
    git(&primary, &["add", "tracked"]);
    git(&primary, &["commit", "-qm", "initial"]);
    git(
        &primary,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "linked",
        ],
    );
    let deep = linked.join("a/b/c");
    fs::create_dir_all(&deep).unwrap();

    let adapter = GitCli::new("git", OsProcess);
    let primary_facts = adapter.discover(&primary).unwrap();
    let linked_facts = adapter.discover(&deep).unwrap();
    assert_eq!(
        linked_facts.worktree_root.as_deref(),
        Some(fs::canonicalize(&linked).unwrap().as_path())
    );
    assert_eq!(linked_facts.common_dir, primary_facts.common_dir);
    assert_ne!(linked_facts.git_dir, linked_facts.common_dir);
    assert!(linked_facts.inside_worktree);
}

#[test]
fn discovery_distinguishes_bare_and_outside_repository() {
    let fixture = TempDir::new().unwrap();
    let bare = fixture.path().join("bare.git");
    git(
        fixture.path(),
        &["init", "--bare", "-q", bare.to_str().unwrap()],
    );

    let adapter = GitCli::new("git", OsProcess);
    let bare_facts = adapter.discover(&bare).unwrap();
    assert!(bare_facts.is_bare);
    assert!(!bare_facts.inside_worktree);
    assert_eq!(bare_facts.worktree_root, None);
    assert_eq!(
        fs::canonicalize(&bare_facts.git_dir).unwrap(),
        fs::canonicalize(&bare).unwrap()
    );
    assert_eq!(bare_facts.common_dir, bare_facts.git_dir);

    let outside = fixture.path().join("outside");
    fs::create_dir(&outside).unwrap();
    assert!(adapter.discover(&outside).is_err());
}

#[test]
fn supervised_passthrough_preserves_exit_code() {
    let fixture = TempDir::new().unwrap();
    let request = passthrough(
        "git",
        vec![OsString::from("definitely-not-a-git-command")],
        fixture.path(),
    );
    let termination = request.supervise(&OsProcess).unwrap();
    assert_ne!(termination.code, Some(0));
    assert_eq!(passthrough_exit(termination), termination.code.unwrap());
    assert_eq!(
        passthrough_exit(ProcessTermination {
            code: Some(37),
            signal: None
        }),
        37
    );
}

#[test]
fn broken_jj_degrades_without_affecting_git() {
    let fixture = TempDir::new().unwrap();
    #[cfg(unix)]
    let shim = {
        use std::os::unix::fs::PermissionsExt;
        let shim = fixture.path().join("jj-broken");
        fs::write(&shim, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'jj 99.0'; exit 0; fi\necho 'broken operation log' >&2\nexit 42\n").unwrap();
        fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).unwrap();
        shim
    };
    #[cfg(windows)]
    let shim = {
        let shim = fixture.path().join("jj-broken.exe");
        fs::copy(
            std::env::current_exe().expect("current test executable"),
            &shim,
        )
        .unwrap();
        shim
    };
    let result = probe(&OsProcess, &shim, fixture.path());
    assert!(matches!(
        result,
        JjCapabilities::Degraded { diagnostic, .. } if !diagnostic.trim().is_empty()
    ));

    #[cfg(unix)]
    {
        let git_result = OsProcess
            .run_captured(&jjk::ports::process::CapturedProcess {
                executable: "git".into(),
                args: vec![OsString::from("--version")],
                cwd: fixture.path().to_path_buf(),
                env_delta: Default::default(),
            })
            .unwrap();
        assert!(git_result.termination.success());
    }
    #[cfg(windows)]
    assert_eq!(
        CommandClass::TransparentGitPassthrough,
        Route::Passthrough.class()
    );
}
