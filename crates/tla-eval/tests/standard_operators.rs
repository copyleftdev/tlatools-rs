//! The operators the standard modules provide, and the value model underneath
//! them. Each is exercised directly, including at the boundaries where an
//! off-by-one would otherwise go unnoticed.

use std::collections::{BTreeMap, BTreeSet};

use tla_eval::{Evaluator, Infinite, Spec, Value};

fn value(expr: &str) -> Value {
    let src = format!(
        "---- MODULE T ----\n\
         EXTENDS Naturals, Sequences, FiniteSets, TLC\n\
         VARIABLE v\n\
         X == {expr}\n\
         ===="
    );
    let spec = Spec::parse(&src).unwrap_or_else(|e| panic!("parsing `{expr}`: {e}"));
    let evaluator = Evaluator::new(&spec, BTreeMap::new()).expect("no constants");
    let state = BTreeMap::from([("v".to_string(), Value::Int(0))]);
    evaluator
        .value_of("X", &state)
        .unwrap_or_else(|e| panic!("evaluating `{expr}`: {e}"))
}

fn failure(expr: &str) -> String {
    let src = format!(
        "---- MODULE T ----\n\
         EXTENDS Naturals, Sequences, FiniteSets, TLC\n\
         VARIABLE v\n\
         X == {expr}\n\
         ===="
    );
    let spec = Spec::parse(&src).unwrap_or_else(|e| panic!("parsing `{expr}`: {e}"));
    let evaluator = Evaluator::new(&spec, BTreeMap::new()).expect("no constants");
    let state = BTreeMap::from([("v".to_string(), Value::Int(0))]);
    match evaluator.value_of("X", &state) {
        Ok(v) => panic!("`{expr}` gave {v} where an error was expected"),
        Err(e) => e.to_string(),
    }
}

fn int(n: i64) -> Value {
    Value::Int(n)
}

fn seq(items: &[i64]) -> Value {
    Value::Seq(items.iter().copied().map(Value::Int).collect())
}

// ------------------------------------------------------------- FiniteSets

#[test]
fn cardinality_counts_a_set() {
    assert_eq!(value("Cardinality({})"), int(0));
    assert_eq!(value("Cardinality({1, 2, 2, 3})"), int(3));
}

#[test]
fn is_finite_set_distinguishes_the_enumerable() {
    assert_eq!(value("IsFiniteSet({1, 2})"), Value::Bool(true));
    assert_eq!(value("IsFiniteSet(Nat)"), Value::Bool(false));
}

#[test]
fn cardinality_of_an_infinite_set_is_refused() {
    assert!(failure("Cardinality(Nat)").contains("infinite"));
}

// -------------------------------------------------------------- Sequences

#[test]
fn sequence_operators() {
    assert_eq!(value("Len(<<1, 2, 3>>)"), int(3));
    assert_eq!(value("Len(<<>>)"), int(0));
    assert_eq!(value("Head(<<7, 8>>)"), int(7));
    assert_eq!(value("Tail(<<7, 8>>)"), seq(&[8]));
    assert_eq!(value("Tail(<<7>>)"), seq(&[]));
    assert_eq!(value("Append(<<1>>, 2)"), seq(&[1, 2]));
    assert_eq!(value("<<1, 2>> \\o <<3>>"), seq(&[1, 2, 3]));
}

#[test]
fn head_and_tail_of_an_empty_sequence_are_refused() {
    assert!(failure("Head(<<>>)").contains("empty"));
    assert!(failure("Tail(<<>>)").contains("empty"));
}

