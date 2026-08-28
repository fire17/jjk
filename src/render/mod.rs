pub mod graph;
pub mod human;
pub mod json;
pub mod style;
pub mod table;

use crate::app::query::QueryOutcome;
use crate::cli::{OutputMode, OutputPolicy};
pub use json::{EnvelopeMeta, MachineError};

#[derive(Debug)]
pub enum RenderError {
    Json(serde_json::Error),
}

pub fn outcome(value: &QueryOutcome, policy: OutputPolicy) -> Result<String, RenderError> {
    let meta = EnvelopeMeta {
        projection_version: Some(projection_version(value)),
        ..EnvelopeMeta::default()
    };
    outcome_with_meta(value, policy, meta)
}

pub fn outcome_with_meta(
    value: &QueryOutcome,
    policy: OutputPolicy,
    meta: EnvelopeMeta<'_>,
) -> Result<String, RenderError> {
    match policy.mode {
        OutputMode::Human => Ok(human::outcome(value, policy)),
        OutputMode::Json => json::render_with_meta(value, meta, &[]).map_err(RenderError::Json),
        OutputMode::Quiet => Ok(String::new()),
    }
}

fn projection_version(value: &QueryOutcome) -> u64 {
    match value {
        QueryOutcome::Current(model) => model.revision,
        QueryOutcome::Status(model) => model.revision,
        QueryOutcome::Graph(model) => model.revision,
        QueryOutcome::Story(model) => model.revision,
        QueryOutcome::Show(model) => model.revision,
        QueryOutcome::Diff(model) => model.revision,
    }
}
