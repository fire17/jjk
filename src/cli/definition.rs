//! Single data source for command help and generated CLI assets.

use crate::cli::route::{CommandClaim, CommandClass, claimed_commands};

/// Stable CLI definition version.
pub const DEFINITION_VERSION: u16 = 1;

/// Human-facing command descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandDescriptor {
    /// Versioned routing claim.
    pub claim: CommandClaim,
    /// Concise contract-accurate description.
    pub summary: &'static str,
}

/// Complete stable command descriptors. Passthrough is a fallback rule rather than
/// an enumerable command list, so future Git verbs work without a JJK release.
#[must_use]
pub fn command_descriptors() -> Vec<CommandDescriptor> {
    claimed_commands().into_iter().map(|claim| CommandDescriptor {
        summary: summary(claim.name, claim.class),
        claim,
    }).collect()
}

fn summary(name: &str, class: CommandClass) -> &'static str {
    match (name, class) {
        ("status", CommandClass::GitEnhanced) => "Git status truth plus JJK state and recovery condition",
        ("diff", CommandClass::GitEnhanced) => "Git diff with explicit state-aware context",
        ("log", CommandClass::GitEnhanced) => "Git history with semantic state context",
        ("push", CommandClass::GitEnhanced) => "Push with explicit JJK state-ref behavior",
        ("pull", CommandClass::GitEnhanced) => "Pull with bounded state reconciliation",
        (_, CommandClass::JjkNative) => "JJK semantic state operation",
        _ => "Transparent Git passthrough",
    }
}

/// Top-level help footer describing the non-enumerable fallback contract.
pub const PASSTHROUGH_HELP: &str = "Every command not claimed above is passed to the real Git executable unchanged.";