/// `SubSeq` clamps at both ends and returns nothing when they cross, so the
/// comparison it turns on is checked either side of the boundary.
#[test]
fn subseq_clamps_and_can_be_empty() {
    assert_eq!(value("SubSeq(<<1, 2, 3>>, 2, 3)"), seq(&[2, 3]));
    assert_eq!(value("SubSeq(<<1, 2, 3>>, 2, 2)"), seq(&[2]));
    assert_eq!(value("SubSeq(<<1, 2, 3>>, 3, 2)"), seq(&[]));
    assert_eq!(value("SubSeq(<<1, 2, 3>>, 1, 9)"), seq(&[1, 2, 3]));
    assert_eq!(value("SubSeq(<<1, 2, 3>>, 0, 2)"), seq(&[1, 2]));
}

/// `Seq(S)` is every finite sequence over `S`: membership is decidable,
/// enumeration is not.
#[test]
fn the_set_of_sequences_supports_membership_only() {
    assert_eq!(value("<<1, 2>> \\in Seq({1, 2})"), Value::Bool(true));
    assert_eq!(value("<<1, 3>> \\in Seq({1, 2})"), Value::Bool(false));
    assert_eq!(value("<<>> \\in Seq({1})"), Value::Bool(true));
    assert_eq!(value("5 \\in Seq({1})"), Value::Bool(false));
    assert!(failure("Cardinality(Seq({1}))").contains("infinite"));
}

// --------------------------------------------------------------------- TLC

