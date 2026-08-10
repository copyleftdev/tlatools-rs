//! A parser meets input it was not written for: a half-saved file, a paste
//! that lost its tail, a specification being typed. None of that may panic —
//! an error is a result, a panic is a defect.
//!
//! Every prefix and every single-token deletion of every fixture is fed back
//! in. That is a poor substitute for a fuzzer, but unlike a fuzzer it runs on
//! every commit and it is deterministic.

use std::path::Path;

use tla_syntax::{DEFAULT_NESTING_LIMIT, parse_expression, parse_module, parse_module_bounded};

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

fn nested(depth: usize) -> String {
    format!(
        "---- MODULE M ----\nX == {}1{}\n====",
        "(".repeat(depth),
        ")".repeat(depth)
    )
}

/// Nesting has to be bounded, or a file of open brackets takes the process
/// down with it. Anything within the bound must still parse.
///
/// Run on a thread with a stack chosen from the measurement in
/// `examples/depth.rs`: the default limit costs about 5 MiB unoptimised, and
/// tests are unoptimised, so the 2 MiB a spawned thread is given by default
/// would not do. That the caller has to think about this is the reason
/// `parse_module_bounded` exists.
#[test]
fn nesting_is_bounded_rather_than_fatal() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            for depth in [1, 8, 32, DEFAULT_NESTING_LIMIT - 4] {
                assert!(
                    parse_module(&nested(depth)).is_ok(),
                    "nesting {depth} deep is within the bound and should parse"
                );
            }
            for depth in [DEFAULT_NESTING_LIMIT + 1, 20_000] {
                let err = parse_module(&nested(depth)).expect_err("beyond the bound");
                assert!(err.message.contains("nest"), "{err}");
            }
        })
        .expect("thread")
        .join()
        .expect("the parser does not exhaust a stack sized for it");
}

/// A caller with less stack than the default assumes can ask for less, and
/// gets an error where it would otherwise have run out.
#[test]
fn the_nesting_limit_can_be_lowered() {
    assert!(parse_module_bounded(&nested(8), 16).is_ok());
    let err = parse_module_bounded(&nested(20), 16).expect_err("beyond the given bound");
    assert!(err.message.contains("16"), "the limit is named: {err}");
    // A deeply nested file is refused without the stack ever being at risk.
    assert!(parse_module_bounded(&nested(100_000), 16).is_err());
}
