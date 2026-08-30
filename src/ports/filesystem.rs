//! Filesystem effects with explicit durable replacement semantics.

use std::io;
use std::path::{Path, PathBuf};

/// Minimal filesystem effects required at infrastructure boundaries.
pub trait Filesystem {
    /// Atomically replace `destination` with `bytes` using a sibling temporary file.
    fn atomic_write(&self, destination: &Path, bytes: &[u8]) -> io::Result<()>;

    /// Resolve a path through the operating system.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// Whether path metadata exists without following the final symlink.
    fn symlink_metadata_exists(&self, path: &Path) -> io::Result<bool>;

    /// Resolve `candidate` and prove it remains beneath canonical `root`.
    /// Implementations must reject symlink escapes and the root itself.
    fn canonicalize_beneath(&self, root: &Path, candidate: &Path) -> io::Result<PathBuf>;
}
