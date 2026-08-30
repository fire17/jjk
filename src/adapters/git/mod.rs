//! Git CLI adapter. Git remains the substrate authority.

pub mod command;
pub mod discover;
pub mod observe;
pub mod passthrough;

pub use crate::ports::git::GitCapabilities;
pub use command::{GitCli, GitError};
pub use discover::RepositoryDiscovery;
pub use passthrough::{Passthrough, passthrough};
