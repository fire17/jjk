//! Effect interfaces implemented by infrastructure adapters.

pub(crate) mod clock;
pub mod filesystem;
pub mod git;
pub(crate) mod ids;
pub mod jj;
pub(crate) mod journal;
pub(crate) mod lock;
pub(crate) mod operation;
pub mod process;
pub(crate) mod projection;
pub mod repository;
