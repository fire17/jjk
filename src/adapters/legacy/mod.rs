//! One-way, read-only adapters for formats retired by the Rust rewrite.

pub(crate) mod repo_json;
pub(crate) mod sqlite;

#[cfg(test)]
mod tests;
