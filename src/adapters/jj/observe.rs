//! Non-mutating optional-JJ capability observation.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ports::jj::{JjCapabilities, JjPort};
use crate::ports::process::{CapturedProcess, ProcessOutput, ProcessRunner};

/// Runtime-facing lifecycle state of the optional JJ executable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JjCapabilityState {
    /// The executable has not been observed and is not currently resolvable.
    Absent,
    /// The executable works; repository-specific fields describe whether it is usable here.
    Present,
    /// The executable resolves but a required read-only observation failed.
    Degraded,
    /// This adapter observed JJ previously, but the executable is no longer resolvable.
    Removed,
}

/// Complete, JSON-stable optional-JJ report for `doctor` and `status`.
///
/// Every probe is read-only. A report never enables a mutation path; callers must continue
/// to use Git as the complete substrate regardless of the reported state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JjCapabilityReport {
    /// Current executable lifecycle state.
    pub state: JjCapabilityState,
    /// Executable candidate that was probed, including when it was absent.
    pub executable: PathBuf,
    /// Normalized `jj --version` output when that observation succeeded.
    pub version: Option<String>,
    /// True only after JJ's Git root matches the Git-discovered common directory.
    pub colocated: bool,
    /// Always true: every stable operation remains available through the normative Git path.
    pub git_only_complete: bool,
    /// Canonical JJ workspace root, when JJ discovered one.
    pub workspace_root: Option<PathBuf>,
    /// Canonical underlying Git repository root reported by JJ, when discovered.
    pub git_root: Option<PathBuf>,
    /// Whether a NUL-delimited operation identity was observed without snapshotting.
    pub operation_log_readable: bool,
    /// Current JJ operation identity, present exactly when the operation-log probe succeeded.
    pub operation_id: Option<String>,
    /// Display-safe explanation of the capability or degradation.
    pub diagnostic: String,
}

/// Optional JJ executable and process implementation.
#[derive(Clone, Debug)]
pub struct JjCli<R> {
    executable: PathBuf,
    runner: R,
    last_version: Arc<Mutex<Option<String>>>,
}

impl<R> JjCli<R> {
    /// Construct an optional JJ adapter. Executable absence remains a capability result.
    pub fn new(executable: impl Into<PathBuf>, runner: R) -> Self {
        Self {
            executable: executable.into(),
            runner,
            last_version: Arc::new(Mutex::new(None)),
        }
    }
}

impl<R: ProcessRunner> JjCli<R> {
    /// Produce the complete read-only capability report used by runtime diagnostics.
    ///
    /// `expected_git_common_dir` must be the canonical repository common directory observed
    /// through Git. Passing `None` deliberately leaves colocation unverified and skips the
    /// operation-log probe. Reuse the adapter instance to distinguish initial absence from
    /// removal after a previous successful executable observation.
    #[must_use]
    pub fn capability_report(
        &self,
        cwd: &Path,
        expected_git_common_dir: Option<&Path>,
    ) -> JjCapabilityReport {
        capability_report(
            &self.runner,
            &self.executable,
            cwd,
            expected_git_common_dir,
            &self.last_version,
        )
    }
}

impl<R: ProcessRunner> JjPort for JjCli<R> {
    fn probe(&self, cwd: &Path) -> JjCapabilities {
        probe(&self.runner, &self.executable, cwd)
    }
}

