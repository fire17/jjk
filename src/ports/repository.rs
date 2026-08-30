//! Repository discovery facts supplied by the Git authority.

use std::ffi::OsString;
use std::path::PathBuf;

/// Git object identifier encoding used by this repository.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectFormat {
    /// Conventional 160-bit Git identifiers.
    Sha1,
    /// Experimental 256-bit Git identifiers.
    Sha256,
    /// A format introduced by a future Git release.
    Other(OsString),
}

/// Authoritative repository locations and form observed through Git plumbing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryDiscovery {
    /// Directory from which discovery was requested.
    pub invocation_dir: PathBuf,
    /// Worktree root, absent for a bare repository.
    pub worktree_root: Option<PathBuf>,
    /// Worktree-specific Git directory.
    pub git_dir: PathBuf,
    /// Git common directory shared by linked worktrees.
    pub common_dir: PathBuf,
    /// Whether Git reports a bare repository.
    pub is_bare: bool,
    /// Whether the invocation is inside a worktree.
    pub inside_worktree: bool,
    /// Repository object identifier format.
    pub object_format: ObjectFormat,
}

/// Discovers repository truth without interpreting `.git` paths itself.
pub trait RepositoryDiscoveryPort {
    /// Adapter-specific error.
    type Error;

    /// Discover the repository containing `cwd` through the substrate authority.
    fn discover(&self, cwd: &std::path::Path) -> Result<RepositoryDiscovery, Self::Error>;
}
