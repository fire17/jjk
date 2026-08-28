//! Stable CLI exit mapping.

use crate::ports::process::ProcessTermination;

/// Stable JJK-native exit classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum ExitCode {
    /// Operation completed successfully.
    Success = 0,
    /// User input was invalid or ambiguous.
    Usage = 2,
    /// Repository or capability was unavailable.
    Unavailable = 3,
    /// Safety precondition prevented an operation.
    Conflict = 4,
    /// Recovery or repair is required.
    RecoveryRequired = 5,
    /// An unexpected internal failure occurred.
    Internal = 70,
}

impl ExitCode {
    /// Numeric process code.
    #[must_use]
    pub const fn get(self) -> i32 {
        self as i32
    }
}

/// Shell-compatible failures that occur before a Git process starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchFailure {
    /// Git resolved to JJK itself or could not be executed.
    NotExecutable,
    /// No Git executable could be resolved.
    NotFound,
}

/// Map a Git launch failure without using a JJK-native exit class.
#[must_use]
pub const fn launch_exit(failure: LaunchFailure) -> i32 {
    match failure {
        LaunchFailure::NotExecutable => 126,
        LaunchFailure::NotFound => 127,
    }
}

/// Convert supervised child termination to shell-compatible exit behavior.
/// Direct passthrough uses exec and therefore needs no mapping.
#[must_use]
pub const fn passthrough_exit(termination: ProcessTermination) -> i32 {
    match (termination.code, termination.signal) {
        (Some(code), _) => code,
        (None, Some(signal)) => 128 + signal,
        (None, None) => ExitCode::Internal as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_normal_git_exit_code() {
        assert_eq!(
            passthrough_exit(ProcessTermination {
                code: Some(37),
                signal: None
            }),
            37
        );
    }

    #[test]
    fn exposes_supervised_signal_termination() {
        assert_eq!(
            passthrough_exit(ProcessTermination {
                code: None,
                signal: Some(2)
            }),
            130
        );
    }

    #[test]
    fn launch_failures_are_shell_compatible() {
        assert_eq!(launch_exit(LaunchFailure::NotExecutable), 126);
        assert_eq!(launch_exit(LaunchFailure::NotFound), 127);
    }

    #[test]
    fn native_exit_values_are_stable() {
        assert_eq!(
            [
                ExitCode::Success.get(),
                ExitCode::Usage.get(),
                ExitCode::Unavailable.get(),
                ExitCode::Conflict.get(),
                ExitCode::RecoveryRequired.get(),
                ExitCode::Internal.get(),
            ],
            [0, 2, 3, 4, 5, 70]
        );
    }
}
