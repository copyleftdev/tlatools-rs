//! Where one definition stops and the next begins.
//!
//! This is the part of reading TLA+ that has no punctuation to guide it: a
//! definition ends when the next one starts, and a generated specification
//! will happily wrap an expression into the first column. Each case below is a
//! pair — the same first column, once continuing an expression and once
//! starting a definition.

use tla_syntax::{Expr, LetInstance, Unit, parse_expression, parse_module};

/// Parse `X == a` followed by `rest`, and report what `X` came out as. If the
/// boundary is misplaced, `X` swallows what follows, or the parse fails.
#[track_caller]
fn after(rest: &str) -> String {
    let src = format!("---- MODULE T ----\nX == a\n{rest}\n====");
    let module = parse_module(&src).unwrap_or_else(|e| panic!("{e}\nin:\n{src}"));
    module
        .definition("X")
        .unwrap_or_else(|| panic!("X is gone\nin:\n{src}"))
        .body
        .to_string()
}

#[track_caller]
fn defines(rest: &str, name: &str) -> bool {
    let src = format!("---- MODULE T ----\nX == a\n{rest}\n====");
    parse_module(&src).is_ok_and(|m| m.definition(name).is_some())
}

/// Every shape a definition's left-hand side can take ends the one before it.
#[test]
fn a_definition_of_any_shape_ends_the_previous_one() {
    for (rest, name) in [
        ("Y == 1", "Y"),
        ("Y(p) == p", "Y"),
        ("Y(p, q) == p", "Y"),
        ("Y(f(_)) == 1", "Y"),
        ("Y[i \\in S] == i", "Y"),
        ("Y[i, j \\in S] == i", "Y"),
        ("p ++ q == p", "++"),
        ("p ^+ == p", "^+"),
        ("-. p == p", "-"),
    ] {
        assert_eq!(after(rest), "a", "`{rest}` should end X");
        assert!(defines(rest, name), "`{rest}` should define {name}");
    }
}

#[test]
fn a_declaration_ends_the_definition_before_it() {
    for rest in ["VARIABLE v", "CONSTANT c", "THEOREM TRUE", "ASSUME TRUE"] {
        assert_eq!(after(rest), "a", "`{rest}` should end X");
    }
}

/// The same first column, without a definition's shape, is the expression
/// carrying on.
#[test]
fn an_expression_wrapped_into_the_first_column_carries_on() {
    for (rest, expected) in [
        ("\\/ b", "a \\/ b"),
        ("+ b", "a + b"),
        ("\\cup b", "a \\cup b"),
        ("= b", "a = b"),
    ] {
        assert_eq!(after(rest), expected, "`{rest}` continues the expression");
    }
}

/// A `==` belonging to the definition *after* next must not be mistaken for
/// this one's — which is what a "look a few tokens ahead" rule would do.
#[test]
fn a_later_definitions_equals_does_not_end_this_one() {
    let src = "---- MODULE T ----\nX == a\n\\/ b\n\\/ c\nY == 2\n====";
    let m = parse_module(src).expect("parses");
    assert_eq!(
        m.definition("X").expect("X").body.to_string(),
        "a \\/ b \\/ c"
    );
    assert_eq!(m.definition("Y").expect("Y").body.to_string(), "2");
}

/// Brackets on the left-hand side are skipped as a whole, however they nest.
#[test]
fn a_bracketed_left_hand_side_is_found_past_its_brackets() {
    for (rest, name) in [
        ("Y[i \\in [j \\in S |-> j]] == i", "Y"),
        ("Y(p, f(_), q) == p", "Y"),
    ] {
        assert_eq!(after(rest), "a", "`{rest}`");
        assert!(defines(rest, name), "`{rest}`");
    }
    // Brackets with no `==` after them are an application, not a definition.
    assert_eq!(after("(b)"), "a(b)");
}

