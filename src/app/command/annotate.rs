use serde::{Deserialize, Serialize};

use crate::domain::StateId;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnnotationIntent {
    SetStar { state: StateId, enabled: bool },
    SetTag { state: StateId, tag: String, enabled: bool },
    SetMessage { state: StateId, message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnnotationError { EmptyTag, EmptyMessage, TooLong { field: &'static str, max_bytes: usize } }

pub fn star(state: StateId, enabled: bool) -> AnnotationIntent { AnnotationIntent::SetStar { state, enabled } }
pub fn tag(state: StateId, tag: &str, enabled: bool) -> Result<AnnotationIntent, AnnotationError> {
    let tag = tag.trim();
    if tag.is_empty() { return Err(AnnotationError::EmptyTag); }
    if tag.len() > 128 { return Err(AnnotationError::TooLong { field: "tag", max_bytes: 128 }); }
    Ok(AnnotationIntent::SetTag { state, tag: tag.to_owned(), enabled })
}
pub fn message(state: StateId, message: &str) -> Result<AnnotationIntent, AnnotationError> {
    let message = message.trim();
    if message.is_empty() { return Err(AnnotationError::EmptyMessage); }
    if message.len() > 16 * 1024 { return Err(AnnotationError::TooLong { field: "message", max_bytes: 16 * 1024 }); }
    Ok(AnnotationIntent::SetMessage { state, message: message.to_owned() })
}
