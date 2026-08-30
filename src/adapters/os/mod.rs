//! Operating-system adapters.

pub(crate) mod clock;
pub(crate) mod failpoint;
pub mod filesystem;
pub(crate) mod ids;
pub(crate) mod lock;
pub mod process;
pub(crate) mod safe_path;
