//! Native-string-safe Git CLI execution.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::ports::git::GitOutput;
use crate::ports::process::{CapturedProcess, ProcessRunner};

/// Error returned by the Git CLI adapter.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Git could not be started or observed.
    #[error("could not run Git: {0}")]
    Io(#[from] std::io::Error),
    /// Git rejected a bounded plumbing query.
    #[error("Git command failed with exit {exit_code}: {diagnostic}")]
    Command { exit_code: i32, diagnostic: String },
    /// Git returned output that violates the plumbing contract.
    #[error("Git returned invalid {field}: {detail}")]
    InvalidOutput { field: &'static str, detail: String },
}

/// Git executable and process implementation.
#[derive(Clone, Debug)]
pub struct GitCli<R> {
    executable: PathBuf,
    runner: R,
}

impl<R> GitCli<R> {
    /// Construct a Git adapter. `executable` may be an absolute path or a PATH lookup name.
    pub fn new(executable: impl Into<PathBuf>, runner: R) -> Self {
        Self {
            executable: executable.into(),
            runner,
        }
    }

    /// Selected real Git executable.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}

impl<R: ProcessRunner> GitCli<R> {
    /// Run a bounded plumbing query and retain exact output bytes.
    pub fn run<I, S>(&self, cwd: &Path, args: I) -> Result<GitOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.run_with_env(cwd, args, BTreeMap::new())
    }

    /// Run a bounded plumbing query with an explicit environment delta.
    pub(crate) fn run_with_env<I, S>(
        &self,
        cwd: &Path,
        args: I,
        env_delta: BTreeMap<OsString, Option<OsString>>,
    ) -> Result<GitOutput, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let request = CapturedProcess {
            executable: self.executable.clone(),
            args: args
                .into_iter()
                .map(|arg| arg.as_ref().to_os_string())
                .collect(),
            cwd: cwd.to_path_buf(),
            env_delta,
        };
        let output = self.runner.run_captured(&request)?;
        Ok(GitOutput {
            exit_code: output
                .termination
                .code
                .unwrap_or(128 + output.termination.signal.unwrap_or(0)),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    /// Run a required plumbing query, returning stdout on success.
    pub(crate) fn required<I, S>(&self, cwd: &Path, args: I) -> Result<Vec<u8>, GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let output = self.run(cwd, args)?;
        if output.exit_code == 0 {
            Ok(output.stdout)
        } else {
            Err(GitError::Command {
                exit_code: output.exit_code,
                diagnostic: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        }
    }
}

pub(crate) fn trim_line(mut bytes: Vec<u8>) -> Vec<u8> {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    bytes
}

pub(crate) fn native(bytes: Vec<u8>, field: &'static str) -> Result<OsString, GitError> {
    #[cfg(unix)]
    let _ = field;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes)
            .map(OsString::from)
            .map_err(|error| GitError::InvalidOutput {
                field,
                detail: error.to_string(),
            })
    }
}
