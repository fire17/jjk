//! Optional Jujutsu integration. Git-only behavior is complete.

mod observe;

pub use crate::ports::jj::JjCapabilities;
pub use observe::{JjCapabilityReport, JjCapabilityState, JjCli, probe};
