use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::app::query::StateReadModel;
use crate::domain::StateId;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolutionCandidate {
    pub id: StateId,
    pub label: String,
    pub kind: String,
    pub attempt: String,
    pub created_at_utc: String,
}

impl From<&StateReadModel> for ResolutionCandidate {
    fn from(state: &StateReadModel) -> Self {
        Self { id: state.id, label: state.label.clone(), kind: state.kind.clone(), attempt: state.attempt_id.to_string(), created_at_utc: state.created_at_utc.clone() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Resolution {
    Exact { candidate: ResolutionCandidate },
    UniqueMatch { candidate: ResolutionCandidate },
    Ambiguous { query: String, candidates: Vec<ResolutionCandidate> },
    NotFound { query: String },
}

impl Resolution {
    pub fn require_unique(self) -> Result<ResolutionCandidate, ResolveError> {
        match self {
            Self::Exact { candidate } | Self::UniqueMatch { candidate } => Ok(candidate),
            Self::Ambiguous { query, candidates } => Err(ResolveError::Ambiguous { query, candidates }),
            Self::NotFound { query } => Err(ResolveError::NotFound { query }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolveError {
    EmptyQuery,
    Ambiguous { query: String, candidates: Vec<ResolutionCandidate> },
    NotFound { query: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyQuery => f.write_str("target cannot be empty"),
            Self::Ambiguous { query, candidates } => write!(f, "target {query:?} is ambiguous ({} candidates); use an exact state ID", candidates.len()),
            Self::NotFound { query } => write!(f, "no state matched {query:?}"),
        }
    }
}
impl Error for ResolveError {}

pub fn resolve_state(states: &[StateReadModel], query: &str, include_archived: bool) -> Result<Resolution, ResolveError> {
    let query = query.trim();
    if query.is_empty() { return Err(ResolveError::EmptyQuery); }
    let visible = states.iter().filter(|state| include_archived || !state.archived).collect::<Vec<_>>();

    // Exact IDs dominate human names. Exact names still refuse duplicate labels/messages.
    if let Some(state) = visible.iter().find(|state| state.id.to_string() == query) {
        return Ok(Resolution::Exact { candidate: ResolutionCandidate::from(*state) });
    }
    let exact = visible.iter().filter(|state| {
        state.label == query || state.message.as_deref() == Some(query)
    }).copied().collect::<Vec<_>>();
    if exact.len() == 1 {
        return Ok(Resolution::Exact { candidate: ResolutionCandidate::from(exact[0]) });
    }
    if exact.len() > 1 {
        return Ok(Resolution::Ambiguous { query: query.to_owned(), candidates: canonical_candidates(exact) });
    }

    let folded = query.to_lowercase();
    let matches = visible.iter().filter(|state| {
        state.id.to_string().starts_with(query)
            || state.label.to_lowercase().contains(&folded)
            || state.message.as_deref().is_some_and(|message| message.to_lowercase().contains(&folded))
    }).copied().collect::<Vec<_>>();
    match matches.len() {
        0 => Ok(Resolution::NotFound { query: query.to_owned() }),
        1 => Ok(Resolution::UniqueMatch { candidate: ResolutionCandidate::from(matches[0]) }),
        _ => Ok(Resolution::Ambiguous { query: query.to_owned(), candidates: canonical_candidates(matches) }),
    }
}

fn canonical_candidates(mut states: Vec<&StateReadModel>) -> Vec<ResolutionCandidate> {
    states.sort_by(|left, right| left.sequence.cmp(&right.sequence).then_with(|| left.id.cmp(&right.id)));
    states.into_iter().map(ResolutionCandidate::from).collect()
}
