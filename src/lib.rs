//! JJK reusable library.
//! Domain modules are public so application layers share one semantic model.

#![allow(dead_code)]

pub mod app;
pub mod cli;
pub mod domain;
pub mod error;
pub mod render;

#[doc(hidden)]
pub mod adapters;
#[doc(hidden)]
pub mod ports;

pub use error::{DomainError, JjkError};
