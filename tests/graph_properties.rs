use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use jjk::app::plan::{NavigationMode, NavigationPlanError, plan_navigation};
use jjk::app::query::{
    AnnotationReadModel, AttemptReadModel, ChangeStats as ReadChangeStats, NavigationHistory,
    PatchReadModel, QueryError, QueryService, ReadSnapshotSource, RecoveryReadModel,
    RepositorySnapshot, StateReadModel, WorkspaceReadModel, WorktreeChanges,
};
use jjk::domain::{
    Attempt, AttemptId, GitObjectId, ObjectAlgorithm, State, StateGraph, StateId, StateKind,
};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestCaseError, TestRunner};
use serde_json::Value;
use tempfile::TempDir;
use uuid::Uuid;

const PROPERTY_CASES: u32 = 64;
const PROPERTY_SEED: u64 = 0x4a4a_4b47_5241_5048;

fn property_runner(seed_offset: u64) -> TestRunner {
    TestRunner::new(Config {
        cases: PROPERTY_CASES,
        max_shrink_iters: 16_384,
        failure_persistence: None,
        rng_algorithm: RngAlgorithm::ChaCha,
        rng_seed: RngSeed::Fixed(PROPERTY_SEED.wrapping_add(seed_offset)),
        ..Config::default()
    })
}

