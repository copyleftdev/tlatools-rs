//! One case per corner of the grammar, kept self-contained.
//!
//! The parser is measured against the public corpus of TLA+ specifications
//! (see `examples/audit.rs`), but that corpus lives elsewhere. These snippets
//! pin the constructs that measurement drove out, so the coverage cannot
//! silently regress in a checkout that has no corpus.

use tla_syntax::{Expr, Unit, parse_module};

fn module(body: &str) -> tla_syntax::Module {
    let src = format!("---- MODULE T ----\n{body}\n====================");
    parse_module(&src).unwrap_or_else(|e| panic!("{e}\nin:\n{src}"))
}

fn definition(body: &str, name: &str) -> Expr {
    module(body)
        .definition(name)
        .unwrap_or_else(|| panic!("`{name}` is not defined"))
        .body
        .clone()
}

/// Text around the module is prose, not TLA+, and routinely contains
/// characters the language does not allow loose.
#[test]
fn prose_outside_the_module_is_not_lexed() {
    let src = "\
Notes: see http://example.com/a/b -- costs $5 & rising.

---- MODULE T ----
Foo == 1
====================

$ tlc T.tla -config T.cfg
Ran 100% clean & finished.";
    let m = parse_module(src).expect("the prose is ignored");
    assert_eq!(m.name, "T");
    assert_eq!(m.units.len(), 1);
}

