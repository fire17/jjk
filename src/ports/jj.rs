//! Optional Jujutsu capability boundary.

use std::path::Path;

/// Optional-JJ probe result. Git-only operation remains complete for every variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JjCapabilities {
    /// No `jj` executable was found.
    Unavailable,
    /// JJ is installed but this repository is not colocated with JJ.
    Installed { version: String },
    /// A colocated JJ repository responded successfully.
    Available { version: String, root: std::path::PathBuf },
    /// JJ exists but failed its capability probe. The diagnostic is safe to display.
    Degraded { version: Option<String>, diagnostic: String },
}

/// Read-only optional-JJ capability probe.
pub trait JjPort {
    /// Probe JJ without mutating Git, JJ, the worktree, or JJK metadata.
    fn probe(&self, cwd: &Path) -> JjCapabilities;
}
