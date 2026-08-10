use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use tla_eval::{Error as EvalError, Evaluator, Spec, State, Value};
use tla_syntax::{Expr, parse_expression};

use crate::schema::{Schema, decode, decode_state};

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    /// The specification's source, not a path: the caller decides where it
    /// comes from and the oracle never touches the filesystem.
    pub spec: String,
    #[serde(default = "init_default")]
    pub init_op: String,
    #[serde(default = "next_default")]
    pub next_op: String,
    #[serde(default)]
    pub constants: BTreeMap<String, ConstantSpec>,
    pub schema: BTreeMap<String, Schema>,
    pub states: Vec<Json>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Predicates over `Reached`, the set of states the implementation visited.
    #[serde(default)]
    pub outcomes: BTreeMap<String, String>,
}

fn init_default() -> String {
    "Init".to_string()
}

fn next_default() -> String {
    "Next".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConstantSpec {
    pub value: Json,
    pub schema: Schema,
}

/// `[source, target, label]`, as the explorer reports it.
pub type Edge = (usize, usize, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    /// The root is not a legal initial state.
    Init,
    /// Some edge is not a step the specification permits.
    Refines,
    /// A required outcome is unreachable.
    Coverage,
    /// The implementation's states do not have the shape the specification and
    /// schema call for. The candidate is at fault, but not for a protocol
    /// mistake.
    #[serde(rename = "contract_violation")]
    Contract,
    /// The job itself could not be carried out: an unparsable specification, a
    /// constant with no value, a predicate that is not ground.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeReport {
    pub index: usize,
    pub source: usize,
    pub target: usize,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Stats {
    pub states: usize,
    pub edges: usize,
    pub edges_checked: usize,
    pub outcomes_checked: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub status: Status,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<EdgeReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub stats: Stats,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.status == Status::Pass
    }

    fn of(status: Status, detail: impl Into<String>, stats: Stats) -> Self {
        Self {
            status,
            detail: detail.into(),
            edge: None,
            outcome: None,
            stats,
        }
    }
}

pub fn check(job: &Job) -> Report {
    let mut stats = Stats {
        states: job.states.len(),
        edges: job.edges.len(),
        ..Stats::default()
    };

    let spec = match Spec::parse(&job.spec) {
        Ok(s) => s,
        Err(e) => return Report::of(Status::Error, format!("specification: {e}"), stats),
    };

    let constants = match decode_constants(&job.constants) {
        Ok(c) => c,
        Err(detail) => return Report::of(Status::Error, detail, stats),
    };

    let states = match decode_states(job) {
        Ok(s) => s,
        Err(detail) => return Report::of(Status::Contract, detail, stats),
    };
    let Some(root) = states.first() else {
        return Report::of(
            Status::Contract,
            "the implementation reported no states at all",
            stats,
        );
    };

    let outcomes = match parse_outcomes(job) {
        Ok(o) => o,
        Err(detail) => return Report::of(Status::Error, detail, stats),
    };

    let evaluator = match Evaluator::new(&spec, constants.clone()) {
        Ok(e) => e,
        Err(e) => return Report::of(Status::Error, e.to_string(), stats),
    };

    match evaluator.holds_at(&job.init_op, root) {
        Ok(true) => {}
        Ok(false) => {
            return Report::of(
                Status::Init,
                format!(
                    "the first state the implementation reports is not a legal initial \
                     state of the specification: {}",
                    render(root)
                ),
                stats,
            );
        }
        Err(e) => return Report::of(classify(&e), format!("{}: {e}", job.init_op), stats),
    }

    if let Some(failure) = check_edges(job, &evaluator, &states, &mut stats) {
        return failure;
    }

    if outcomes.is_empty() {
        return Report::of(Status::Pass, "all obligations discharged", stats);
    }

    // `Reached` is what a coverage predicate quantifies over, and it behaves
    // exactly like a constant: fixed for the whole check, mentioning no
    // variable.
    let mut with_reached = constants;
    with_reached.insert(
        "Reached".to_string(),
        Value::set(states.iter().cloned().map(Value::Record)),
    );
    let coverage = match Evaluator::new(&spec, with_reached) {
        Ok(e) => e,
        Err(e) => return Report::of(Status::Error, e.to_string(), stats),
    };
    if let Some(failure) = check_outcomes(job, &coverage, &outcomes, &mut stats) {
        return failure;
    }

    Report::of(Status::Pass, "all obligations discharged", stats)
}

fn check_edges(
    job: &Job,
    evaluator: &Evaluator<'_>,
    states: &[State],
    stats: &mut Stats,
) -> Option<Report> {
    for (index, (source, target, label)) in job.edges.iter().enumerate() {
        let (Some(from), Some(to)) = (states.get(*source), states.get(*target)) else {
            return Some(Report::of(
                Status::Error,
                format!("edge {index} refers to a state index outside the graph"),
                *stats,
            ));
        };
        stats.edges_checked = index + 1;
        match evaluator.step_allowed(&job.next_op, from, to) {
            Ok(true) => {}
            Ok(false) => {
                let mut report = Report::of(
                    Status::Refines,
                    format!(
                        "no action of the specification takes {} to {}",
                        render(from),
                        render(to)
                    ),
                    *stats,
                );
                report.edge = Some(EdgeReport {
                    index,
                    source: *source,
                    target: *target,
                    label: label.clone(),
                });
                return Some(report);
            }
            Err(e) => {
                return Some(Report::of(
                    classify(&e),
                    format!("{}: {e}", job.next_op),
                    *stats,
                ));
            }
        }
    }
    None
}

fn check_outcomes(
    job: &Job,
    coverage: &Evaluator<'_>,
    outcomes: &[(String, Expr)],
    stats: &mut Stats,
) -> Option<Report> {
    let nowhere = State::new();
    for (name, expr) in outcomes {
        stats.outcomes_checked += 1;
        match coverage.eval_at(expr, &nowhere, None) {
            Ok(Value::Bool(true)) => {}
            Ok(Value::Bool(false)) => {
                let mut report = Report::of(
                    Status::Coverage,
                    format!(
                        "every transition is legal, but no reachable state satisfies the \
                         required outcome `{name}`: {}",
                        job.outcomes[name]
                    ),
                    *stats,
                );
                report.outcome = Some(name.clone());
                return Some(report);
            }
            Ok(other) => {
                return Some(Report::of(
                    Status::Error,
                    format!("outcome `{name}` is {other}, not a boolean"),
                    *stats,
                ));
            }
            Err(e) => {
                return Some(Report::of(
                    Status::Error,
                    format!("outcome `{name}`: {e}"),
                    *stats,
                ));
            }
        }
    }
    None
}

/// A type error means the state does not fit the shape the specification reads
/// it with — the schema let it through, but the specification cannot use it.
fn classify(e: &EvalError) -> Status {
    match e {
        EvalError::Type(_) => Status::Contract,
        _ => Status::Error,
    }
}

fn decode_constants(
    given: &BTreeMap<String, ConstantSpec>,
) -> Result<BTreeMap<String, Value>, String> {
    given
        .iter()
        .map(|(name, spec)| {
            decode(&spec.value, &spec.schema, name)
                .map(|v| (name.clone(), v))
                .map_err(|e| format!("constant {e}"))
        })
        .collect()
}

fn decode_states(job: &Job) -> Result<Vec<State>, String> {
    job.states
        .iter()
        .enumerate()
        .map(|(i, json)| {
            decode_state(json, &job.schema, &format!("state[{i}]")).map_err(|e| e.to_string())
        })
        .collect()
}

fn parse_outcomes(job: &Job) -> Result<Vec<(String, Expr)>, String> {
    job.outcomes
        .iter()
        .map(|(name, src)| {
            parse_expression(src)
                .map(|e| (name.clone(), e))
                .map_err(|e| format!("outcome `{name}`: {e}"))
        })
        .collect()
}

fn render(state: &State) -> String {
    let body: Vec<String> = state
        .iter()
        .map(|(name, value)| format!("{name} = {value}"))
        .collect();
    format!("[{}]", body.join(", "))
}