/// Probe the legacy optional-JJ port without requiring Git discovery.
///
/// Runtime `doctor` and `status` should use [`JjCli::capability_report`] instead because this
/// compatibility surface cannot prove colocation against Git's authoritative common directory.
#[must_use]
pub fn probe(
    runner: &impl ProcessRunner,
    executable: impl Into<PathBuf>,
    cwd: &Path,
) -> JjCapabilities {
    let executable = executable.into();
    let version = match version(runner, &executable, cwd) {
        Ok(version) => version,
        Err(ObservationFailure::Absent) => return JjCapabilities::Unavailable,
        Err(ObservationFailure::NotRepository) => return JjCapabilities::Unavailable,
        Err(ObservationFailure::Failed(diagnostic)) => {
            return JjCapabilities::Degraded {
                version: None,
                diagnostic,
            };
        }
    };
    let _workspace_root = match repository_path(
        runner,
        &executable,
        cwd,
        ["--ignore-working-copy", "root"],
        "JJ workspace root",
    ) {
        Ok(path) => path,
        Err(ObservationFailure::NotRepository) => return JjCapabilities::Installed { version },
        Err(ObservationFailure::Absent) => return JjCapabilities::Unavailable,
        Err(ObservationFailure::Failed(diagnostic)) => {
            return JjCapabilities::Degraded {
                version: Some(version),
                diagnostic,
            };
        }
    };
    let _git_root = match repository_path(
        runner,
        &executable,
        cwd,
        ["--ignore-working-copy", "git", "root"],
        "JJ Git root",
    ) {
        Ok(path) => path,
        Err(ObservationFailure::NotRepository) => return JjCapabilities::Installed { version },
        Err(ObservationFailure::Absent) => return JjCapabilities::Unavailable,
        Err(ObservationFailure::Failed(diagnostic)) => {
            return JjCapabilities::Degraded {
                version: Some(version),
                diagnostic,
            };
        }
    };
    match operation_identity(runner, &executable, cwd) {
        Ok(_) | Err(ObservationFailure::NotRepository) => JjCapabilities::Installed { version },
        Err(ObservationFailure::Absent) => JjCapabilities::Unavailable,
        Err(ObservationFailure::Failed(diagnostic)) => JjCapabilities::Degraded {
            version: Some(version),
            diagnostic,
        },
    }
}

fn capability_report(
    runner: &impl ProcessRunner,
    executable: &Path,
    cwd: &Path,
    expected_git_common_dir: Option<&Path>,
    last_version: &Mutex<Option<String>>,
) -> JjCapabilityReport {
    let version = match version(runner, executable, cwd) {
        Ok(version) => {
            *last_version
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(version.clone());
            version
        }
        Err(ObservationFailure::Absent) => {
            let version = last_version
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            let state = if version.is_some() {
                JjCapabilityState::Removed
            } else {
                JjCapabilityState::Absent
            };
            let diagnostic = match state {
                JjCapabilityState::Removed => {
                    "JJ was observed earlier but is no longer available; Git-only operation is unaffected"
                }
                _ => "JJ is not installed; Git-only operation is fully available",
            };
            return base_report(state, executable, version, diagnostic);
        }
        Err(ObservationFailure::NotRepository) => unreachable!("version is repository-independent"),
        Err(ObservationFailure::Failed(diagnostic)) => {
            let version = last_version
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            return base_report(
                JjCapabilityState::Degraded,
                executable,
                version,
                format!("JJ version probe failed: {diagnostic}; Git-only operation is unaffected"),
            );
        }
    };

    let workspace_root = match repository_path(
        runner,
        executable,
        cwd,
        ["--ignore-working-copy", "root"],
        "JJ workspace root",
    ) {
        Ok(path) => path,
        Err(ObservationFailure::NotRepository) => {
            return present_not_colocated(
                executable,
                version,
                "JJ is installed, but the current directory is not in a JJ workspace",
            );
        }
        Err(ObservationFailure::Absent) => {
            return base_report(
                JjCapabilityState::Removed,
                executable,
                Some(version),
                "JJ disappeared during its workspace-root probe; Git-only operation is unaffected",
            );
        }
        Err(failure) => return degraded(executable, version, "workspace-root probe", failure),
    };
    let git_root = match repository_path(
        runner,
        executable,
        cwd,
        ["--ignore-working-copy", "git", "root"],
        "JJ Git root",
    ) {
        Ok(path) => path,
        Err(ObservationFailure::NotRepository) => {
            return present_not_colocated(
                executable,
                version,
                "JJ found a workspace without a colocated Git repository",
            );
        }
        Err(ObservationFailure::Absent) => {
            return base_report(
                JjCapabilityState::Removed,
                executable,
                Some(version),
                "JJ disappeared during its Git-root probe; Git-only operation is unaffected",
            );
        }
        Err(failure) => return degraded(executable, version, "Git-root probe", failure),
    };

    let Some(expected_git_common_dir) = expected_git_common_dir else {
        return JjCapabilityReport {
            state: JjCapabilityState::Present, executable: executable.to_path_buf(), version: Some(version), colocated: false,
            git_only_complete: true,
            workspace_root: Some(workspace_root), git_root: Some(git_root), operation_log_readable: false, operation_id: None,
            diagnostic: "Git common directory was not supplied; JJ colocation was not verified and JJ remains disabled".into(),
        };
    };
    let expected_git_common_dir = match canonical_path(
        expected_git_common_dir,
        cwd,
        "Git common directory",
    ) {
        Ok(path) => path,
        Err(diagnostic) => {
            return JjCapabilityReport {
                state: JjCapabilityState::Degraded,
                executable: executable.to_path_buf(),
                version: Some(version),
                colocated: false,
                git_only_complete: true,
                workspace_root: Some(workspace_root),
                git_root: Some(git_root),
                operation_log_readable: false,
                operation_id: None,
                diagnostic: format!(
                    "cannot verify JJ colocation: {diagnostic}; Git-only operation is unaffected"
                ),
            };
        }
    };
    if git_root != expected_git_common_dir {
        return JjCapabilityReport {
            state: JjCapabilityState::Present,
            executable: executable.to_path_buf(),
            version: Some(version),
            colocated: false,
            git_only_complete: true,
            workspace_root: Some(workspace_root),
            git_root: Some(git_root.clone()),
            operation_log_readable: false,
            operation_id: None,
            diagnostic: format!(
                "JJ uses Git repository `{}`, not Git's discovered common directory `{}`; JJ remains disabled",
                git_root.display(),
                expected_git_common_dir.display()
            ),
        };
    }

    match operation_identity(runner, executable, cwd) {
        Ok(operation_id) => JjCapabilityReport {
            state: JjCapabilityState::Present,
            executable: executable.to_path_buf(),
            version: Some(version),
            colocated: true,
            git_only_complete: true,
            workspace_root: Some(workspace_root),
            git_root: Some(git_root),
            operation_log_readable: true,
            operation_id: Some(operation_id),
            diagnostic:
                "JJ is healthy, colocated, and supports safe read-only operation-log observation"
                    .into(),
        },
        Err(ObservationFailure::Absent) => {
            let mut report = base_report(
                JjCapabilityState::Removed,
                executable,
                Some(version),
                "JJ disappeared during its operation-log probe; Git-only operation is unaffected",
            );
            report.workspace_root = Some(workspace_root);
            report.git_root = Some(git_root);
            report.colocated = true;
            report
        }
        Err(failure) => {
            let mut report = degraded(executable, version, "operation-log probe", failure);
            report.workspace_root = Some(workspace_root);
            report.git_root = Some(git_root);
            report.colocated = true;
            report
        }
    }
}

