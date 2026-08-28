pub mod graph;
pub mod human;
pub mod json;
pub mod style;
pub mod table;

use crate::app::query::QueryOutcome;
use crate::cli::{OutputMode, OutputPolicy};

#[derive(Debug)]
pub enum RenderError { Json(serde_json::Error) }

pub fn outcome(value: &QueryOutcome, policy: OutputPolicy) -> Result<String, RenderError> {
    match policy.mode {
        OutputMode::Human => Ok(human::outcome(value, policy)),
        OutputMode::Json => json::render(value).map_err(RenderError::Json),
        OutputMode::Quiet => Ok(String::new()),
    }
}