fn deterministic_state_id(index: u64) -> StateId {
    let mut bytes = [0_u8; 16];
    bytes[0..8].copy_from_slice(&0x019a_1234_5678_7000_u64.to_be_bytes());
    bytes[8..16].copy_from_slice(&index.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    StateId::from_uuid(Uuid::from_bytes(bytes)).expect("deterministic UUIDv7")
}

fn deterministic_attempt_id(index: u64) -> AttemptId {
    let mut bytes = deterministic_state_id(index).into_bytes();
    bytes[15] ^= 0xa5;
    AttemptId::from_uuid(Uuid::from_bytes(bytes)).expect("deterministic UUIDv7")
}

fn oid(index: usize) -> GitObjectId {
    GitObjectId::new(
        ObjectAlgorithm::Sha1,
        vec![(index as u8).wrapping_add(1); 20],
    )
    .expect("valid SHA-1 object ID")
}

fn state_record(
    id: StateId,
    attempt_id: AttemptId,
    parent: Option<StateId>,
    sequence: u64,
    archived: bool,
) -> StateReadModel {
    StateReadModel {
        id,
        attempt_id,
        logical_parent: parent,
        git_object: oid(sequence as usize),
        kind: "step".into(),
        label: format!("state-{sequence}"),
        message: Some(format!("state {sequence}")),
        created_at_utc: format!("2026-08-28T00:00:{:02}Z", sequence % 60),
        sequence,
        archived,
        annotations: Vec::<AnnotationReadModel>::new(),
        stats: ReadChangeStats::default(),
    }
}

fn snapshot(
    states: Vec<StateReadModel>,
    attempts: Vec<AttemptReadModel>,
    current_state: Option<StateId>,
) -> RepositorySnapshot {
    RepositorySnapshot {
        revision: 17,
        repository_label: "property fixture".into(),
        current_state,
        current_attempt: current_state.and_then(|id| {
            states
                .iter()
                .find(|state| state.id == id)
                .map(|state| state.attempt_id)
        }),
        workspace: WorkspaceReadModel {
            id: None,
            branch: None,
            head: None,
            changes: WorktreeChanges::default(),
        },
        recovery: RecoveryReadModel {
            required: false,
            summary: None,
        },
        states,
        attempts,
        navigation: NavigationHistory::default(),
        capabilities: BTreeMap::new(),
        warnings: Vec::new(),
    }
}

#[derive(Clone)]
struct SnapshotSource(RepositorySnapshot);

impl ReadSnapshotSource for SnapshotSource {
    fn read_snapshot(&self) -> Result<RepositorySnapshot, QueryError> {
        Ok(self.0.clone())
    }

    fn read_diff(
        &self,
        _from: Option<StateId>,
        _to: StateId,
        _scope: jjk::app::query::DiffScope,
        _expected_revision: u64,
    ) -> Result<PatchReadModel, QueryError> {
        unreachable!("graph properties do not request patches")
    }
}

fn parent_indices(max_states: usize) -> impl Strategy<Value = Vec<usize>> {
    vec(any::<usize>(), 0..=max_states).prop_map(|choices| {
        choices
            .into_iter()
            .enumerate()
            .map(|(index, choice)| if index == 0 { 0 } else { choice % index })
            .collect()
    })
}

#[test]
fn stable_identities_are_unique_and_type_separated() {
    let strategy = 1_usize..96;
    property_runner(1)
        .run(&strategy, |count| {
            let states = (0..count).map(|_| StateId::new_v7()).collect::<Vec<_>>();
            let attempts = (0..count).map(|_| AttemptId::new_v7()).collect::<Vec<_>>();
            let state_text = states
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>();
            let attempt_text = attempts
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>();

            prop_assert_eq!(state_text.len(), count);
            prop_assert_eq!(attempt_text.len(), count);
            prop_assert!(state_text.is_disjoint(&attempt_text));
            for id in states {
                prop_assert_eq!(id.to_string().parse::<StateId>(), Ok(id));
                prop_assert!(id.to_string().parse::<AttemptId>().is_err());
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn generated_logical_parent_graphs_are_acyclic() {
    property_runner(2)
        .run(&parent_indices(80), |parents| {
            if parents.is_empty() {
                return Ok(());
            }
            let ids = (0..parents.len())
                .map(|index| deterministic_state_id(index as u64 + 1))
                .collect::<Vec<_>>();
            let attempt_id = deterministic_attempt_id(1);
            let mut graph = StateGraph::new();
            graph
                .add_attempt(Attempt::new(attempt_id, ids[0], "property attempt").unwrap())
                .unwrap();

            for index in 0..ids.len() {
                let parent = (index > 0).then_some(ids[parents[index]]);
                let state = State::new(
                    ids[index],
                    StateKind::Step,
                    oid(index),
                    parent,
                    attempt_id,
                    format!("state-{index}"),
                )
                .expect("generated state is valid");
                graph
                    .add_state(state)
                    .expect("topological insertion succeeds");
            }
            prop_assert!(graph.validate().is_ok());

            for start in &ids {
                let mut cursor = Some(*start);
                let mut visited = BTreeSet::new();
                while let Some(id) = cursor {
                    prop_assert!(visited.insert(id), "logical-parent cycle from {start}");
                    cursor = graph.logical_parent(id);
                }
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn state_order_and_graph_traversal_are_deterministic() {
    let strategy = parent_indices(48).prop_flat_map(|parents| {
        let len = parents.len();
        (Just(parents), vec(any::<u64>(), len))
    });
    property_runner(3)
        .run(&strategy, |(parents, insertion_keys)| {
            if parents.is_empty() {
                return Ok(());
            }
            let ids = (0..parents.len())
                .map(|index| deterministic_state_id(index as u64 + 100))
                .collect::<Vec<_>>();
            let attempt_id = deterministic_attempt_id(2);
            let states = ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    state_record(
                        *id,
                        attempt_id,
                        (index > 0).then_some(ids[parents[index]]),
                        index as u64 + 1,
                        false,
                    )
                })
                .collect::<Vec<_>>();
            let attempt = AttemptReadModel {
                id: attempt_id,
                label: "attempt".into(),
                tip: *ids.last().unwrap(),
                archived: false,
            };
            let mut shuffled = states
                .clone()
                .into_iter()
                .zip(insertion_keys)
                .collect::<Vec<_>>();
            shuffled.sort_by_key(|(_, key)| *key);
            let shuffled = shuffled
                .into_iter()
                .map(|(state, _)| state)
                .collect::<Vec<_>>();

            let expected = QueryService::new(&SnapshotSource(snapshot(
                states,
                vec![attempt.clone()],
                Some(ids[0]),
            )))
            .graph(false)
            .unwrap();
            let actual = QueryService::new(&SnapshotSource(snapshot(
                shuffled,
                vec![attempt],
                Some(ids[0]),
            )))
            .graph(false)
            .unwrap();
            prop_assert_eq!(&actual, &expected);
            prop_assert_eq!(
                serde_json::to_vec(&actual).unwrap(),
                serde_json::to_vec(&expected).unwrap()
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn archive_visibility_never_rewrites_surviving_topology() {
    let strategy = parent_indices(48).prop_flat_map(|parents| {
        let len = parents.len();
        (Just(parents), vec(any::<bool>(), len))
    });
    property_runner(4)
        .run(&strategy, |(parents, mut archived)| {
            if parents.is_empty() {
                return Ok(());
            }
            archived[0] = false;
            let ids = (0..parents.len())
                .map(|index| deterministic_state_id(index as u64 + 200))
                .collect::<Vec<_>>();
            let attempt_id = deterministic_attempt_id(3);
            let states = ids
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    state_record(
                        *id,
                        attempt_id,
                        (index > 0).then_some(ids[parents[index]]),
                        index as u64 + 1,
                        archived[index],
                    )
                })
                .collect::<Vec<_>>();
            let attempt = AttemptReadModel {
                id: attempt_id,
                label: "attempt".into(),
                tip: *ids.last().unwrap(),
                archived: false,
            };
            let source = SnapshotSource(snapshot(states.clone(), vec![attempt], Some(ids[0])));
            let all = QueryService::new(&source).graph(true).unwrap();
            let visible = QueryService::new(&source).graph(false).unwrap();
            let visible_ids = visible
                .nodes
                .iter()
                .map(|node| node.state.id)
                .collect::<BTreeSet<_>>();
            let all_parents = all
                .nodes
                .iter()
                .map(|node| (node.state.id, node.state.logical_parent))
                .collect::<BTreeMap<_, _>>();

            prop_assert_eq!(all.nodes.len(), states.len());
            prop_assert_eq!(
                visible.omitted.archived_states,
                archived.iter().filter(|value| **value).count()
            );
            for node in &visible.nodes {
                prop_assert_eq!(node.state.logical_parent, all_parents[&node.state.id]);
            }
            for edge in &visible.edges {
                prop_assert!(visible_ids.contains(&edge.parent));
                prop_assert!(visible_ids.contains(&edge.child));
                prop_assert_eq!(all_parents[&edge.child], Some(edge.parent));
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn down_refuses_ambiguous_siblings_and_preserves_every_candidate() {
    let strategy = 2_usize..24;
    property_runner(5)
        .run(&strategy, |child_count| {
            let root = deterministic_state_id(300);
            let attempt_id = deterministic_attempt_id(4);
            let mut states = vec![state_record(root, attempt_id, None, 1, false)];
            let mut expected = (0..child_count)
                .map(|index| deterministic_state_id(index as u64 + 301))
                .collect::<Vec<_>>();
            for (index, id) in expected.iter().copied().enumerate() {
                states.push(state_record(
                    id,
                    attempt_id,
                    Some(root),
                    index as u64 + 2,
                    false,
                ));
            }
            expected.sort();
            let before = states.clone();
            let model = snapshot(
                states,
                vec![AttemptReadModel {
                    id: attempt_id,
                    label: "attempt".into(),
                    tip: *expected.last().unwrap(),
                    archived: false,
                }],
                Some(root),
            );

            let result = plan_navigation(&model, NavigationMode::Down, None);
            prop_assert_eq!(
                result,
                Err(NavigationPlanError::AmbiguousChildren(expected))
            );
            prop_assert_eq!(model.states, before);
            Ok(())
        })
        .unwrap();
}

fn run(cwd: &Path, program: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run command")
}

fn successful(cwd: &Path, program: &Path, args: &[&str]) -> Output {
    let output = run(cwd, program, args);
    assert!(
        output.status.success(),
        "{} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

fn init_repository(root: &Path) {
    successful(root, Path::new("git"), &["init", "-q", "-b", "main"]);
    fs::write(root.join("story.txt"), "base\n").expect("write fixture");
    successful(root, Path::new("git"), &["add", "story.txt"]);
    successful(
        root,
        Path::new("git"),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "base",
        ],
    );
}

fn capture(root: &Path, jjk: &Path, index: usize, kind: &str) -> Value {
    fs::write(root.join("story.txt"), format!("state {index}\n")).expect("write state");
    successful(root, Path::new("git"), &["add", "story.txt"]);
    json(&successful(
        root,
        jjk,
        &[kind, "--json", "--", &format!("state {index}")],
    ))
}

fn normalized_graph(
    root: &Path,
    jjk: &Path,
) -> Vec<(String, String, String, Option<String>, u64, bool)> {
    let graph = json(&successful(root, jjk, &["see", "--json"]));
    graph["states"]
        .as_array()
        .expect("states")
        .iter()
        .map(|state| {
            (
                state["state_id"].as_str().unwrap().to_owned(),
                state["attempt_id"].as_str().unwrap().to_owned(),
                state["kind"].as_str().unwrap().to_owned(),
                state["logical_parent"].as_str().map(ToOwned::to_owned),
                state["created_seq"].as_u64().unwrap(),
                state["archived"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[test]
fn compiled_cli_command_sequences_reopen_to_the_same_projection() {
    let strategy = vec(prop_oneof![Just("save"), Just("step"), Just("nice")], 1..7);
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    property_runner(6)
        .run(&strategy, |kinds| {
            let directory =
                TempDir::new().map_err(|error| TestCaseError::fail(error.to_string()))?;
            let root: PathBuf = directory.path().to_path_buf();
            init_repository(&root);
            let setup = json(&successful(&root, &jjk, &["setup", "--json"]));
            let repository_id = setup["repository_id"].clone();
            let imported = normalized_graph(&root, &jjk);
            let imported_ids = imported
                .iter()
                .map(|state| state.0.clone())
                .collect::<BTreeSet<_>>();
            let mut captured = Vec::new();
            for (index, kind) in kinds.iter().enumerate() {
                captured.push(capture(&root, &jjk, index + 1, kind));
            }

            let before = normalized_graph(&root, &jjk);
            let before_current = json(&successful(&root, &jjk, &["current", "--json"]));
            let reopened_setup = json(&successful(&root, &jjk, &["setup", "--json"]));
            let after = normalized_graph(&root, &jjk);
            let after_current = json(&successful(&root, &jjk, &["current", "--json"]));

            prop_assert_eq!(reopened_setup["created"].as_bool(), Some(false));
            prop_assert_eq!(&reopened_setup["repository_id"], &repository_id);
            prop_assert_eq!(&after, &before);
            prop_assert_eq!(&after_current, &before_current);
            let ids = after
                .iter()
                .map(|state| state.0.as_str())
                .collect::<BTreeSet<_>>();
            prop_assert_eq!(ids.len(), imported_ids.len() + captured.len());
            let captured_states = after
                .iter()
                .filter(|state| !imported_ids.contains(&state.0))
                .collect::<Vec<_>>();
            for window in captured_states.windows(2) {
                prop_assert_eq!(window[0].3.as_deref(), Some(window[1].0.as_str()));
                prop_assert!(window[0].4 > window[1].4);
            }
            Ok(())
        })
        .unwrap();
}

#[test]
fn compiled_cli_archive_and_ambiguous_down_preserve_sibling_futures() {
    let jjk = assert_cmd::cargo::cargo_bin!("jjk");
    let directory = TempDir::new().expect("tempdir");
    let root = directory.path();
    init_repository(root);
    successful(root, &jjk, &["setup", "--json"]);
    let imported = normalized_graph(root, &jjk);
    let imported_ids = imported
        .iter()
        .map(|state| state.0.clone())
        .collect::<BTreeSet<_>>();
    let parent = capture(root, &jjk, 1, "step");
    let first = capture(root, &jjk, 2, "step");
    successful(
        root,
        &jjk,
        &["return", parent["state_id"].as_str().unwrap(), "--json"],
    );
    let second = capture(root, &jjk, 3, "nice");
    successful(
        root,
        &jjk,
        &["return", parent["state_id"].as_str().unwrap(), "--json"],
    );

    let before = normalized_graph(root, &jjk);
    let refused = run(root, &jjk, &["down", "--json"]);
    assert_eq!(refused.status.code(), Some(4));
    assert_eq!(normalized_graph(root, &jjk), before);
    let current = json(&successful(root, &jjk, &["current", "--json"]));
    assert_eq!(current["state_id"], parent["state_id"]);

    successful(
        root,
        &jjk,
        &["archive", first["state_id"].as_str().unwrap(), "--json"],
    );
    let hidden = normalized_graph(root, &jjk);
    assert_eq!(hidden.len(), imported_ids.len() + 2);
    assert!(
        hidden
            .iter()
            .any(|state| state.0 == second["state_id"].as_str().unwrap())
    );
    successful(
        root,
        &jjk,
        &["recover", first["state_id"].as_str().unwrap(), "--json"],
    );
    assert_eq!(normalized_graph(root, &jjk), before);
}
