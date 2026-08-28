//! Transparent delegation to the real Git executable.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::ports::process::{InheritedProcess, ProcessReplacer, ProcessRunner, ProcessTermination};

/// Transparent Git passthrough request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Passthrough {
    /// Selected real Git executable.
    pub executable: PathBuf,
    /// Original Git arguments, byte/native-string preserving and in original order.
    pub argv: Vec<OsString>,
    /// Original invocation directory.
    pub cwd: PathBuf,
}

impl Passthrough {
    /// Construct a passthrough request without parsing any Git arguments.
    #[must_use]
    pub fn new(
        executable: impl Into<PathBuf>,
        argv: Vec<OsString>,
        cwd: impl Into<PathBuf>,
    ) -> Self {
        Self {
            executable: executable.into(),
            argv,
            cwd: cwd.into(),
        }
    }

    fn process(&self) -> InheritedProcess {
        InheritedProcess {
            executable: self.executable.clone(),
            args: self.argv.clone(),
            cwd: self.cwd.clone(),
        }
    }

    /// Replace the current process on platforms that support native exec.
    ///
    /// The returned value is always an operating-system launch error; successful exec
    /// replaces the caller and therefore never returns.
    pub fn exec(self, replacer: &impl ProcessReplacer) -> std::io::Error {
        replacer.replace(&self.process())
    }

    /// Supervise Git with inherited stdio and return exact termination facts.
    pub fn supervise(&self, runner: &impl ProcessRunner) -> std::io::Result<ProcessTermination> {
        runner.run_inherited(&self.process())
    }
}

/// Build a transparent passthrough request.
#[must_use]
pub fn passthrough(executable: impl Into<PathBuf>, argv: Vec<OsString>, cwd: &Path) -> Passthrough {
    Passthrough::new(executable, argv, cwd)
}
