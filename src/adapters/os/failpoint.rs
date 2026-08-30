//! Deterministic, process-scoped fault injection for durable operation boundaries.
//!
//! Production behavior is disabled unless `JJK_FAILPOINT` contains one exact name
//! from [`Failpoint::ALL`]. An evaluator snapshots that setting when it is created;
//! it never mutates process-global environment or shares firing state with another
//! evaluator. The armed boundary returns one injected error, then disarms.

use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Per-process environment setting read by [`FailpointEvaluator::from_env`].
pub(crate) const FAILPOINT_ENV: &str = "JJK_FAILPOINT";

/// Named durable-operation boundaries available to the compiled binary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Failpoint {
    /// Immediately before durable operation preparation.
    BeforePrepare,
    /// After durable preparation and before the first external effect.
    AfterPrepareBeforeFirstEffect,
    /// After an external effect has completed and before continuing.
    AfterEachEffect,
    /// Immediately before verification begins.
    BeforeVerify,
    /// After successful verification and before the commit marker.
    AfterVerifyBeforeCommit,
    /// Checked after the commit call returns but before success is reported. An
    /// injected error therefore models an ambiguous committed outcome; recovery
    /// must reopen the store and prove whether the commit became durable.
    CommitAmbiguity,
}

impl Failpoint {
    pub(crate) const FP_0: &'static str = "FP-0-before-prepare";
    pub(crate) const FP_1: &'static str = "FP-1-after-prepare-before-first-effect";
    pub(crate) const FP_2: &'static str = "FP-2-after-each-effect";
    pub(crate) const FP_3: &'static str = "FP-3-before-verify";
    pub(crate) const FP_4: &'static str = "FP-4-after-verify-before-commit";
    pub(crate) const FP_5: &'static str = "FP-5-commit-ambiguity";

    /// Complete registry in durable operation order.
    pub(crate) const ALL: [Self; 6] = [
        Self::BeforePrepare,
        Self::AfterPrepareBeforeFirstEffect,
        Self::AfterEachEffect,
        Self::BeforeVerify,
        Self::AfterVerifyBeforeCommit,
        Self::CommitAmbiguity,
    ];

    /// Stable external name accepted by `JJK_FAILPOINT`.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::BeforePrepare => Self::FP_0,
            Self::AfterPrepareBeforeFirstEffect => Self::FP_1,
            Self::AfterEachEffect => Self::FP_2,
            Self::BeforeVerify => Self::FP_3,
            Self::AfterVerifyBeforeCommit => Self::FP_4,
            Self::CommitAmbiguity => Self::FP_5,
        }
    }
}

impl fmt::Display for Failpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Failpoint {
    type Err = FailpointConfigurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|failpoint| failpoint.name() == value)
            .ok_or_else(|| FailpointConfigurationError::Unknown(value.to_owned()))
    }
}

/// Invalid process-local fault-injection configuration.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub(crate) enum FailpointConfigurationError {
    /// The environment value was not valid Unicode.
    #[error("{FAILPOINT_ENV} must be UTF-8")]
    NonUtf8,
    /// The environment named no registered boundary.
    #[error("unknown JJK failpoint `{0}`")]
    Unknown(String),
}

/// Deterministic error injected at one named operation boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("injected failure at {failpoint}")]
pub(crate) struct InjectedFailure {
    /// Boundary at which execution was deliberately stopped.
    pub(crate) failpoint: Failpoint,
}

/// Explicit, instance-local evaluator; safe for concurrent tests and processes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FailpointEvaluator {
    armed: Option<Failpoint>,
}

impl FailpointEvaluator {
    /// Returns a disabled evaluator.
    pub(crate) const fn off() -> Self {
        Self { armed: None }
    }

    /// Arms one evaluator directly, without touching ambient process state.
    pub(crate) const fn armed(failpoint: Failpoint) -> Self {
        Self {
            armed: Some(failpoint),
        }
    }

    /// Snapshots `JJK_FAILPOINT` for this evaluator. Absence means disabled.
    pub(crate) fn from_env() -> Result<Self, FailpointConfigurationError> {
        Self::from_setting(env::var_os(FAILPOINT_ENV).as_deref())
    }

    /// Parses an explicit process setting. This seam keeps unit tests free of
    /// global environment mutation and lets callers inject already-sanitized input.
    pub(crate) fn from_setting(
        setting: Option<&OsStr>,
    ) -> Result<Self, FailpointConfigurationError> {
        match setting {
            None => Ok(Self::off()),
            Some(value) => value
                .to_str()
                .ok_or(FailpointConfigurationError::NonUtf8)?
                .parse()
                .map(Self::armed),
        }
    }

    /// Injects once at the armed boundary. Nonmatching checks do not consume it.
    pub(crate) fn check(&mut self, boundary: Failpoint) -> Result<(), InjectedFailure> {
        if self.armed == Some(boundary) {
            self.armed = None;
            return Err(InjectedFailure {
                failpoint: boundary,
            });
        }
        Ok(())
    }

    /// Boundary still armed for this evaluator, if any.
    pub(crate) const fn pending(&self) -> Option<Failpoint> {
        self.armed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_names_are_stable_unique_and_parseable() {
        let names = Failpoint::ALL.map(Failpoint::name);
        assert_eq!(
            names,
            [
                "FP-0-before-prepare",
                "FP-1-after-prepare-before-first-effect",
                "FP-2-after-each-effect",
                "FP-3-before-verify",
                "FP-4-after-verify-before-commit",
                "FP-5-commit-ambiguity",
            ]
        );
        let unique = names.into_iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), Failpoint::ALL.len());
        for failpoint in Failpoint::ALL {
            assert_eq!(failpoint.name().parse(), Ok(failpoint));
        }
    }

    #[test]
    fn absent_setting_is_disabled_and_unknown_setting_fails_closed() {
        let mut disabled = FailpointEvaluator::from_setting(None).expect("disabled evaluator");
        for boundary in Failpoint::ALL {
            assert_eq!(disabled.check(boundary), Ok(()));
        }
        assert_eq!(disabled.pending(), None);
        assert_eq!(
            FailpointEvaluator::from_setting(Some(OsStr::new("FP-99-imaginary"))),
            Err(FailpointConfigurationError::Unknown(
                "FP-99-imaginary".into()
            )),
        );
    }

    #[test]
    fn armed_boundary_fires_exactly_once_without_global_state() {
        let mut first = FailpointEvaluator::armed(Failpoint::AfterEachEffect);
        let mut independent = first.clone();
        assert_eq!(first.check(Failpoint::BeforePrepare), Ok(()));
        assert_eq!(first.pending(), Some(Failpoint::AfterEachEffect));
        assert_eq!(
            first.check(Failpoint::AfterEachEffect),
            Err(InjectedFailure {
                failpoint: Failpoint::AfterEachEffect
            }),
        );
        assert_eq!(first.check(Failpoint::AfterEachEffect), Ok(()));
        assert_eq!(independent.pending(), Some(Failpoint::AfterEachEffect));
        assert_eq!(
            independent.check(Failpoint::AfterEachEffect),
            Err(InjectedFailure {
                failpoint: Failpoint::AfterEachEffect
            }),
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_setting_fails_closed() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            FailpointEvaluator::from_setting(Some(OsStr::from_bytes(b"FP-0-\xff"))),
            Err(FailpointConfigurationError::NonUtf8),
        );
    }
}