fn base_report(
    state: JjCapabilityState,
    executable: &Path,
    version: Option<String>,
    diagnostic: impl Into<String>,
) -> JjCapabilityReport {
    JjCapabilityReport {
        state,
        executable: executable.to_path_buf(),
        version,
        colocated: false,
        workspace_root: None,
        git_root: None,
        operation_log_readable: false,
        operation_id: None,
        git_only_complete: true,
        diagnostic: diagnostic.into(),
    }
}

fn present_not_colocated(
    executable: &Path,
    version: String,
    diagnostic: impl Into<String>,
) -> JjCapabilityReport {
    base_report(
        JjCapabilityState::Present,
        executable,
        Some(version),
        format!(
            "{}; JJ remains disabled and Git-only operation is unaffected",
            diagnostic.into()
        ),
    )
}
fn degraded(
    executable: &Path,
    version: String,
    step: &str,
    failure: ObservationFailure,
) -> JjCapabilityReport {
    let diagnostic = match failure {
        ObservationFailure::Absent => format!("JJ disappeared during its {step}"),
        ObservationFailure::NotRepository => format!("JJ reported no repository during its {step}"),
        ObservationFailure::Failed(diagnostic) => format!("JJ {step} failed: {diagnostic}"),
    };
    base_report(
        JjCapabilityState::Degraded,
        executable,
        Some(version),
        format!("{diagnostic}; Git-only operation is unaffected"),
    )
}

fn version(
    runner: &impl ProcessRunner,
    executable: &Path,
    cwd: &Path,
) -> Result<String, ObservationFailure> {
    let output = successful(runner, executable, cwd, ["--version"], false)?;
    let value = text(&output);
    if value.is_empty() {
        Err(ObservationFailure::Failed(
            "JJ version probe returned empty output".into(),
        ))
    } else {
        Ok(value)
    }
}

