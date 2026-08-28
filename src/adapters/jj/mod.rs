//! Optional Jujutsu integration. Git-only behavior is complete.

mod observe;

pub use observe::probe;
pub use crate::ports::jj::JjCapabilities;
