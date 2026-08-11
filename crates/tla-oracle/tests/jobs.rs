//! One job per verdict the oracle can reach, driven through the wire format
//! rather than the Rust types, because the wire format is the contract.

use std::path::Path;

use tla_oracle::{Job, Status, check};

fn spec() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/BoundedBuffer.tla");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// A job over the bounded buffer: put 1, then get it back.
fn job(states: &str, edges: &str, outcomes: &str) -> Job {
    let source = format!(
        r#"{{
          "spec": {spec},
          "constants": {{
            "Capacity": {{"value": 2, "schema": {{"kind": "int"}}}},
            "MaxItems": {{"value": 3, "schema": {{"kind": "int"}}}}
          }},
          "schema": {{
            "buf":      {{"kind": "seq", "of": {{"kind": "int"}}}},
            "nextVal":  {{"kind": "int"}},
            "received": {{"kind": "seq", "of": {{"kind": "int"}}}}
          }},
          "states": {states},
          "edges": {edges},
          "outcomes": {outcomes}
        }}"#,
        spec = serde_json::to_string(&spec()).expect("string encodes"),
    );
    serde_json::from_str(&source).expect("the job parses")
}

const WALK: &str = r#"[
    {"buf": [],  "nextVal": 1, "received": []},
    {"buf": [1], "nextVal": 2, "received": []},
    {"buf": [],  "nextVal": 2, "received": [1]}
]"#;