fn repository_path<const N: usize>(
    runner: &impl ProcessRunner,
    executable: &Path,
    cwd: &Path,
    args: [&str; N],
    field: &str,
) -> Result<PathBuf, ObservationFailure> {
    let output = successful(runner, executable, cwd, args, true)?;
    let raw = PathBuf::from(text(&output));
    if raw.as_os_str().is_empty() {
        return Err(ObservationFailure::Failed(format!(
            "{field} probe returned an empty path"
        )));
    }
    canonical_path(&raw, cwd, field).map_err(ObservationFailure::Failed)
}

fn canonical_path(path: &Path, cwd: &Path, field: &str) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    fs::canonicalize(&absolute).map_err(|error| {
        format!(
            "cannot canonicalize {field} `{}`: {error}",
            absolute.display()
        )
    })
}

fn operation_identity(
    runner: &impl ProcessRunner,
    executable: &Path,
    cwd: &Path,
) -> Result<String, ObservationFailure> {
    let output = successful(
        runner,
        executable,
        cwd,
        [
            "--at-operation=@",
            "--ignore-working-copy",
            "op",
            "log",
            "--limit",
            "1",
            "--no-graph",
            "-T",
            "id ++ \"\\0\"",
        ],
        true,
    )?;
    let Some(identity) = output.strip_suffix(&[0]) else {
        return Err(ObservationFailure::Failed(
            "JJ operation probe returned no NUL-terminated identity".into(),
        ));
    };
    if identity.is_empty() || identity.contains(&0) {
        return Err(ObservationFailure::Failed(
            "JJ operation probe returned an invalid identity".into(),
        ));
    }
    String::from_utf8(identity.to_vec())
        .map_err(|_| ObservationFailure::Failed("JJ operation identity was not UTF-8".into()))
}

fn run<const N: usize>(
    runner: &impl ProcessRunner,
    executable: &Path,
    cwd: &Path,
    args: [&str; N],
) -> std::io::Result<ProcessOutput> {
    runner.run_captured(&CapturedProcess {
        executable: executable.to_path_buf(),
        args: args.into_iter().map(OsString::from).collect(),
        cwd: cwd.to_path_buf(),
        env_delta: BTreeMap::new(),
    })
}

enum ObservationFailure {
    Absent,
    NotRepository,
    Failed(String),
}

