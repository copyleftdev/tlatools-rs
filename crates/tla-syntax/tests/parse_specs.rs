//! Every specification the parser is meant to serve must parse, and parse into
//! the shape the evaluator will look for. The fixtures are the real specs from
//! the benchmark this crate exists to serve, plus one module in the generated
//! form the oracle emits.

use std::path::Path;

use tla_syntax::{Expr, QuantKind, Unit, parse_module};

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn all_fixtures() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("fixture directory")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into())
        .collect();
    names.sort();
    names
}

#[test]
fn every_fixture_parses() {
    for name in all_fixtures() {
        let src = fixture(&name);
        if let Err(e) = parse_module(&src) {
            panic!("{name}: {e}");
        }
    }
}

#[test]
fn module_name_and_extends() {
    let m = parse_module(&fixture("Paxos.tla")).expect("parses");
    assert_eq!(m.name, "Paxos");
    assert_eq!(m.extends, vec!["Integers", "FiniteSets"]);
}

#[test]
fn declarations_are_collected() {
    let m = parse_module(&fixture("Paxos.tla")).expect("parses");
    let constants: Vec<&str> = m.constants().map(|d| d.name.as_str()).collect();
    assert_eq!(constants, ["Acceptor", "Value", "Ballot", "Quorum"]);
    let variables: Vec<&str> = m.variables().map(String::as_str).collect();
    assert_eq!(variables, ["maxBal", "maxVBal", "maxVal", "msgs"]);
}

#[test]
fn operator_definitions_keep_their_parameters() {
    let m = parse_module(&fixture("Paxos.tla")).expect("parses");
    let d = m.definition("Phase2a").expect("Phase2a is defined");
    assert_eq!(d.params, ["b", "v"]);
}

/// A bulleted list is scoped by the column of its bullets, so the trailing
/// `/\ Send(...)` belongs to `Phase2a`'s outer conjunction and not to the
/// `\E Q \in Quorum` nested three levels inside it.
#[test]
fn junction_lists_are_scoped_by_column() {
    let m = parse_module(&fixture("Paxos.tla")).expect("parses");
    let body = &m.definition("Phase2a").expect("defined").body;
    assert_eq!(
        conjuncts(body).len(),
        4,
        "Phase2a has four top-level conjuncts"
    );
}

#[test]
fn nested_junctions_nest() {
    let m = parse_module(&fixture("TwoPhase.tla")).expect("parses");
    let body = &m.definition("TPNext").expect("defined").body;
    let top = disjuncts(body);
    assert_eq!(top.len(), 3, "TMCommit, TMAbort and the quantified group");
    let Expr::Quant { kind, body, .. } = top[2] else {
        panic!("third disjunct is the existential, got {:?}", top[2]);
    };
    assert_eq!(*kind, QuantKind::Exists);
    assert_eq!(disjuncts(body).len(), 5, "five actions under \\E r \\in RM");
}

/// `[][A]_v /\ B` is a conjunction of two formulas: `[]` binds tighter.
#[test]
fn always_binds_tighter_than_conjunction() {
    let m = parse_module(&fixture("TV.tla")).expect("parses");
    let body = &m.definition("TraceSpec").expect("defined").body;
    assert_eq!(conjuncts(body).len(), 3);
}

#[test]
fn instance_substitutions_are_recorded() {
    let m = parse_module(&fixture("TV.tla")).expect("parses");
    let inst = m
        .units
        .iter()
        .find_map(|u| match u {
            Unit::Instance { name, module, subs } => Some((name, module, subs)),
            _ => None,
        })
        .expect("TV instantiates TwoPhase");
    assert_eq!(inst.0.as_deref(), Some("S"));
    assert_eq!(inst.1, "TwoPhase");
    assert_eq!(inst.2.len(), 1);
    assert_eq!(inst.2[0].0, "RM");
}

#[test]
fn qualified_names_resolve_through_the_instance() {
    let m = parse_module(&fixture("TV.tla")).expect("parses");
    let body = &m.definition("TraceNext").expect("defined").body;
    let found = conjuncts(body).iter().any(|e| {
        matches!(e, Expr::Qualified { instance, name, .. } if instance == "S" && name == "TPNext")
    });
    assert!(found, "S!TPNext appears as a conjunct");
}

#[test]
fn a_missing_header_is_an_error() {
    let err = parse_module("Init == x = 1").expect_err("no MODULE header");
    assert!(err.message.contains("MODULE"), "{err}");
}

#[test]
fn errors_carry_a_position() {
    let err = parse_module("---- MODULE M ----\nFoo == [a |-> ]\n====================")
        .expect_err("empty record field");
    assert_eq!(err.line, 2);
}

fn conjuncts(e: &Expr) -> Vec<&Expr> {
    flatten(e, tla_syntax::token::Op::And)
}

fn disjuncts(e: &Expr) -> Vec<&Expr> {
    flatten(e, tla_syntax::token::Op::Or)
}

fn flatten(e: &Expr, op: tla_syntax::token::Op) -> Vec<&Expr> {
    match e {
        Expr::Binary(o, l, r) if *o == op => {
            let mut out = flatten(l, op);
            out.extend(flatten(r, op));
            out
        }
        other => vec![other],
    }
}
