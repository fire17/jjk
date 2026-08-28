//! Pure, native-string-safe CLI ownership routing.

use std::ffi::{OsStr, OsString};
use std::ops::Range;

use crate::cli::output::parse_status_output;

pub const REGISTRY_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandClass {
    JjkNative,
    GitEnhanced,
    TransparentGitPassthrough,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Native(&'static str),
    Enhanced(&'static str),
    Passthrough,
    Help,
    Version,
}

impl Route {
    #[must_use]
    pub const fn class(self) -> CommandClass {
        match self {
            Self::Native(_) | Self::Help | Self::Version => CommandClass::JjkNative,
            Self::Enhanced(_) => CommandClass::GitEnhanced,
            Self::Passthrough => CommandClass::TransparentGitPassthrough,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandClaim {
    pub name: &'static str,
    pub since: u16,
    pub class: CommandClass,
}

const NATIVE: &[&str] = &[
    "setup",
    "save",
    "step",
    "nice",
    "see",
    "return",
    "pick",
    "fork",
    "freeze",
    "current",
    "story",
    "back",
    "forward",
    "up",
    "down",
    "archive",
    "recover",
    "undo",
    "redo",
    "backup",
    "load",
    "handoff",
    "validate",
    "doctor",
    "completion",
];
const ENHANCED: &[&str] = &["status"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PassthroughReason {
    ExplicitEscape,
    UnownedVerb,
    NonUtf8Verb,
    FutureOrUnknownGlobal,
    MalformedGitGlobal,
    TerminalGitGlobal,
    UnownedStatusForm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitContext {
    pub globals: Range<usize>,
    pub command_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchRequest {
    Native {
        command: &'static str,
        context: GitContext,
        argv: Vec<OsString>,
    },
    EnhancedStatus {
        context: GitContext,
        argv: Vec<OsString>,
    },
    Passthrough {
        argv: Vec<OsString>,
        reason: PassthroughReason,
    },
    Help,
    Version,
}

impl DispatchRequest {
    #[must_use]
    pub const fn route(&self) -> Route {
        match self {
            Self::Native { command, .. } => Route::Native(command),
            Self::EnhancedStatus { .. } => Route::Enhanced("status"),
            Self::Passthrough { .. } => Route::Passthrough,
            Self::Help => Route::Help,
            Self::Version => Route::Version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PrefixScan {
    Command(GitContext),
    NoCommand,
    Terminal,
    FutureGlobal,
    Malformed,
}

/// Classify a standalone verb without Git-global context.
#[must_use]
pub fn classify(first: &OsStr) -> Route {
    let Some(token) = first.to_str() else {
        return Route::Passthrough;
    };
    match token {
        "" | "help" | "-h" | "--help" => Route::Help,
        "version" | "-v" | "--version" => Route::Version,
        _ if NATIVE.contains(&token) => Route::Native(registered(NATIVE, token)),
        _ if ENHANCED.contains(&token) => Route::Enhanced(registered(ENHANCED, token)),
        _ => Route::Passthrough,
    }
}

/// Route original argv once, before repository access. Only explicit `git --` is removed.
#[must_use]
pub fn dispatch(argv: Vec<OsString>) -> DispatchRequest {
    if argv.first().is_some_and(|value| value == OsStr::new("git"))
        && argv.get(1).is_some_and(|value| value == OsStr::new("--"))
    {
        return DispatchRequest::Passthrough {
            argv: argv[2..].to_vec(),
            reason: PassthroughReason::ExplicitEscape,
        };
    }
    if argv.is_empty() {
        return DispatchRequest::Help;
    }
    if argv.len() == 1 {
        match classify(&argv[0]) {
            Route::Help => return DispatchRequest::Help,
            Route::Version => return DispatchRequest::Version,
            _ => {}
        }
    }
    let context = match scan_prefix(&argv) {
        PrefixScan::Command(context) => context,
        PrefixScan::NoCommand | PrefixScan::Malformed => {
            return DispatchRequest::Passthrough {
                argv,
                reason: PassthroughReason::MalformedGitGlobal,
            };
        }
        PrefixScan::Terminal => {
            return DispatchRequest::Passthrough {
                argv,
                reason: PassthroughReason::TerminalGitGlobal,
            };
        }
        PrefixScan::FutureGlobal => {
            return DispatchRequest::Passthrough {
                argv,
                reason: PassthroughReason::FutureOrUnknownGlobal,
            };
        }
    };
    let Some(verb) = argv[context.command_index].to_str() else {
        return DispatchRequest::Passthrough {
            argv,
            reason: PassthroughReason::NonUtf8Verb,
        };
    };
    if NATIVE.contains(&verb) {
        return DispatchRequest::Native {
            command: registered(NATIVE, verb),
            context,
            argv,
        };
    }
    if verb == "status" {
        let tail = &argv[context.command_index + 1..];
        if parse_status_output(tail).is_some() {
            return DispatchRequest::EnhancedStatus { context, argv };
        }
        return DispatchRequest::Passthrough {
            argv,
            reason: PassthroughReason::UnownedStatusForm,
        };
    }
    DispatchRequest::Passthrough {
        argv,
        reason: PassthroughReason::UnownedVerb,
    }
}

/// Compatibility wrapper retaining the original argv for all non-escape routes.
#[must_use]
pub fn route(argv: Vec<OsString>) -> (Route, Vec<OsString>) {
    let request = dispatch(argv);
    match request {
        DispatchRequest::Native { command, argv, .. } => (Route::Native(command), argv),
        DispatchRequest::EnhancedStatus { argv, .. } => (Route::Enhanced("status"), argv),
        DispatchRequest::Passthrough { argv, .. } => (Route::Passthrough, argv),
        DispatchRequest::Help => (Route::Help, Vec::new()),
        DispatchRequest::Version => (Route::Version, Vec::new()),
    }
}

#[must_use]
pub fn claimed_commands() -> Vec<CommandClaim> {
    NATIVE
        .iter()
        .map(|name| CommandClaim {
            name,
            since: REGISTRY_VERSION,
            class: CommandClass::JjkNative,
        })
        .chain(ENHANCED.iter().map(|name| CommandClaim {
            name,
            since: REGISTRY_VERSION,
            class: CommandClass::GitEnhanced,
        }))
        .collect()
}

fn registered(registry: &'static [&'static str], token: &str) -> &'static str {
    registry
        .iter()
        .copied()
        .find(|name| *name == token)
        .expect("guarded by contains")
}

fn scan_prefix(argv: &[OsString]) -> PrefixScan {
    let mut index = 0;
    while index < argv.len() {
        let token = &argv[index];
        let Some(text) = token.to_str() else {
            return PrefixScan::Command(GitContext {
                globals: 0..index,
                command_index: index,
            });
        };
        if !text.starts_with('-') || text == "-" {
            return PrefixScan::Command(GitContext {
                globals: 0..index,
                command_index: index,
            });
        }
        if text == "--" {
            return PrefixScan::FutureGlobal;
        }
        if matches!(
            text,
            "--version" | "-v" | "--help" | "-h" | "--html-path" | "--man-path" | "--info-path"
        ) || text == "--exec-path"
            || text.starts_with("--exec-path=")
        {
            return PrefixScan::Terminal;
        }
        if matches!(
            text,
            "-p" | "--paginate"
                | "-P"
                | "--no-pager"
                | "--no-replace-objects"
                | "--bare"
                | "--literal-pathspecs"
                | "--glob-pathspecs"
                | "--noglob-pathspecs"
                | "--icase-pathspecs"
        ) {
            index += 1;
            continue;
        }
        if text == "-C" || text == "-c" {
            if argv.get(index + 1).is_none() {
                return PrefixScan::Malformed;
            }
            index += 2;
            continue;
        }
        if (text.starts_with("-C") || text.starts_with("-c")) && text.len() > 2 {
            index += 1;
            continue;
        }
        if matches!(
            text,
            "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix" | "--config-env"
        ) {
            if argv.get(index + 1).is_none() {
                return PrefixScan::Malformed;
            }
            index += 2;
            continue;
        }
        if [
            "--git-dir=",
            "--work-tree=",
            "--namespace=",
            "--super-prefix=",
            "--config-env=",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix) && text.len() > prefix.len())
        {
            index += 1;
            continue;
        }
        return PrefixScan::FutureGlobal;
    }
    PrefixScan::NoCommand
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn final_registry_claims_only_stable_commands() {
        for command in NATIVE {
            assert!(matches!(classify(OsStr::new(command)), Route::Native(_)));
        }
        assert_eq!(classify(OsStr::new("status")), Route::Enhanced("status"));
        for command in [
            "init",
            "show",
            "diff",
            "log",
            "push",
            "pull",
            "clone",
            "rebase",
            "worktree",
            "timeshift",
            "star",
            "promote",
        ] {
            assert_eq!(
                classify(OsStr::new(command)),
                Route::Passthrough,
                "{command}"
            );
        }
    }

    #[test]
    fn explicit_git_escape_strips_exactly_two_tokens() {
        assert_eq!(
            dispatch(args(&["git", "--", "status", "--json"])),
            DispatchRequest::Passthrough {
                argv: args(&["status", "--json"]),
                reason: PassthroughReason::ExplicitEscape,
            }
        );
    }

    #[test]
    fn globals_find_owned_command_without_rewriting_argv() {
        let argv = args(&["-C", "../repo", "-cfoo=bar", "save", "--", "checkpoint"]);
        let routed = dispatch(argv.clone());
        assert_eq!(
            routed,
            DispatchRequest::Native {
                command: "save",
                context: GitContext {
                    globals: 0..3,
                    command_index: 3
                },
                argv,
            }
        );
    }

    #[test]
    fn status_unknown_and_machine_options_passthrough() {
        for argv in [
            args(&["status", "--porcelain=v2", "-z"]),
            args(&["status", "--future"]),
            args(&["status", "--format", "yaml"]),
        ] {
            assert!(matches!(
                dispatch(argv),
                DispatchRequest::Passthrough {
                    reason: PassthroughReason::UnownedStatusForm,
                    ..
                }
            ));
        }
        for argv in [
            args(&["status"]),
            args(&["status", "--format", "json"]),
            args(&["-C", "repo", "status", "--json", "--width=80"]),
        ] {
            assert!(matches!(
                dispatch(argv),
                DispatchRequest::EnhancedStatus { .. }
            ));
        }
    }

    #[test]
    fn future_globals_and_non_unicode_pass_through() {
        assert!(matches!(
            dispatch(args(&["--future-global", "status"])),
            DispatchRequest::Passthrough {
                reason: PassthroughReason::FutureOrUnknownGlobal,
                ..
            }
        ));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let raw = vec![OsString::from(OsStr::from_bytes(b"verb-\xff"))];
            assert!(matches!(
                dispatch(raw),
                DispatchRequest::Passthrough {
                    reason: PassthroughReason::NonUtf8Verb,
                    ..
                }
            ));
        }
    }
}
