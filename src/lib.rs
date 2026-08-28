//! JJK reusable library.
//! Domain modules are public so application layers share one semantic model.

#![allow(dead_code)]

pub mod app;
pub mod cli;
pub mod domain;
pub mod error;
pub mod render;

mod adapters;
mod ports;

pub use error::{DomainError, JjkError};