fn successful<const N: usize>(
    runner: &impl ProcessRunner,
    executable: &Path,
    cwd: &Path,
    args: [&str; N],
    classify_repository: bool,
) -> Result<Vec<u8>, ObservationFailure> {
    match run(runner, executable, cwd, args) {
        Ok(output) if output.termination.success() => Ok(output.stdout),
        Ok(output) if classify_repository && looks_like_not_repo(&output.stderr) => {
            Err(ObservationFailure::NotRepository)
        }
        Ok(output) => Err(ObservationFailure::Failed(diagnostic(
            &output.stderr,
            output.termination.code,
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ObservationFailure::Absent)
        }
        Err(error) => Err(ObservationFailure::Failed(error.to_string())),
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

fn diagnostic(stderr: &[u8], code: Option<i32>) -> String {
    let message = text(stderr);
    if message.is_empty() {
        format!("JJ exited with status {}", code.unwrap_or(-1))
    } else {
        message
    }
}

fn looks_like_not_repo(stderr: &[u8]) -> bool {
    let value = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    value.contains("no jj repo")
        || value.contains("no jj repository")
        || value.contains("not a jj repo")
        || value.contains("not a jj repository")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io;

    use super::*;
    use crate::ports::process::{InheritedProcess, ProcessTermination};

    #[derive(Debug)]
    enum Reply {
        Output(ProcessOutput),
        NotFound,
    }

    #[derive(Debug)]
    struct ScriptedRunner {
        replies: RefCell<VecDeque<Reply>>,
        requests: RefCell<Vec<CapturedProcess>>,
    }

    impl ScriptedRunner {
        fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
            Self {
                replies: RefCell::new(replies.into_iter().collect()),
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    impl ProcessRunner for ScriptedRunner {
        fn run_captured(&self, request: &CapturedProcess) -> io::Result<ProcessOutput> {
            self.requests.borrow_mut().push(request.clone());
            match self
                .replies
                .borrow_mut()
                .pop_front()
                .expect("scripted reply")
            {
                Reply::Output(output) => Ok(output),
                Reply::NotFound => Err(io::Error::from(io::ErrorKind::NotFound)),
            }
        }

        fn run_inherited(&self, _: &InheritedProcess) -> io::Result<ProcessTermination> {
            panic!("JJ capability observation must never inherit stdio or mutate")
        }
    }

    fn success(stdout: impl Into<Vec<u8>>) -> Reply {
        Reply::Output(ProcessOutput {
            termination: ProcessTermination {
                code: Some(0),
                signal: None,
            },
            stdout: stdout.into(),
            stderr: Vec::new(),
        })
    }

    fn failure(stderr: &str) -> Reply {
        Reply::Output(ProcessOutput {
            termination: ProcessTermination {
                code: Some(1),
                signal: None,
            },
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        })
    }

    #[test]
    fn absence_then_presence_then_removal_are_distinct() {
        let root = tempfile::tempdir().unwrap();
        let runner = ScriptedRunner::new([
            Reply::NotFound,
            success("jj 0.33.0\n"),
            failure("Error: There is no jj repo in this directory"),
            Reply::NotFound,
        ]);
        let jj = JjCli::new("jj", runner);

        assert_eq!(
            jj.capability_report(root.path(), None).state,
            JjCapabilityState::Absent
        );
        assert_eq!(
            jj.capability_report(root.path(), None).state,
            JjCapabilityState::Present
        );
        let removed = jj.capability_report(root.path(), None);
        assert_eq!(removed.state, JjCapabilityState::Removed);
        assert_eq!(removed.version.as_deref(), Some("jj 0.33.0"));
    }

    #[test]
    fn broken_executable_is_degraded_without_repository_probes() {
        let root = tempfile::tempdir().unwrap();
        let runner = ScriptedRunner::new([failure("dynamic loader error")]);
        let jj = JjCli::new("jj", runner);

        let report = jj.capability_report(root.path(), None);

        assert_eq!(report.state, JjCapabilityState::Degraded);
        assert!(!report.colocated);
        assert!(!report.operation_log_readable);
        assert!(report.git_only_complete);
        assert!(
            report
                .diagnostic
                .contains("Git-only operation is unaffected")
        );
        assert_eq!(jj.runner.requests.borrow().len(), 1);
    }

    #[test]
    fn healthy_colocation_observes_operation_log_read_only() {
        let root = tempfile::tempdir().unwrap();
        let canonical = fs::canonicalize(root.path()).unwrap();
        let runner = ScriptedRunner::new([
            success("jj 0.33.0\n"),
            success(format!("{}\n", canonical.display())),
            success(format!("{}\n", canonical.display())),
            success(b"operation-1\0".to_vec()),
        ]);
        let jj = JjCli::new("jj", runner);

        let report = jj.capability_report(root.path(), Some(root.path()));

        assert_eq!(report.state, JjCapabilityState::Present);
        assert!(report.colocated);
        assert!(report.operation_log_readable);
        assert!(report.git_only_complete);
        assert_eq!(report.operation_id.as_deref(), Some("operation-1"));
        let requests = jj.runner.requests.borrow();
        let operation = requests.last().unwrap();
        assert!(operation.args.contains(&OsString::from("--at-operation=@")));
        assert!(
            operation
                .args
                .contains(&OsString::from("--ignore-working-copy"))
        );
        assert!(requests.iter().all(|request| request.env_delta.is_empty()));
    }

    #[test]
    fn mismatched_git_root_never_reads_operation_log() {
        let workspace = tempfile::tempdir().unwrap();
        let jj_git = tempfile::tempdir().unwrap();
        let expected_git = tempfile::tempdir().unwrap();
        let runner = ScriptedRunner::new([
            success("jj 0.33.0\n"),
            success(format!("{}\n", workspace.path().display())),
            success(format!("{}\n", jj_git.path().display())),
        ]);
        let jj = JjCli::new("jj", runner);

        let report = jj.capability_report(workspace.path(), Some(expected_git.path()));

        assert_eq!(report.state, JjCapabilityState::Present);
        assert!(!report.colocated);
        assert!(!report.operation_log_readable);
        assert!(report.git_only_complete);
        assert_eq!(jj.runner.requests.borrow().len(), 3);
    }
}