#[test]
fn an_infix_operator_can_be_defined() {
    let m = module("a \\prec b == a < b");
    let def = m.definition("\\prec").expect("the operator is defined");
    let names: Vec<&str> = def.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn prefix_and_postfix_operators_can_be_defined() {
    let m = module("-. a == 0 - a\nb ^+ == b + 1");
    assert!(m.definition("-").is_some(), "prefix minus");
    assert!(m.definition("^+").is_some(), "postfix plus");
}

/// `s^+` follows its operand; treating it as infix would consume whatever
/// came next.
#[test]
fn a_postfix_operator_binds_to_what_precedes_it() {
    let body = definition("Foo == S^+ \\cup T", "Foo");
    assert!(
        matches!(&body, Expr::Binary(_, lhs, _) if matches!(**lhs, Expr::Unary(..))),
        "{body}"
    );
}

#[test]
fn a_function_may_be_defined_by_its_graph() {
    let body = definition("f[x \\in S] == x + 1", "f");
    assert!(matches!(body, Expr::FnDef { .. }), "{body}");
}

/// `CASE` arms are separated by `[]`, which also spells the always operator.
#[test]
fn case_arms_are_separated_by_boxes() {
    let body = definition(
        r#"Colour == CASE n = 1 -> "red" [] n = 2 -> "green" [] OTHER -> "grey""#,
        "Colour",
    );
    let Expr::Case { arms, other } = &body else {
        panic!("expected a CASE, got {body}");
    };
    assert_eq!(arms.len(), 2);
    assert!(other.is_some());
}

/// A bulleted list is an operand. The `=>` below ends the list and then
/// applies to the whole of it.
#[test]
fn an_operator_may_follow_a_bulleted_list() {
    let body = definition(
        "Safe ==\n  /\\ TypeOK\n  /\\ OneVote\n  => Consistent",
        "Safe",
    );
    assert!(
        matches!(&body, Expr::Binary(op, ..) if *op == tla_syntax::token::Op::Implies),
        "{body}"
    );
}

#[test]
fn higher_order_parameters_record_their_arity() {
    let m = module("Fold(op(_, _), base, S) == base");
    let def = m.definition("Fold").expect("defined");
    let arities: Vec<usize> = def.params.iter().map(|p| p.arity).collect();
    assert_eq!(arities, [2, 0, 0]);
}

#[test]
fn a_lambda_may_be_passed_where_an_operator_is_wanted() {
    let body = definition("Foo == Apply(LAMBDA x, y : x + y, 1)", "Foo");
    let Expr::Apply(_, args) = &body else {
        panic!("expected an application, got {body}");
    };
    assert!(matches!(args[0], Expr::Lambda { .. }), "{}", args[0]);
}

/// `FoldSet(+, 0, S)` passes the operator, rather than applying it.
#[test]
fn an_operator_may_be_passed_as_an_argument() {
    let body = definition("Sum(S) == FoldSet(+, 0, S)", "Sum");
    let Expr::Apply(_, args) = &body else {
        panic!("expected an application, got {body}");
    };
    assert_eq!(args[0], Expr::Ident("+".to_string()));
}

#[test]
fn temporal_quantifiers_parse_and_are_marked_as_such() {
    let body = definition("Hidden == \\EE t : Timer(t)", "Hidden");
    let Expr::Quant { kind, .. } = &body else {
        panic!("expected a quantifier, got {body}");
    };
    assert!(kind.is_temporal());
}

/// A quantifier inside `[...]` puts a colon where a record set would have
/// one; only the subscript afterwards settles which it is.
#[test]
fn an_action_subscript_is_recognised_past_an_inner_colon() {
    let body = definition("Keeps == [][\\A i \\in Proc : p[i] = p'[i]]_vars", "Keeps");
    let Expr::Unary(_, inner) = &body else {
        panic!("expected [] applied to something, got {body}");
    };
    assert!(matches!(**inner, Expr::ActionBox { .. }), "{inner}");
}

#[test]
fn a_fairness_subscript_may_be_a_tuple() {
    let body = definition("F == WF_<<a, b>>(Next)", "F");
    let Expr::Fairness { subscript, .. } = &body else {
        panic!("expected a fairness condition, got {body}");
    };
    assert!(matches!(**subscript, Expr::Tuple(_)), "{subscript}");
}

/// `CHOOSE` brings its own colon, which a lookahead would mistake for the one
/// that separates a set filter from its predicate.
#[test]
fn a_choose_inside_braces_is_not_a_set_filter() {
    let body = definition("Pick == {CHOOSE x \\in S : TRUE}", "Pick");
    let Expr::SetEnum(items) = &body else {
        panic!("expected a one-element set, got {body}");
    };
    assert!(matches!(items[0], Expr::Choose { .. }), "{}", items[0]);
}

#[test]
fn set_filters_and_set_maps_are_told_apart() {
    assert!(matches!(
        definition("A == {x \\in S : P(x)}", "A"),
        Expr::SetFilter { .. }
    ));
    assert!(matches!(
        definition("B == {f[x] : x \\in S}", "B"),
        Expr::SetMap { .. }
    ));
}

#[test]
fn except_accepts_several_indices_at_once() {
    let body = definition("Set == [f EXCEPT ![a, b] = v]", "Set");
    assert!(matches!(body, Expr::Except { .. }), "{body}");
}

#[test]
fn a_named_theorem_keeps_its_statement() {
    let m = module("THEOREM Correct == Spec => []Inv");
    assert!(m.units.iter().any(|u| matches!(u, Unit::Theorem(_))));
}

/// Proofs are for a prover. They are recognised so the module after them can
/// still be read.
#[test]
fn a_proof_is_skipped_and_the_module_continues() {
    let m = module(
        "THEOREM Correct == Spec => []Inv\n\
         <1>1. Init => Inv\n\
           BY DEF Init, Inv\n\
         <1>2. QED\n\
           BY <1>1\n\
         After == 42",
    );
    assert!(
        m.definition("After").is_some(),
        "the definition after the proof is still read"
    );
}

#[test]
fn a_module_may_contain_another() {
    let m = module("---- MODULE Inner ----\nX == 1\n====================\nOuter == 2");
    assert!(m.units.iter().any(|u| matches!(u, Unit::Inner(_))));
    assert!(m.definition("Outer").is_some());
}

#[test]
fn labels_are_read_and_dropped() {
    let body = definition("Step == /\\ Lbl:: x = 1\n        /\\ y = 2", "Step");
    assert_eq!(body.to_string(), "x = 1 /\\ y = 2");
}

#[test]
fn instance_names_may_be_chained_and_take_arguments() {
    let body = definition("Q == A!B!C", "Q");
    let Expr::Qualified { instance, name, .. } = &body else {
        panic!("expected a qualified name, got {body}");
    };
    assert_eq!((instance.as_str(), name.as_str()), ("A!B", "C"));
}

#[test]
fn a_name_may_begin_with_a_digit_or_an_underscore() {
    let m = module("1aMessage == 1\n_hidden == 2");
    assert!(m.definition("1aMessage").is_some());
    assert!(m.definition("_hidden").is_some());
}

/// Everything above must also survive being printed and read back.
#[test]
fn the_new_forms_round_trip() {
    for source in [
        "S^+ \\cup T",
        "CASE n = 1 -> \"a\" [] OTHER -> \"b\"",
        "\\EE t : Timer(t)",
        "WF_<<a, b>>(Next)",
        "{CHOOSE x \\in S : TRUE}",
        "[f EXCEPT ![a, b] = v]",
        "A!B!C",
        "Apply(LAMBDA x, y : x + y, 1)",
        "[][\\A i \\in Proc : p[i] = p'[i]]_vars",
    ] {
        let parsed = tla_syntax::parse_expression(source)
            .unwrap_or_else(|e| panic!("parsing `{source}`: {e}"));
        let printed = parsed.to_string();
        let again = tla_syntax::parse_expression(&printed)
            .unwrap_or_else(|e| panic!("re-reading `{printed}`: {e}"));
        assert_eq!(parsed, again, "`{source}` printed as `{printed}`");
    }
}