#[test]
fn assert_passes_or_reports_its_message() {
    assert_eq!(value(r#"Assert(TRUE, "unused")"#), Value::Bool(true));
    assert!(failure(r#"Assert(FALSE, "the reason")"#).contains("the reason"));
}

#[test]
fn printing_operators_pass_their_value_through() {
    assert_eq!(value(r#"Print("label", 42)"#), int(42));
    assert_eq!(value(r#"PrintT("label")"#), Value::Bool(true));
    assert_eq!(value("ToString(<<1, 2>>)"), Value::string("<<1, 2>>"));
}

#[test]
fn a_function_can_be_built_from_pairs() {
    assert_eq!(
        value(r#"("a" :> 1) @@ ("b" :> 2)"#),
        value(r"[a |-> 1, b |-> 2]")
    );
    // `@@` keeps the left-hand value where both define a point.
    assert_eq!(value(r#"("a" :> 1) @@ ("a" :> 9)"#), value(r"[a |-> 1]"));
}

// ------------------------------------------------------- the infinite sets

#[test]
fn the_infinite_sets_decide_membership() {
    assert_eq!(value("3 \\in Nat"), Value::Bool(true));
    assert_eq!(value("-3 \\in Nat"), Value::Bool(false));
    assert_eq!(value("-3 \\in Int"), Value::Bool(true));
    assert_eq!(value(r#""x" \in Int"#), Value::Bool(false));
    assert_eq!(value(r#""x" \in STRING"#), Value::Bool(true));
    assert_eq!(value("TRUE \\in BOOLEAN"), Value::Bool(true));
    assert_eq!(value("1 \\in BOOLEAN"), Value::Bool(false));
}

#[test]
fn an_infinite_set_cannot_be_enumerated() {
    assert!(failure("\\E n \\in Nat : n = 1").contains("cannot be enumerated"));
}

// ------------------------------------------------------- the value model

/// A function's representation follows its domain, so the same value written
/// two ways compares equal.
#[test]
fn functions_records_and_sequences_are_one_thing() {
    assert_eq!(value("[i \\in 1..2 |-> i]"), seq(&[1, 2]));
    assert_eq!(value(r#"[i \in {"a"} |-> 1]"#), value("[a |-> 1]"));
    assert_eq!(value("[i \\in {} |-> i]"), seq(&[]));
    // A domain that is neither `1..n` nor all strings stays a function.
    assert!(matches!(value("[i \\in {2, 4} |-> i]"), Value::Func(_)));
}

#[test]
fn domain_and_application_agree_for_every_representation() {
    let sequence = seq(&[10, 20]);
    assert_eq!(
        sequence.domain(),
        Some(BTreeSet::from([int(1), int(2)])),
        "a sequence is a function on 1..n"
    );
    assert_eq!(sequence.apply(&int(1)), Some(int(10)));
    assert_eq!(sequence.apply(&int(0)), None);
    assert_eq!(sequence.apply(&int(3)), None);

    let record = value("[a |-> 1, b |-> 2]");
    assert_eq!(
        record.domain(),
        Some(BTreeSet::from([Value::string("a"), Value::string("b")]))
    );
    assert_eq!(record.apply(&Value::string("b")), Some(int(2)));
    assert_eq!(record.apply(&Value::string("c")), None);

    let function = value("[i \\in {2, 4} |-> i]");
    assert_eq!(function.domain(), Some(BTreeSet::from([int(2), int(4)])));
    assert_eq!(function.apply(&int(4)), Some(int(4)));

    assert_eq!(int(1).domain(), None, "an integer is not a function");
    assert_eq!(int(1).apply(&int(1)), None);
}

#[test]
fn the_graph_of_a_function_can_be_read_back() {
    assert_eq!(
        seq(&[10, 20]).entries(),
        Some(BTreeMap::from([(int(1), int(10)), (int(2), int(20))])),
        "a sequence is indexed from one"
    );
    assert_eq!(
        value("[a |-> 1]").entries(),
        Some(BTreeMap::from([(Value::string("a"), int(1))]))
    );
    assert!(value("[i \\in {2, 4} |-> i]").entries().is_some());
    assert_eq!(int(1).entries(), None);
    assert_eq!(Value::set([int(1)]).entries(), None);
}

#[test]
fn values_describe_themselves() {
    for (v, name, printed) in [
        (Value::Bool(true), "a boolean", "TRUE"),
        (Value::Bool(false), "a boolean", "FALSE"),
        (int(-2), "an integer", "-2"),
        (Value::string("s"), "a string", "\"s\""),
        (seq(&[1]), "a sequence", "<<1>>"),
        (Value::set([int(1)]), "a set", "{1}"),
        (value("[a |-> 1]"), "a record", "[a |-> 1]"),
        (
            value("[i \\in {2, 4} |-> i]"),
            "a function",
            "(2 :> 2 @@ 4 :> 4)",
        ),
        (Value::Infinite(Infinite::Nat), "a set", "Nat"),
        (Value::Infinite(Infinite::Int), "a set", "Int"),
        (Value::Infinite(Infinite::Strings), "a set", "STRING"),
        (
            Value::Infinite(Infinite::Sequences(Box::new(Value::set([int(1)])))),
            "a set",
            "Seq({1})",
        ),
    ] {
        assert_eq!(v.type_name(), name, "{v}");
        assert_eq!(v.to_string(), printed);
        assert_eq!(v.is_set(), name == "a set", "{v}");
    }
}

// -------------------------------------------------------------- arithmetic

#[test]
fn arithmetic_reports_rather_than_wraps() {
    assert_eq!(value("7 \\div 2"), int(3));
    assert_eq!(value("7 % 2"), int(1));
    assert_eq!(value("2 ^ 10"), int(1024));
    assert!(failure("1 \\div 0").contains("undefined"));
    assert!(failure("2 ^ 1000").contains("overflow"));
}

#[test]
fn set_operators() {
    assert_eq!(value("{1, 2} \\cup {2, 3}"), value("{1, 2, 3}"));
    assert_eq!(value("{1, 2} \\cap {2, 3}"), value("{2}"));
    assert_eq!(value("{1, 2} \\ {2}"), value("{1}"));
    assert_eq!(value("SUBSET {1, 2}"), value("{{}, {1}, {2}, {1, 2}}"));
    assert_eq!(value("UNION {{1}, {2}}"), value("{1, 2}"));
    assert_eq!(value("{1} \\X {2}"), value("{<<1, 2>>}"));
    assert_eq!(value("{1, 2} \\subseteq {1, 2, 3}"), Value::Bool(true));
    assert_eq!(value("{1, 4} \\subseteq {1, 2, 3}"), Value::Bool(false));
}
