//! Operators the specification defines itself, operators passed as arguments,
//! and modules built out of other modules.

use std::collections::BTreeMap;

use tla_eval::{Evaluator, Modules, Spec, State, Value};

/// Modules held in memory, so a test can describe a whole specification.
struct Sources(BTreeMap<&'static str, &'static str>);

impl Modules for Sources {
    fn source(&self, name: &str) -> Option<String> {
        self.0.get(name).map(|s| (*s).to_string())
    }
}

fn spec(src: &str) -> Spec {
    Spec::parse(src).unwrap_or_else(|e| panic!("{e}\nin:\n{src}"))
}

fn evaluator<'a>(spec: &'a Spec, constants: &[(&str, Value)]) -> Evaluator<'a> {
    let map = constants
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    Evaluator::new(spec, map).expect("the constants cover the declarations")
}

fn state(pairs: &[(&str, i64)]) -> State {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), Value::Int(*v)))
        .collect()
}

// ------------------------------------------------- operators a spec defines

const OPS: &str = r"---- MODULE Ops ----
EXTENDS Naturals
VARIABLE x
a \prec b == a < b
-. a == 0 - a
b ^+ == b + 1
\* `-.` is how a prefix definition is written; the use is plain `-x`.
Small   == x \prec 5
Negated == -x
Bumped  == x^+
====================";

