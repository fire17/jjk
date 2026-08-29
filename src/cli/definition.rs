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
    claimed_commands()
        .into_iter()
        .map(|claim| CommandDescriptor {
            summary: summary(claim.name, claim.class),
            claim,
        })
        .collect()
}

fn summary(name: &str, class: CommandClass) -> &'static str {
    match (name, class) {
        ("setup", _) => "Create or enroll a JJK safe space without changing Git content",
        ("save", _) => "Capture an explicitly described semantic state",
        ("step", _) => "Capture a meaningful working step",
        ("nice", _) => "Capture a known-good waypoint",
        ("star", _) => "Mark an existing state as a memorable anchor",
        ("unstar", _) => "Remove the memorable-anchor mark from a state",
        ("see", _) => "Show the semantic state graph",
        ("return", _) => "Return to an exact prior state without deleting futures",
        ("pick", _) => "Apply one state's exact parent-to-state delta",
        ("fork", _) => "Create an isolated sibling attempt",
        ("freeze", _) => "Create or inspect a portable verified state bundle",
        ("current", _) => "Show the current semantic location",
        ("story", _) => "Show curated semantic milestones",
        ("back", _) => "Move backward through navigation history",
        ("forward", _) => "Move forward through navigation history",
        ("up", _) => "Move to the logical parent state",
        ("down", _) => "Move to an unambiguous logical child",
        ("archive", _) => "Hide a state without erasing topology or reachability",
        ("recover", _) => "Recover archived state or interrupted work",
        ("undo", _) => "Undo the last complete JJK control-plane operation",
        ("redo", _) => "Redo the last undone JJK control-plane operation",
        ("backup", _) => "Create or verify a complete local recovery artifact",
        ("load", _) => "Preview or restore a verified backup",
        ("handoff", _) => "Create or consume a typed work handoff",
        ("validate", _) => "Record validation bound to exact content",
        ("doctor", _) => "Inspect integrity, capabilities, and recovery needs",
        ("completion", _) => "Generate completion from the production registry",
        ("status", CommandClass::GitEnhanced) => {
            "Git status truth plus JJK state and recovery condition"
        }
        _ => "Transparent Git passthrough",
    }
}

/// Render stable top-level help from the same registry that executes commands.
#[must_use]
pub fn top_level_help() -> String {
    let mut text = String::from(
        "JJK — state-first development over Git\n\nUsage: jjk <command> [arguments]\n\nCommands:\n",
    );
    for descriptor in command_descriptors() {
        use std::fmt::Write as _;
        let _ = writeln!(
            text,
            "  {:<12} {}",
            descriptor.claim.name, descriptor.summary
        );
    }
    text.push_str("\n");
    text.push_str(PASSTHROUGH_HELP);
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_is_generated_from_closed_registry() {
        let help = top_level_help();
        for descriptor in command_descriptors() {
            assert!(help.contains(descriptor.claim.name));
        }
        for unclaimed in ["init", "show", "diff", "worktree", "timeshift"] {
            assert!(
                !help
                    .lines()
                    .any(|line| line.trim_start().starts_with(unclaimed)),
                "{unclaimed}"
            );
        }
    }
}

/// Top-level help footer describing the non-enumerable fallback contract.
pub const PASSTHROUGH_HELP: &str =
    "Every command not claimed above is passed to the real Git executable unchanged.";
