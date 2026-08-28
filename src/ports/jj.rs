//! Optional Jujutsu capability boundary.

use std::path::Path;

/// Optional-JJ probe result. Git-only operation remains complete for every variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JjCapabilities {
    /// No `jj` executable was found.
    Unavailable,
    /// JJ is installed but this repository is not colocated with JJ.
    Installed { version: String },
    /// A colocated JJ repository passed backend and non-mutating operation probes.
    Available {
        version: String,
        workspace_root: std::path::PathBuf,
        git_root: std::path::PathBuf,
        operation_id: String,
    },
    /// JJ exists but failed its capability probe. The diagnostic is safe to display.
    Degraded {
        version: Option<String>,
        diagnostic: String,
    },
}

/// Read-only optional-JJ capability probe.
pub trait JjPort {
    /// Probe JJ without mutating Git, JJ, the worktree, or JJK metadata.
    fn probe(&self, cwd: &Path) -> JjCapabilities;
}
