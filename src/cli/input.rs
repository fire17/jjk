//! Native process input captured before command classification.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

/// Raw CLI invocation. Arguments exclude the JJK executable itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawInvocation {
    /// Native arguments in exact received order.
    pub argv: Vec<OsString>,
    /// Invocation working directory.
    pub cwd: PathBuf,
}

impl RawInvocation {
    /// Capture the current process invocation without Unicode conversion.
    pub fn current() -> io::Result<Self> {
        Ok(Self {
            argv: std::env::args_os().skip(1).collect(),
            cwd: std::env::current_dir()?,
        })
    }

    /// Construct an invocation for embedding or tests.
    #[must_use]
    pub fn new(argv: Vec<OsString>, cwd: PathBuf) -> Self {
        Self { argv, cwd }
    }

    /// First token examined by the router.
    #[must_use]
    pub fn first(&self) -> Option<&std::ffi::OsStr> {
        self.argv.first().map(OsString::as_os_str)
    }
}
