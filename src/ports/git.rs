//! Minimum Git observation effects used by application services.

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Installation and repository capabilities reported by Git.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitCapabilities {
    /// Raw normalized version text after the `git version` prefix.
    pub version: OsString,
    /// Object format reported for the current repository, when in one.
    pub object_format: Option<crate::ports::repository::ObjectFormat>,
    /// Common Git directory, when in a repository.
    pub common_dir: Option<std::path::PathBuf>,
    /// Whether this checkout is a linked worktree.
    pub linked_worktree: bool,
}

/// Exact byte output from a bounded Git plumbing command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitOutput {
    /// Normal exit code or platform-derived equivalent.
    pub exit_code: i32,
    /// Stdout bytes without decoding.
    pub stdout: Vec<u8>,
    /// Stderr bytes without decoding.
    pub stderr: Vec<u8>,
}

/// Read-only Git authority surface.
pub trait GitPort {
    /// Adapter-specific error.
    type Error;

    /// Observe installation and repository capabilities.
    fn capabilities(&self, cwd: &Path) -> Result<GitCapabilities, Self::Error>;

    /// Execute an explicit bounded plumbing query.
    fn query<I, S>(&self, cwd: &Path, args: I) -> Result<GitOutput, Self::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>;
}
