//! Application services, command orchestration, and pure planning.

pub mod command;
pub mod plan;
pub mod query;
pub(crate) mod reconcile;
pub(crate) mod repair;
pub mod resolve;
pub(crate) mod runtime_mutation;
pub(crate) mod transaction;
