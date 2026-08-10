//! A parser meets input it was not written for: a half-saved file, a paste
//! that lost its tail, a specification being typed. None of that may panic —
//! an error is a result, a panic is a defect.
//!
//! Every prefix and every single-token deletion of every fixture is fed back
//! in. That is a poor substitute for a fuzzer, but unlike a fuzzer it runs on
//! every commit and it is deterministic.

use std::path::Path;

use tla_syntax::{parse_expression, parse_module};

fn fixtures() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs");
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("fixture directory")
        .map(|entry| {
            let path = entry.expect("dir entry").path();
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into();
            (name, std::fs::read_to_string(&path).expect("readable"))
        })
        .collect();
    out.sort();
    out
}

/// Nothing may panic, however the input is cut short.
#[test]
fn every_truncation_of_every_fixture_is_survivable() {
    for (name, src) in fixtures() {
        for cut in 0..src.len() {
            if !src.is_char_boundary(cut) {
                continue;
            }
            let result = std::panic::catch_unwind(|| parse_module(&src[..cut]));
            assert!(result.is_ok(), "{name}: panicked on the first {cut} bytes");
        }
    }
}

/// Nothing may panic when a single line is missing, which is what a bad merge
/// or a mis-edit looks like.
#[test]
fn every_dropped_line_is_survivable() {
    for (name, src) in fixtures() {
        let lines: Vec<&str> = src.lines().collect();
        for skip in 0..lines.len() {
            let damaged: String = lines
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != skip)
                .map(|(_, line)| *line)
                .collect::<Vec<_>>()
                .join("\n");
            let result = std::panic::catch_unwind(|| parse_module(&damaged));
            assert!(result.is_ok(), "{name}: panicked without line {}", skip + 1);
        }
    }
}

/// Unbalanced and nonsensical input reaches a parser more often than valid
/// input does.
#[test]
fn hostile_fragments_are_errors_and_not_panics() {
    for fragment in [
        "",
        " ",
        "\0",
        "----",
        "---- MODULE",
        "---- MODULE M",
        "---- MODULE M ----",
        "---- MODULE M ----\n",
        "====",
        "(*",
        "(* (* *)",
        "\"",
        "\\",
        "[",
        "[[[[[[[[",
        "{{{{{{{{",
        "<<<<<<<<",
        "((((((((",
        "---- MODULE M ----\nX == [\n====",
        "---- MODULE M ----\nX == {a : \n====",
        "---- MODULE M ----\nX == LET\n====",
        "---- MODULE M ----\nX == CHOOSE\n====",
        "---- MODULE M ----\nX == \\A\n====",
        "---- MODULE M ----\nX == [f EXCEPT !\n====",
        "---- MODULE M ----\nEXTENDS\n====",
        "---- MODULE M ----\nVARIABLE\n====",
        "---- MODULE M ----\nX == 99999999999999999999999999\n====",
        "---- MODULE M ----\n/\\ /\\ /\\ /\\\n====",
        "---- MODULE M ----\nX == a!\n====",
        "---- MODULE M ----\nX == WF_\n====",
    ] {
        let owned = fragment.to_string();
        let module = std::panic::catch_unwind(move || parse_module(&owned));
        assert!(module.is_ok(), "parse_module panicked on {fragment:?}");

        let owned = fragment.to_string();
        let expression = std::panic::catch_unwind(move || parse_expression(&owned));
        assert!(
            expression.is_ok(),
            "parse_expression panicked on {fragment:?}"
        );
    }
}

/// Nesting has to be bounded, or a file of open brackets takes the process
/// down with it. Anything within the bound must still parse.
#[test]
fn nesting_is_bounded_rather_than_fatal() {
    let nested = |depth: usize| {
        format!(
            "---- MODULE M ----\nX == {}1{}\n====",
            "(".repeat(depth),
            ")".repeat(depth)
        )
    };
    for depth in [1, 8, 32, 60] {
        assert!(
            parse_module(&nested(depth)).is_ok(),
            "nesting {depth} deep is within the bound and should parse"
        );
    }
    for depth in [200, 5_000] {
        let err = parse_module(&nested(depth)).expect_err("beyond the bound");
        assert!(err.message.contains("nest"), "{err}");
    }
}