#[test]
fn a_legal_walk_passes() {
    let report = check(&job(WALK, r#"[[0,1,"put"],[1,2,"get"]]"#, "{}"));
    assert_eq!(report.status, Status::Pass, "{}", report.detail);
    assert_eq!(report.stats.edges_checked, 2);
}

#[test]
fn an_illegal_root_is_an_init_failure() {
    let states = r#"[{"buf": [7], "nextVal": 1, "received": []}]"#;
    let report = check(&job(states, "[]", "{}"));
    assert_eq!(report.status, Status::Init, "{}", report.detail);
}

/// The buffer holds two items; a third is not a step the specification allows.
/// The graph starts legally and only goes wrong on its last edge, so it is the
/// refinement arm that must catch it and not the initial-state one.
#[test]
fn an_illegal_edge_is_named() {
    let states = r#"[
        {"buf": [],        "nextVal": 1, "received": []},
        {"buf": [1],       "nextVal": 2, "received": []},
        {"buf": [1, 2],    "nextVal": 3, "received": []},
        {"buf": [1, 2, 3], "nextVal": 4, "received": []}
    ]"#;
    let edges = r#"[[0,1,"put"],[1,2,"put"],[2,3,"overfill"]]"#;
    let report = check(&job(states, edges, "{}"));
    assert_eq!(report.status, Status::Refines, "{}", report.detail);
    let edge = report.edge.expect("the offending edge is reported");
    assert_eq!((edge.index, edge.source, edge.target), (2, 2, 3));
    assert_eq!(edge.label, "overfill");
}

/// A specification is `Init /\ [][Next]_vars`, so a step that leaves every
/// variable unchanged is permitted by every specification. `BoundedBuffer`'s
/// `Next` has no such action — an implementation that idles, retries or
/// re-renders must still refine it.
#[test]
fn a_stuttering_step_is_permitted() {
    let edges = r#"[[0,0,"idle"],[0,1,"put"],[1,1,"retry"],[1,2,"get"]]"#;
    let report = check(&job(WALK, edges, "{}"));
    assert_eq!(report.status, Status::Pass, "{}", report.detail);
    assert_eq!(report.stats.stutter_steps, 2);
    assert_eq!(report.stats.edges_checked, 4);
}

/// Stuttering is unchanged *variables*, not an unchanged label: two distinct
/// states are still asked about, however the edge is named.
#[test]
fn a_relabelled_illegal_step_is_still_caught() {
    let states = r#"[
        {"buf": [],        "nextVal": 1, "received": []},
        {"buf": [1, 2, 3], "nextVal": 4, "received": []}
    ]"#;
    let report = check(&job(states, r#"[[0,1,"idle"]]"#, "{}"));
    assert_eq!(report.status, Status::Refines, "{}", report.detail);
    assert_eq!(report.stats.stutter_steps, 0);
}

/// Every step above is legal, but the buffer never actually fills.
#[test]
fn an_unreachable_outcome_is_a_coverage_failure() {
    let outcomes = r#"{"buffer_fills": "\\E s \\in Reached : Len(s.buf) = Capacity"}"#;
    let report = check(&job(WALK, r#"[[0,1,"put"],[1,2,"get"]]"#, outcomes));
    assert_eq!(report.status, Status::Coverage, "{}", report.detail);
    assert_eq!(report.outcome.as_deref(), Some("buffer_fills"));
}

#[test]
fn a_reachable_outcome_passes() {
    let outcomes = r#"{"something_arrives": "\\E s \\in Reached : Len(s.received) = 1"}"#;
    let report = check(&job(WALK, r#"[[0,1,"put"],[1,2,"get"]]"#, outcomes));
    assert_eq!(report.status, Status::Pass, "{}", report.detail);
    assert_eq!(report.stats.outcomes_checked, 1);
}

/// A set written where the schema calls for a sequence is the candidate's
/// fault, but it is not a protocol mistake and is reported as its own arm.
#[test]
fn a_state_of_the_wrong_shape_is_a_contract_violation() {
    let states = r#"[{"buf": 1, "nextVal": 1, "received": []}]"#;
    let report = check(&job(states, "[]", "{}"));
    assert_eq!(report.status, Status::Contract, "{}", report.detail);
    assert!(report.detail.contains("state[0].buf"), "{}", report.detail);
}

#[test]
fn a_missing_variable_is_a_contract_violation() {
    let states = r#"[{"buf": [], "nextVal": 1}]"#;
    let report = check(&job(states, "[]", "{}"));
    assert_eq!(report.status, Status::Contract, "{}", report.detail);
    assert!(report.detail.contains("received"), "{}", report.detail);
}

#[test]
fn duplicate_members_of_a_set_are_rejected() {
    let source = r#"{
        "spec": "---- MODULE M ----\nVARIABLE s\nInit == s = {1}\n====",
        "schema": {"s": {"kind": "set", "of": {"kind": "int"}}},
        "states": [{"s": [1, 1]}]
    }"#;
    let job: Job = serde_json::from_str(source).expect("the job parses");
    let report = check(&job);
    assert_eq!(report.status, Status::Contract, "{}", report.detail);
    assert!(report.detail.contains("duplicate"), "{}", report.detail);
}

/// A job the oracle cannot carry out is not the candidate's failure, and is
/// kept distinct so a broken task is never scored as a broken implementation.
#[test]
fn a_broken_specification_is_an_error_not_a_verdict() {
    let source = r#"{
        "spec": "not a module at all",
        "schema": {"s": {"kind": "int"}},
        "states": [{"s": 1}]
    }"#;
    let job: Job = serde_json::from_str(source).expect("the job parses");
    assert_eq!(check(&job).status, Status::Error);
}

#[test]
fn a_constant_without_a_value_is_an_error() {
    let source = r#"{
        "spec": "---- MODULE M ----\nCONSTANT K\nVARIABLE s\nInit == s = K\n====",
        "schema": {"s": {"kind": "int"}},
        "states": [{"s": 1}]
    }"#;
    let job: Job = serde_json::from_str(source).expect("the job parses");
    let report = check(&job);
    assert_eq!(report.status, Status::Error);
    assert!(report.detail.contains('K'), "{}", report.detail);
}

#[test]
fn the_status_names_match_the_harness_vocabulary() {
    let rendered = serde_json::to_value(check(&job(
        r#"[{"buf": 1, "nextVal": 1, "received": []}]"#,
        "[]",
        "{}",
    )))
    .expect("the report encodes");
    assert_eq!(rendered["status"], "contract_violation");
}

/// The report says what the implementation looked as though it was trying to
/// do, which is the part a model checker cannot supply.
#[test]
fn a_refinement_failure_names_the_action_that_came_closest() {
    let states = r#"[
        {"buf": [],        "nextVal": 1, "received": []},
        {"buf": [1],       "nextVal": 2, "received": []},
        {"buf": [1, 2],    "nextVal": 3, "received": []},
        {"buf": [1, 2, 3], "nextVal": 4, "received": []}
    ]"#;
    let edges = r#"[[0,1,"put"],[1,2,"put"],[2,3,"overfill"]]"#;
    let report = check(&job(states, edges, "{}"));

    let closest = report.blocked.first().expect("an action came close");
    assert_eq!(closest.action, "Put");
    assert_eq!(closest.conjunct, "Len(buf) < Capacity");
    assert!(!closest.about_next_state, "capacity is a guard");
    assert!(
        report.detail.contains("`Put`") && report.detail.contains("not available"),
        "{}",
        report.detail
    );
}

#[test]
fn a_passing_check_carries_no_diagnosis() {
    let report = check(&job(WALK, r#"[[0,1,"put"],[1,2,"get"]]"#, "{}"));
    assert!(report.blocked.is_empty());
    let rendered = serde_json::to_value(&report).expect("the report encodes");
    assert!(
        rendered.get("blocked").is_none(),
        "an empty list is omitted"
    );
}

#[test]
fn the_verdict_is_readable_as_a_boolean_and_the_counts_are_kept() {
    let passing = check(&job(WALK, r#"[[0,1,"put"],[1,2,"get"]]"#, "{}"));
    assert!(passing.passed());
    assert_eq!(passing.stats.states, 3);
    assert_eq!(passing.stats.edges, 2);
    assert_eq!(passing.stats.edges_checked, 2);

    let failing = check(&job(
        r#"[{"buf": [7], "nextVal": 1, "received": []}]"#,
        "[]",
        "{}",
    ));
    assert!(!failing.passed());
    assert_eq!(failing.stats.states, 1);
    assert_eq!(failing.stats.edges, 0);
}

/// An initial-state failure quotes the state, so whoever reads the report does
/// not have to go and look it up.
#[test]
fn an_illegal_root_is_quoted_in_the_report() {
    let report = check(&job(
        r#"[{"buf": [7], "nextVal": 1, "received": []}]"#,
        "[]",
        "{}",
    ));
    assert_eq!(report.status, Status::Init);
    assert!(report.detail.contains("buf = <<7>>"), "{}", report.detail);
    assert!(report.detail.contains("nextVal = 1"), "{}", report.detail);
}

/// A state the schema accepts can still be one the specification cannot read.
/// That is the candidate's fault but not a protocol mistake, so it is the
/// contract arm rather than an error or a refinement failure.
#[test]
fn a_state_the_specification_cannot_read_is_a_contract_violation() {
    let source = r#"{
        "spec": "---- MODULE M ----\nEXTENDS Sequences\nVARIABLE s\nInit == Len(s) = 0\n====",
        "schema": {"s": {"kind": "int"}},
        "states": [{"s": 5}]
    }"#;
    let job: Job = serde_json::from_str(source).expect("the job parses");
    let report = check(&job);
    assert_eq!(report.status, Status::Contract, "{}", report.detail);
    assert!(report.detail.contains("Len"), "{}", report.detail);
}

#[test]
fn an_edge_pointing_outside_the_graph_is_an_error() {
    let report = check(&job(WALK, r#"[[0,99,"nowhere"]]"#, "{}"));
    assert_eq!(report.status, Status::Error, "{}", report.detail);
    assert!(
        report.detail.contains("outside the graph"),
        "{}",
        report.detail
    );
}

/// JSON has one number type and one boolean type; the schema says which the
/// specification wanted, and `true` is not an integer.
#[test]
fn a_boolean_is_not_accepted_where_an_integer_is_wanted() {
    let source = r#"{
        "spec": "---- MODULE M ----\nVARIABLE s\nInit == s = 1\n====",
        "schema": {"s": {"kind": "int"}},
        "states": [{"s": true}]
    }"#;
    let job: Job = serde_json::from_str(source).expect("the job parses");
    let report = check(&job);
    assert_eq!(report.status, Status::Contract, "{}", report.detail);
    assert!(report.detail.contains("integer"), "{}", report.detail);
}

#[test]
fn an_outcome_that_is_not_a_predicate_is_an_error() {
    let outcomes = r#"{"nonsense": "1 + 1"}"#;
    let report = check(&job(WALK, "[]", outcomes));
    assert_eq!(report.status, Status::Error, "{}", report.detail);
    assert!(report.detail.contains("not a boolean"), "{}", report.detail);
}

#[test]
fn an_unparsable_outcome_names_itself() {
    let outcomes = r#"{"broken": "\\E s \\in"}"#;
    let report = check(&job(WALK, "[]", outcomes));
    assert_eq!(report.status, Status::Error, "{}", report.detail);
    assert!(report.detail.contains("broken"), "{}", report.detail);
}
