//! Native process boundaries.
//!
//! Transparent passthrough uses [`InheritedProcess`] so callers cannot accidentally
//! request captured or rewritten standard streams.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

/// A request whose standard streams and environment are inherited unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InheritedProcess {
    /// Executable selected by the caller.
    pub executable: PathBuf,
    /// Arguments after argv[0], preserved as native strings.
    pub args: Vec<OsString>,
    /// Working directory observed by the child.
    pub cwd: PathBuf,
}

/// A non-interactive request used for bounded substrate observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedProcess {
    /// Executable selected by the caller.
    pub executable: PathBuf,
    /// Arguments after argv[0], preserved as native strings.
    pub args: Vec<OsString>,
    /// Working directory observed by the child.
    pub cwd: PathBuf,
    /// Explicit environment changes. Everything else remains inherited.
    pub env_delta: BTreeMap<OsString, Option<OsString>>,
}

/// Portable termination facts. A process has either an exit code or a signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessTermination {
    /// Normal process exit code, when the child exited normally.
    pub code: Option<i32>,
    /// Terminating Unix signal, when available.
    pub signal: Option<i32>,
}

impl ProcessTermination {
    /// Whether the process exited normally with status zero.
    #[must_use]
    pub const fn success(self) -> bool {
        matches!(self.code, Some(0)) && self.signal.is_none()
    }
}

/// Captured bytes from a bounded, non-interactive command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessOutput {
    /// Termination facts.
    pub termination: ProcessTermination,
    /// Exact stdout bytes.
    pub stdout: Vec<u8>,
    /// Exact stderr bytes.
    pub stderr: Vec<u8>,
}

/// Runs child processes.
pub trait ProcessRunner {
    /// Run a bounded process to completion with piped stdout/stderr.
    fn run_captured(&self, request: &CapturedProcess) -> io::Result<ProcessOutput>;

    /// Run a process with inherited environment and stdio.
    fn run_inherited(&self, request: &InheritedProcess) -> io::Result<ProcessTermination>;
}

/// Replaces the current process for behavior-transparent delegation.
pub trait ProcessReplacer {
    /// Replace the current process. Success does not return.
    fn replace(&self, request: &InheritedProcess) -> io::Error;
}
