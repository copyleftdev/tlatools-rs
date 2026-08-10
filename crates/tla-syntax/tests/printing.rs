//! The printer is what a counterexample quotes, so it has to say what it read.
//! Every operator and every shape of expression is written out and read back;
//! anything that does not survive means the quoted guard would be a lie.

use tla_syntax::parse_expression;

#[track_caller]
fn round_trip(source: &str) -> String {
    let parsed = parse_expression(source).unwrap_or_else(|e| panic!("parsing `{source}`: {e}"));
    let printed = parsed.to_string();
    let again = parse_expression(&printed)
        .unwrap_or_else(|e| panic!("`{source}` printed as `{printed}`, which failed: {e}"));
    assert_eq!(parsed, again, "`{source}` printed as `{printed}`");
    printed
}

#[test]
fn every_infix_operator_survives_printing() {
    for source in [
        "a => b",
        "a <=> b",
        "a \\/ b",
        "a /\\ b",
        "a = b",
        "a # b",
        "a < b",
        "a > b",
        "a <= b",
        "a >= b",
        "a \\in b",
        "a \\notin b",
        "a \\subseteq b",
        "a \\supseteq b",
        "a @@ b",
        "a :> b",
        "a \\cup b",
        "a \\cap b",
        "a \\ b",
        "a .. b",
        "a + b",
        "a - b",
        "a * b",
        "a \\div b",
        "a % b",
        "a \\X b",
        "a \\o b",
        "a ^ b",
        "a ~> b",
        "a \\prec b",
        "a \\oplus b",
        "a & b",
        "a | b",
        "a $ b",
        "a <: b",
        "a =| b",
        "a |- b",
        "a -| b",
        "a |= b",
        "a || b",
        "a && b",
        "a ## b",
        "a // b",
        "a ** b",
        "a \\cdot b",
        "a \\sqcap b",
        "a (+) b",
        "a (\\X) b",
    ] {
        round_trip(source);
    }
}

#[test]
fn every_prefix_and_postfix_operator_survives_printing() {
    for source in [
        "~a",
        "-a",
        "DOMAIN f",
        "SUBSET S",
        "UNION S",
        "ENABLED A",
        "UNCHANGED v",
        "[]P",
        "<>P",
        "a^+",
        "a^*",
        "a^#",
    ] {
        round_trip(source);
    }
}

/// A word-shaped prefix operator needs a space after it or it runs into its
/// operand; a symbolic one must not gain one.
#[test]
fn word_operators_keep_their_space() {
    assert_eq!(round_trip("DOMAIN f"), "DOMAIN f");
    assert_eq!(round_trip("SUBSET S"), "SUBSET S");
    assert_eq!(round_trip("UNION S"), "UNION S");
    assert_eq!(round_trip("ENABLED A"), "ENABLED A");
    assert_eq!(round_trip("UNCHANGED v"), "UNCHANGED v");
    assert_eq!(round_trip("~a"), "~a");
    assert_eq!(round_trip("-a"), "-a");
}

#[test]
fn every_shape_of_expression_survives_printing() {
    for source in [
        "1",
        "-1",
        "TRUE",
        "FALSE",
        r#""text""#,
        "name",
        "x'",
        "@",
        "Op(a, b)",
        "f[a]",
        "f[a, b]",
        "r.field",
        "I!Name",
        "I!Name(a)",
        "I!J!K",
        "<<a, b>>",
        "<<>>",
        "{a, b}",
        "{}",
        "{x \\in S : P}",
        "{<<x, y>> \\in S : P}",
        "{e : x \\in S}",
        "{e : x \\in S, y \\in T}",
        "[a |-> 1, b |-> 2]",
        "[a : S, b : T]",
        "[x \\in S |-> e]",
        "[x, y \\in S |-> e]",
        "[S -> T]",
        "[f EXCEPT ![a] = 1]",
        "[f EXCEPT ![a] = 1, !.b = 2]",
        "[f EXCEPT ![a][b] = 1]",
        "[f EXCEPT ![a] = @ + 1]",
        "\\A x \\in S : P",
        "\\E x \\in S : P",
        "\\A x, y \\in S : P",
        "\\A x \\in S, y \\in T : P",
        "\\AA x : P",
        "\\EE x : P",
        "CHOOSE x \\in S : P",
        "LET a == 1 IN a",
        "LET a == 1 b == 2 IN a + b",
        "LET f(x) == x IN f(1)",
        "IF c THEN 1 ELSE 2",
        "CASE a -> 1 [] b -> 2",
        "CASE a -> 1 [] OTHER -> 2",
        "LAMBDA x : x",
        "LAMBDA x, y : x",
        "[A]_v",
        "[A]_<<a, b>>",
        "<<A>>_v",
        "WF_v(A)",
        "SF_v(A)",
        "WF_<<a, b>>(A)",
    ] {
        round_trip(source);
    }
}

/// Parentheses are placed by precedence rather than remembered, so the ones
/// that matter have to be put back and the ones that do not have to go.
#[test]
fn parentheses_are_restored_exactly_where_they_are_needed() {
    assert_eq!(round_trip("(a + b) * c"), "(a + b) * c");
    assert_eq!(round_trip("a + (b * c)"), "a + b * c");
    assert_eq!(round_trip("a + b + c"), "a + b + c");
    assert_eq!(round_trip("a - (b - c)"), "a - (b - c)");
    assert_eq!(round_trip("(a => b) => c"), "(a => b) => c");
    assert_eq!(round_trip("a => (b => c)"), "a => b => c");
    // A construct that runs to the end of the expression is always wrapped.
    assert_eq!(
        round_trip("(\\A x \\in S : P) /\\ Q"),
        "(\\A x \\in S : P) /\\ Q"
    );
    assert_eq!(
        round_trip("(IF c THEN 1 ELSE 2) + 3"),
        "(IF c THEN 1 ELSE 2) + 3"
    );
    // `[]` binds tighter than `/\`, so it needs no parentheses.
    assert_eq!(round_trip("[]P /\\ Q"), "[]P /\\ Q");
    assert_eq!(round_trip("[](P /\\ Q)"), "[](P /\\ Q)");
}

#[test]
fn strings_are_escaped_so_they_can_be_read_back() {
    assert_eq!(round_trip(r#""a\"b""#), r#""a\"b""#);
    assert_eq!(round_trip(r#""a\\b""#), r#""a\\b""#);
}

#[test]
fn a_definition_prints_with_its_parameters() {
    let module = tla_syntax::parse_module(
        "---- MODULE T ----\nFold(op(_, _), base) == base\nPlain == 1\n====",
    )
    .expect("parses");
    assert_eq!(
        module.definition("Fold").expect("defined").to_string(),
        "Fold(op(_, _), base) == base"
    );
    assert_eq!(
        module.definition("Plain").expect("defined").to_string(),
        "Plain == 1"
    );
}
