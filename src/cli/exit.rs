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
    pub const fn get(self) -> i32 { self as i32 }
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
        assert_eq!(passthrough_exit(ProcessTermination { code: Some(37), signal: None }), 37);
    }

    #[test]
    fn exposes_supervised_signal_termination() {
        assert_eq!(passthrough_exit(ProcessTermination { code: None, signal: Some(2) }), 130);
    }
}
