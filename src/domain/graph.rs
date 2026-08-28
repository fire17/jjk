use super::{
    attempt::Attempt,
    id::{AttemptId, BranchId, CompositionId, PromotionId, StateId, ValidationId, WorkspaceId},
    provenance::Hash256,
    state::State,
};
use crate::error::DomainError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "entity", content = "id", rename_all = "kebab-case")]
pub enum GraphEntity {
    State(StateId),
    Attempt(AttemptId),
    Branch(BranchId),
    Workspace(WorkspaceId),
    Composition(CompositionId),
    Validation(ValidationId),
    Promotion(PromotionId),
}
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    LogicalParent,
    GitParent,
    AttemptContains,
    BranchProjects,
    WorktreeHosts,
    DeltaDerivedFrom,
    DerivedFrom,
    ComposedFrom,
    CompositionUses,
    Validates,
    Promotes,
    Supersedes,
    OwnedBy,
    HandsOffTo,
    RestoresFrom,
}
#[derive(
    Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct GraphEdge {
    pub from: GraphEntity,
    pub to: GraphEntity,
    pub kind: EdgeKind,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StateGraph {
    states: BTreeMap<StateId, State>,
    attempts: BTreeMap<AttemptId, Attempt>,
    edges: BTreeSet<GraphEdge>,
}
impl StateGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_attempt(&mut self, attempt: Attempt) -> Result<(), DomainError> {
        if self.attempts.contains_key(&attempt.id) {
            return Err(DomainError::Duplicate {
                kind: "attempt",
                id: attempt.id.to_string(),
            });
        }
        self.attempts.insert(attempt.id, attempt);
        Ok(())
    }
    pub fn add_state(&mut self, state: State) -> Result<(), DomainError> {
        if self.states.contains_key(&state.id) {
            return Err(DomainError::Duplicate {
                kind: "state",
                id: state.id.to_string(),
            });
        }
        if !self.attempts.contains_key(&state.attempt_id) {
            return Err(DomainError::Missing {
                kind: "attempt",
                id: state.attempt_id.to_string(),
            });
        }
        if let Some(parent) = state.logical_parent() {
            if !self.states.contains_key(&parent) {
                return Err(DomainError::Missing {
                    kind: "logical parent state",
                    id: parent.to_string(),
                });
            }
            self.ensure_no_parent_cycle(state.id, parent)?;
            let parent_rank = self.states[&parent].topology_rank;
            if state.topology_rank <= parent_rank && state.topology_rank != 0 {
                return Err(DomainError::ProjectionInvariant {
                    reason: "child topology rank must exceed parent rank".into(),
                });
            }
        }
        let id = state.id;
        let attempt = state.attempt_id;
        let parent = state.logical_parent();
        self.states.insert(id, state);
        self.edges.insert(GraphEdge {
            from: GraphEntity::Attempt(attempt),
            to: GraphEntity::State(id),
            kind: EdgeKind::AttemptContains,
        });
        if let Some(parent) = parent {
            self.edges.insert(GraphEdge {
                from: GraphEntity::State(parent),
                to: GraphEntity::State(id),
                kind: EdgeKind::LogicalParent,
            });
        }
        Ok(())
    }
    pub fn add_edge(&mut self, edge: GraphEdge) -> Result<(), DomainError> {
        if edge.kind == EdgeKind::LogicalParent {
            return Err(DomainError::IllegalEdge {
                reason: "logical-parent edges are owned by State::logical_parent".into(),
            });
        }
        self.require_entity(edge.from)?;
        self.require_entity(edge.to)?;
        if edge.from == edge.to {
            return Err(DomainError::IllegalEdge {
                reason: "self edge".into(),
            });
        }
        if !self.edges.insert(edge.clone()) {
            return Err(DomainError::Duplicate {
                kind: "graph edge",
                id: format!("{:?}", edge),
            });
        }
        Ok(())
    }
    fn require_entity(&self, entity: GraphEntity) -> Result<(), DomainError> {
        let found = match entity {
            GraphEntity::State(id) => self.states.contains_key(&id),
            GraphEntity::Attempt(id) => self.attempts.contains_key(&id),
            GraphEntity::Branch(_)
            | GraphEntity::Workspace(_)
            | GraphEntity::Composition(_)
            | GraphEntity::Validation(_)
            | GraphEntity::Promotion(_) => true,
        };
        if found {
            Ok(())
        } else {
            Err(DomainError::Missing {
                kind: "graph entity",
                id: format!("{entity:?}"),
            })
        }
    }
    fn ensure_no_parent_cycle(
        &self,
        child: StateId,
        mut parent: StateId,
    ) -> Result<(), DomainError> {
        let mut seen = BTreeSet::new();
        loop {
            if parent == child || !seen.insert(parent) {
                return Err(DomainError::LogicalParentCycle);
            }
            match self.states.get(&parent).and_then(State::logical_parent) {
                Some(next) => parent = next,
                None => return Ok(()),
            }
        }
    }
    #[must_use]
    pub fn state(&self, id: StateId) -> Option<&State> {
        self.states.get(&id)
    }
    #[must_use]
    pub fn attempt(&self, id: AttemptId) -> Option<&Attempt> {
        self.attempts.get(&id)
    }
    pub fn states(&self) -> impl ExactSizeIterator<Item = &State> {
        self.states.values()
    }
    pub fn attempts(&self) -> impl ExactSizeIterator<Item = &Attempt> {
        self.attempts.values()
    }
    pub fn edges(&self) -> impl Iterator<Item = &GraphEdge> {
        self.edges.iter()
    }
    pub fn edges_from(
        &self,
        entity: GraphEntity,
        kind: Option<EdgeKind>,
    ) -> impl Iterator<Item = &GraphEdge> {
        self.edges
            .iter()
            .filter(move |e| e.from == entity && kind.is_none_or(|k| e.kind == k))
    }
    pub fn edges_to(
        &self,
        entity: GraphEntity,
        kind: Option<EdgeKind>,
    ) -> impl Iterator<Item = &GraphEdge> {
        self.edges
            .iter()
            .filter(move |e| e.to == entity && kind.is_none_or(|k| e.kind == k))
    }
    #[must_use]
    pub fn logical_parent(&self, id: StateId) -> Option<StateId> {
        self.states.get(&id).and_then(State::logical_parent)
    }
    pub fn children(&self, id: StateId) -> impl Iterator<Item = &State> {
        self.states
            .values()
            .filter(move |s| s.logical_parent() == Some(id))
    }
    pub fn roots(&self) -> impl Iterator<Item = &State> {
        self.states.values().filter(|s| s.parent.is_complete_root())
    }
    pub fn validate(&self) -> Result<(), DomainError> {
        for state in self.states.values() {
            if !self.attempts.contains_key(&state.attempt_id) {
                return Err(DomainError::Missing {
                    kind: "attempt",
                    id: state.attempt_id.to_string(),
                });
            }
            if let Some(parent) = state.logical_parent() {
                if !self.states.contains_key(&parent) {
                    return Err(DomainError::Missing {
                        kind: "logical parent state",
                        id: parent.to_string(),
                    });
                }
                self.ensure_no_parent_cycle(state.id, parent)?;
            }
        }
        Ok(())
    }
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serializing domain graph cannot fail")
    }
    #[must_use]
    pub fn digest(&self) -> Hash256 {
        Hash256::digest(&self.canonical_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GitObjectId, ObjectAlgorithm, StateKind};
    fn oid(n: u8) -> GitObjectId {
        GitObjectId::new(ObjectAlgorithm::Sha1, vec![n; 20]).unwrap()
    }
    #[test]
    fn graph_relations_are_typed_and_deterministic() {
        let root = StateId::new_v7();
        let attempt = Attempt::new(AttemptId::new_v7(), root, "try").unwrap();
        let aid = attempt.id;
        let mut graph = StateGraph::new();
        graph.add_attempt(attempt).unwrap();
        graph
            .add_state(State::new(root, StateKind::Save, oid(1), None, aid, "root").unwrap())
            .unwrap();
        let child = StateId::new_v7();
        graph
            .add_state(
                State::new(child, StateKind::Step, oid(2), Some(root), aid, "child").unwrap(),
            )
            .unwrap();
        assert_eq!(graph.logical_parent(child), Some(root));
        assert_eq!(
            graph.children(root).map(|s| s.id).collect::<Vec<_>>(),
            vec![child]
        );
        assert_eq!(
            graph
                .edges()
                .filter(|e| e.kind == EdgeKind::LogicalParent)
                .count(),
            1
        );
        assert_eq!(graph.digest(), graph.clone().digest());
    }
    #[test]
    fn logical_parent_cycle_is_rejected() {
        let a = StateId::new_v7();
        let mut graph = StateGraph::new();
        let attempt = Attempt::new(AttemptId::new_v7(), a, "try").unwrap();
        let aid = attempt.id;
        graph.add_attempt(attempt).unwrap();
        graph
            .add_state(State::new(a, StateKind::Save, oid(1), None, aid, "a").unwrap())
            .unwrap();
        let b = StateId::new_v7();
        graph
            .add_state(State::new(b, StateKind::Step, oid(2), Some(a), aid, "b").unwrap())
            .unwrap();
        assert!(matches!(
            graph.ensure_no_parent_cycle(a, b),
            Err(DomainError::LogicalParentCycle)
        ));
    }
}