/// TLA+ writes an operator's *declaration* with holes (`CONSTANT _+_`) and its
/// *definition* with operands (`a + b == ...`). The two are not
/// interchangeable, and SANY rejects the mixture, so this does too.
#[test]
fn a_definition_is_written_with_operands_not_holes() {
    for source in [
        "_ ** _ == 1",
        "_ ^# == 1",
        "Y(f(g(_))) == 1",
        "Y(<<1, 2>>) == 1",
    ] {
        let src = format!("---- MODULE T ----\n{source}\n====");
        assert!(
            parse_module(&src).is_err(),
            "`{source}` is not TLA+ and SANY rejects it too"
        );
    }
}

/// `-. _` declares the prefix operator and `-. a` defines it; a bare `-` with
/// neither is a subtraction.
#[test]
fn a_prefix_definition_needs_a_name_or_a_hole() {
    assert!(defines("-. a == 0 - a", "-"));
    assert!(defines("- a == 0 - a", "-"));
    assert_eq!(after("- b"), "a - b", "with no `==` it is a subtraction");
}

// ------------------------------------------------ operators as substitutions

/// `F <- '` substitutes the prime operator; `F <- a + b` substitutes an
/// expression. What comes after the symbol is what tells them apart.
#[test]
fn a_substitution_may_be_an_operator_or_an_expression() {
    let m = parse_module(
        "---- MODULE T ----\nI == INSTANCE M WITH F <- ', G <- ENABLED, H <- a + b\n====",
    )
    .expect("parses");
    let Unit::Instance { subs, .. } = &m.units[0] else {
        panic!("expected an instance");
    };
    assert_eq!(subs[0].1, Expr::Ident("'".to_string()));
    assert_eq!(subs[1].1, Expr::Ident("ENABLED".to_string()));
    assert!(matches!(subs[2].1, Expr::Binary(..)), "{}", subs[2].1);
}

#[test]
fn an_operator_may_be_the_last_argument() {
    let body = parse_module("---- MODULE T ----\nX == apply(v, +)\n====")
        .expect("parses")
        .definition("X")
        .expect("X")
        .body
        .clone();
    let Expr::Apply(_, args) = &body else {
        panic!("expected an application, got {body}");
    };
    assert_eq!(args[1], Expr::Ident("+".to_string()));
}

// -------------------------------------------------- definitions named oddly

#[test]
fn a_definition_may_be_named_after_a_keyword() {
    for (source, name) in [
        ("TRUE == FALSE", "TRUE"),
        ("DOMAIN == 1", "DOMAIN"),
        ("SUBSET == 2", "SUBSET"),
        ("UNCHANGED == 3", "UNCHANGED"),
        ("LAMBDA == 4", "LAMBDA"),
    ] {
        let src = format!("---- MODULE T ----\n{source}\n====");
        let m = parse_module(&src).unwrap_or_else(|e| panic!("{source}: {e}"));
        assert!(m.definition(name).is_some(), "{source} defines {name}");
    }
    // A keyword that is not a definition is still a keyword.
    assert!(
        parse_module("---- MODULE T ----\nTRUE\n====").is_err(),
        "a bare keyword is not a unit"
    );
}

#[test]
fn a_let_instance_survives_printing() {
    for source in [
        "LET I == INSTANCE M IN I!Op",
        "LET I == INSTANCE M WITH a <- 1, b <- 2 IN I!Op",
        "LET INSTANCE M IN Op",
        "LET d == 1 I == INSTANCE M IN I!Op",
    ] {
        let parsed = parse_expression(source).unwrap_or_else(|e| panic!("parsing `{source}`: {e}"));
        let printed = parsed.to_string();
        let again =
            parse_expression(&printed).unwrap_or_else(|e| panic!("re-reading `{printed}`: {e}"));
        assert_eq!(parsed, again, "`{source}` printed as `{printed}`");
        assert!(printed.contains("INSTANCE M"), "{printed}");
    }
}

#[test]
fn a_let_instance_keeps_its_substitutions() {
    let Expr::Let { instances, .. } =
        parse_expression("LET I == INSTANCE M WITH a <- 1 IN I!Op").expect("parses")
    else {
        panic!("expected a LET");
    };
    let LetInstance { name, module, subs } = &instances[0];
    assert_eq!(name.as_deref(), Some("I"));
    assert_eq!(module, "M");
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].0, "a");
}