#[test]
fn an_infix_operator_the_spec_defines_is_evaluated() {
    let m = spec(OPS);
    let e = evaluator(&m, &[]);
    assert_eq!(
        e.value_of("Small", &state(&[("x", 3)])).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        e.value_of("Small", &state(&[("x", 9)])).unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn prefix_and_postfix_definitions_are_evaluated() {
    let m = spec(OPS);
    let e = evaluator(&m, &[]);
    let at = state(&[("x", 4)]);
    assert_eq!(e.value_of("Negated", &at).unwrap(), Value::Int(-4));
    assert_eq!(e.value_of("Bumped", &at).unwrap(), Value::Int(5));
}

// ------------------------------------------------------ operator arguments

const HIGHER_ORDER: &str = r"---- MODULE HigherOrder ----
EXTENDS Naturals
VARIABLE x
Twice(f(_), v)      == f(f(v))
Combine(op(_, _), a, b) == op(a, b)
Inc(n)   == n + 1
ByName   == Twice(Inc, 1)
ByLambda == Twice(LAMBDA k : k * 2, 3)
BySymbol == Combine(+, 2, 3)
Nested   == Combine(LAMBDA a, b : a * b, x, x)
====================";

#[test]
fn an_operator_may_be_passed_by_name() {
    let m = spec(HIGHER_ORDER);
    let e = evaluator(&m, &[]);
    assert_eq!(
        e.value_of("ByName", &state(&[("x", 0)])).unwrap(),
        Value::Int(3)
    );
}

#[test]
fn a_lambda_may_be_passed_and_applied() {
    let m = spec(HIGHER_ORDER);
    let e = evaluator(&m, &[]);
    assert_eq!(
        e.value_of("ByLambda", &state(&[("x", 0)])).unwrap(),
        Value::Int(12)
    );
}

/// `Combine(+, 2, 3)` passes the operator itself.
#[test]
fn an_operator_symbol_may_be_passed() {
    let m = spec(HIGHER_ORDER);
    let e = evaluator(&m, &[]);
    assert_eq!(
        e.value_of("BySymbol", &state(&[("x", 0)])).unwrap(),
        Value::Int(5)
    );
}

#[test]
fn a_lambda_sees_the_state_it_was_written_in() {
    let m = spec(HIGHER_ORDER);
    let e = evaluator(&m, &[]);
    assert_eq!(
        e.value_of("Nested", &state(&[("x", 6)])).unwrap(),
        Value::Int(36)
    );
}

// -------------------------------------------------------------- extending

#[test]
fn a_module_may_extend_another() {
    let sources = Sources(BTreeMap::from([(
        "Base",
        "---- MODULE Base ----\nEXTENDS Naturals\nCONSTANT Bound\nVARIABLE x\nInRange == x <= Bound\n====",
    )]));
    let m = Spec::load(
        "---- MODULE Top ----\nEXTENDS Base\nOk == InRange /\\ x >= 0\n====",
        &sources,
    )
    .expect("Base is found");

    // A variable and a constant declared in Base belong to Top as well.
    assert_eq!(m.variables().collect::<Vec<_>>(), ["x"]);
    assert_eq!(m.constants().collect::<Vec<_>>(), ["Bound"]);

    let e = evaluator(&m, &[("Bound", Value::Int(5))]);
    assert_eq!(
        e.value_of("Ok", &state(&[("x", 3)])).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        e.value_of("Ok", &state(&[("x", 9)])).unwrap(),
        Value::Bool(false)
    );
}

#[test]
fn a_module_that_cannot_be_found_is_reported() {
    let err = Spec::parse("---- MODULE Top ----\nEXTENDS Missing\nX == 1\n====")
        .expect_err("Missing is not a standard module");
    assert!(format!("{err}").contains("Missing"), "{err}");
}

// -------------------------------------------------------------- instances

/// Two independent counters, each an instance of the same module with its own
/// constant and its own variable.
const COMPOSED: &str = r"---- MODULE Composed ----
EXTENDS Naturals
VARIABLES a, b

---- MODULE Counter ----
EXTENDS Naturals
CONSTANT Limit
VARIABLE n
CInit == n = 0
CNext == n < Limit /\ n' = n + 1
====================

A == INSTANCE Counter WITH Limit <- 3, n <- a
B == INSTANCE Counter WITH Limit <- 5, n <- b

Init == A!CInit /\ B!CInit
Next == \/ A!CNext /\ b' = b
        \/ B!CNext /\ a' = a
====================";

fn composed() -> Spec {
    spec(COMPOSED)
}

#[test]
fn an_instance_substitutes_constants_and_variables() {
    let m = composed();
    let e = evaluator(&m, &[]);
    assert!(e.holds_at("Init", &state(&[("a", 0), ("b", 0)])).unwrap());
    assert!(!e.holds_at("Init", &state(&[("a", 1), ("b", 0)])).unwrap());
}

/// `n' = n + 1` inside `Counter` has to mean `a' = a + 1` here: priming
/// reaches through the substitution, which is why a substitution is held as an
/// expression and not as a value.
#[test]
fn priming_reaches_through_a_substitution() {
    let m = composed();
    let e = evaluator(&m, &[]);
    let start = state(&[("a", 0), ("b", 0)]);
    assert!(
        e.step_allowed("Next", &start, &state(&[("a", 1), ("b", 0)]))
            .unwrap()
    );
    assert!(
        e.step_allowed("Next", &start, &state(&[("a", 0), ("b", 1)]))
            .unwrap()
    );
    assert!(
        !e.step_allowed("Next", &start, &state(&[("a", 2), ("b", 0)]))
            .unwrap()
    );
    assert!(
        !e.step_allowed("Next", &start, &state(&[("a", 1), ("b", 1)]))
            .unwrap()
    );
}

/// The two instances differ only in what was substituted, so the same
/// definition has to give different answers through each.
#[test]
fn instances_of_one_module_keep_their_own_constants() {
    let m = composed();
    let e = evaluator(&m, &[]);
    // `a` has reached A's limit of 3; `b` has not reached B's limit of 5.
    let at = state(&[("a", 3), ("b", 3)]);
    assert!(
        !e.step_allowed("Next", &at, &state(&[("a", 4), ("b", 3)]))
            .unwrap()
    );
    assert!(
        e.step_allowed("Next", &at, &state(&[("a", 3), ("b", 4)]))
            .unwrap()
    );
}

#[test]
fn a_name_the_with_clause_omits_keeps_its_own_name() {
    let sources = Sources(BTreeMap::from([(
        "Inner",
        "---- MODULE Inner ----\nEXTENDS Naturals\nCONSTANTS P, Q\nSum == P + Q\n====",
    )]));
    let m = Spec::load(
        "---- MODULE Outer ----\nEXTENDS Naturals\nCONSTANT Q\nVARIABLE v\n\
         I == INSTANCE Inner WITH P <- 10\nTotal == I!Sum\n====",
        &sources,
    )
    .expect("Inner is found");
    let e = evaluator(&m, &[("Q", Value::Int(7))]);
    assert_eq!(
        e.value_of("Total", &state(&[("v", 0)])).unwrap(),
        Value::Int(17)
    );
}

#[test]
fn instances_may_be_chained() {
    let sources = Sources(BTreeMap::from([
        (
            "Leaf",
            "---- MODULE Leaf ----\nEXTENDS Naturals\nCONSTANT K\nValue == K * 2\n====",
        ),
        (
            "Middle",
            "---- MODULE Middle ----\nEXTENDS Naturals\nCONSTANT M\n\
             L == INSTANCE Leaf WITH K <- M + 1\n====",
        ),
    ]));
    let m = Spec::load(
        "---- MODULE Top ----\nEXTENDS Naturals\nVARIABLE v\n\
         Mid == INSTANCE Middle WITH M <- 4\nAnswer == Mid!L!Value\n====",
        &sources,
    )
    .expect("both modules are found");
    let e = evaluator(&m, &[]);
    assert_eq!(
        e.value_of("Answer", &state(&[("v", 0)])).unwrap(),
        Value::Int(10)
    );
}

#[test]
fn an_unknown_instance_is_reported_by_name() {
    let m = spec("---- MODULE T ----\nVARIABLE v\nX == Nope!Thing\n====");
    let e = evaluator(&m, &[]);
    let err = e
        .value_of("X", &state(&[("v", 0)]))
        .expect_err("no such instance");
    assert!(format!("{err}").contains("Nope"), "{err}");
}
