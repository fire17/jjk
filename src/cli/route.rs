//! Versioned first-token command classification.
//!
//! Only exact known UTF-8 names are claimed. Every other token, including future
//! Git verbs and non-Unicode tokens, routes transparently to Git.

use std::ffi::{OsStr, OsString};

/// Registry schema version. A behavior-changing claim requires a version change.
pub const REGISTRY_VERSION: u16 = 1;

/// User-visible command category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandClass {
    /// Deliberately non-Git semantic vocabulary.
    JjkNative,
    /// A Git-named command for which JJK deliberately adds state-aware value.
    GitEnhanced,
    /// Delegation to Git without argument interpretation.
    TransparentGitPassthrough,
}

/// Route selected from the first native argument.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// Dispatch to the JJK application layer.
    Native(&'static str),
    /// Dispatch to a state-aware Git command implementation.
    Enhanced(&'static str),
    /// Delegate every argument to real Git.
    Passthrough,
    /// Render top-level help without bootstrapping repository adapters.
    Help,
    /// Render the JJK version without bootstrapping repository adapters.
    Version,
}

impl Route {
    /// User-visible command class.
    #[must_use]
    pub const fn class(self) -> CommandClass {
        match self {
            Self::Native(_) | Self::Help | Self::Version => CommandClass::JjkNative,
            Self::Enhanced(_) => CommandClass::GitEnhanced,
            Self::Passthrough => CommandClass::TransparentGitPassthrough,
        }
    }
}

/// A registry entry used by help and generated assets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandClaim {
    /// Exact command token.
    pub name: &'static str,
    /// Version in which JJK claimed it.
    pub since: u16,
    /// Routing category.
    pub class: CommandClass,
}

const NATIVE: &[&str] = &[
    "init", "save", "step", "nice", "see", "current", "show", "return", "back",
    "forward", "up", "down", "pick", "fork", "story", "star", "tags", "message",
    "archive", "recover", "undo", "redo", "backup", "load", "freeze", "doctor",
    "remove", "timeshift", "handoff", "validate",
];

const ENHANCED: &[&str] = &["status", "diff", "log", "push", "pull"];

/// Classify one first token. Empty invocation is JJK help.
#[must_use]
pub fn classify(first: &OsStr) -> Route {
    let Some(token) = first.to_str() else { return Route::Passthrough };
    match token {
        "" | "help" | "-h" | "--help" => Route::Help,
        "version" | "-V" | "--version" => Route::Version,
        _ if NATIVE.contains(&token) => Route::Native(NATIVE.iter().copied().find(|name| *name == token).expect("present")),
        _ if ENHANCED.contains(&token) => Route::Enhanced(ENHANCED.iter().copied().find(|name| *name == token).expect("present")),
        _ => Route::Passthrough,
    }
}

/// Classify full argv while preserving every native string for dispatch/delegation.
#[must_use]
pub fn route(argv: Vec<OsString>) -> (Route, Vec<OsString>) {
    let selected = argv.first().map_or(Route::Help, |first| classify(first));
    (selected, argv)
}

/// Complete versioned claim registry, in stable help order.
#[must_use]
pub fn claimed_commands() -> Vec<CommandClaim> {
    NATIVE.iter().map(|name| CommandClaim { name, since: REGISTRY_VERSION, class: CommandClass::JjkNative })
        .chain(ENHANCED.iter().map(|name| CommandClaim { name, since: REGISTRY_VERSION, class: CommandClass::GitEnhanced }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_git_verb_and_non_unicode_are_passthrough() {
        assert_eq!(classify(OsStr::new("future-git-verb")), Route::Passthrough);
        #[cfg(unix)] {
            use std::os::unix::ffi::OsStrExt;
            assert_eq!(classify(OsStr::from_bytes(b"verb-\xff")), Route::Passthrough);
        }
    }

    #[test]
    fn enhanced_status_and_help_are_claimed() {
        assert_eq!(classify(OsStr::new("status")), Route::Enhanced("status"));
        assert_eq!(classify(OsStr::new("--help")), Route::Help);
        assert_eq!(Route::Help.class(), CommandClass::JjkNative);
    }

    #[test]
    fn routing_does_not_rewrite_passthrough_argv() {
        let argv = vec![OsString::from("future-git-verb"), OsString::from(""), OsString::from("--"), OsString::from("a b")];
        let (selected, retained) = route(argv.clone());
        assert_eq!(selected, Route::Passthrough);
        assert_eq!(retained, argv);
    }
}
