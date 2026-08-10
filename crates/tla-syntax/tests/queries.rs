//! Questions asked of the tree rather than of the text. `mentions_next_state`
//! is what separates an action's guard from its effect in a counterexample, so
//! it is answered for every shape an expression can take.

use tla_syntax::{QuantKind, parse_expression};

fn constrains(source: &str) -> bool {
    parse_expression(source)
        .unwrap_or_else(|e| panic!("parsing `{source}`: {e}"))
        .mentions_next_state()
}

#[test]
fn a_guard_does_not_constrain_the_successor_state() {
    for source in [
        "x",
        "1",
        "TRUE",
        r#""s""#,
        "x < Limit",
        "f[x]",
        "r.field",
        "Op(x, y)",
        "-x",
        "<<x, y>>",
        "{x, y}",
        "{y \\in S : y > x}",
        "{y * 2 : y \\in S}",
        "[a |-> x]",
        "[a : S]",
        "[y \\in S |-> y]",
        "[S -> T]",
        "[f EXCEPT ![k] = v]",
        "\\A y \\in S : y > x",
        "CHOOSE y \\in S : y > x",
        "LET k == x IN k + 1",
        "IF x > 0 THEN 1 ELSE 2",
        "CASE x = 1 -> 2 [] OTHER -> 3",
        "LAMBDA k : k + x",
        "I!Op(x)",
    ] {
        assert!(!constrains(source), "`{source}` mentions no next state");
    }
}

#[test]
fn an_effect_constrains_the_successor_state() {
    for source in [
        "x'",
        "x' = x + 1",
        "x = x'",
        "f[x']",
        "f'[x]",
        "r'.field",
        "Op(x')",
        "-x'",
        "<<x'>>",
        "{x'}",
        "{y \\in S : y = x'}",
        "{y : y \\in S'}",
        "[a |-> x']",
        "[y \\in S' |-> y]",
        "[S' -> T]",
        "[S -> T']",
        "[f EXCEPT ![k] = v']",
        "[f EXCEPT ![k'] = v]",
        "[f' EXCEPT ![k] = v]",
        "\\A y \\in S : y = x'",
        "\\A y \\in S' : y = 1",
        "CHOOSE y \\in S' : TRUE",
        "LET k == x' IN k",
        "LET k == 1 IN k = x'",
        "IF x' > 0 THEN 1 ELSE 2",
        "IF c THEN x' ELSE 2",
        "IF c THEN 1 ELSE x'",
        "CASE x' = 1 -> 2",
        "CASE c -> x'",
        "CASE c -> 1 [] OTHER -> x'",
        "LAMBDA k : k = x'",
        "I!Op(x')",
        "UNCHANGED x",
        "UNCHANGED <<a, b>>",
        "ENABLED A",
        "[A]_v",
        "<<A>>_v",
        "WF_v(A)",
    ] {
        assert!(constrains(source), "`{source}` constrains the next state");
    }
}

#[test]
fn temporal_quantifiers_are_marked_and_ordinary_ones_are_not() {
    assert!(QuantKind::TemporalForall.is_temporal());
    assert!(QuantKind::TemporalExists.is_temporal());
    assert!(!QuantKind::Forall.is_temporal());
    assert!(!QuantKind::Exists.is_temporal());
}
