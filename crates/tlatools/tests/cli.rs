//! The command is the contract for anyone not writing Rust, so it is exercised
//! as a command: real process, real stdin, real exit status.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn tlatools() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tlatools"))
}

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = tlatools()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write");
    child.wait_with_output().expect("output")
}

fn job(states: &str, edges: &str) -> String {
    let spec = "---- MODULE Counter ----\n\
                EXTENDS Naturals\n\
                CONSTANT Limit\n\
                VARIABLE n\n\
                Init == n = 0\n\
                Next == n < Limit /\\ n' = n + 1\n\
                ====";
    format!(
        r#"{{"spec": {spec}, "init_op": "Init", "next_op": "Next",
            "constants": {{"Limit": {{"value": 3, "schema": {{"kind": "int"}}}}}},
            "schema": {{"n": {{"kind": "int"}}}},
            "states": {states}, "edges": {edges}}}"#,
        spec = serde_json::to_string(spec).expect("encodes"),
    )
}

fn verdict(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout was not JSON: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn a_refining_implementation_exits_zero() {
    let out = run(
        &["check", "-"],
        &job(r#"[{"n": 0}, {"n": 1}]"#, r#"[[0,1,"inc"]]"#),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(verdict(&out)["status"], "pass");
}

#[test]
fn a_failing_implementation_exits_one() {
    let out = run(
        &["check", "-"],
        &job(r#"[{"n": 0}, {"n": 5}]"#, r#"[[0,1,"jump"]]"#),
    );
    assert_eq!(out.status.code(), Some(1));
    let report = verdict(&out);
    assert_eq!(report["status"], "refines");
    assert_eq!(report["edge"]["label"], "jump");
}

/// A job that cannot be carried out is not the implementation's failure, and
/// the exit status keeps them apart.
#[test]
fn an_impossible_job_exits_two() {
    let out = run(
        &["check", "-"],
        r#"{"spec": "not a module", "schema": {}, "states": [{}]}"#,
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(verdict(&out)["status"], "error");
}

#[test]
fn the_job_may_come_from_a_file() {
    let dir = std::env::temp_dir().join("tlatools-cli-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("job.json");
    std::fs::write(&path, job(r#"[{"n": 0}]"#, "[]")).expect("write");

    let out = tlatools()
        .args(["check", path.to_str().expect("utf-8")])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(verdict(&out)["status"], "pass");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_missing_file_is_reported_and_is_not_a_verdict() {
    let out = tlatools()
        .args(["check", "/nonexistent/job.json"])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("/nonexistent/job.json"),
        "the path is named"
    );
}

#[test]
fn help_is_available_and_succeeds() {
    let out = tlatools().arg("--help").output().expect("runs");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    for command in ["parse", "fmt", "check"] {
        assert!(text.contains(command), "`{command}` is documented: {text}");
    }
    assert!(text.contains("exit status"), "{text}");
}

#[test]
fn an_unknown_command_is_refused_with_usage() {
    let out = tlatools().arg("frobnicate").output().expect("runs");
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("frobnicate"), "{text}");
    assert!(text.contains("usage"), "{text}");
}

#[test]
fn no_command_is_refused_with_usage() {
    let out = tlatools().output().expect("runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
}

#[test]
fn malformed_json_is_refused_before_anything_is_decided() {
    let out = run(&["check", "-"], "{not json");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("reading the job"));
}

// ------------------------------------------------------------------- files

fn spec_path(name: &str) -> String {
    format!("{}/../../specs/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn parse_reports_one_line_per_file_and_exits_zero() {
    let out = tlatools()
        .args([
            "parse",
            &spec_path("BoundedBuffer.tla"),
            &spec_path("Paxos.tla"),
        ])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.contains("\tok\tBoundedBuffer\t"), "{text}");
    assert!(text.contains("\tok\tPaxos\t"), "{text}");
}

#[test]
fn parse_reports_a_file_it_cannot_read_and_exits_one() {
    let out = tlatools()
        .args(["parse", &spec_path("Paxos.tla"), "/nonexistent.tla"])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(1), "one file failed");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("\tok\tPaxos\t"),
        "the good file still reports"
    );
    assert!(text.contains("unreadable"), "{text}");
}

#[test]
fn parse_reports_where_a_file_stops_making_sense() {
    let dir = std::env::temp_dir().join("tlatools-parse-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("Broken.tla");
    std::fs::write(&path, "---- MODULE Broken ----\nA == 1\nB == [\n====").expect("write");

    let out = tlatools()
        .args(["parse", path.to_str().expect("utf-8")])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("\terror\t"), "{text}");
    // Where exactly a truncated expression is noticed is the parser's call;
    // that a position is reported at all is the command's.
    let position = text.split('\t').nth(2).unwrap_or_default();
    assert!(
        position
            .split_once(':')
            .is_some_and(|(line, _)| line.parse::<u32>().is_ok()),
        "a line and column are reported: {text}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn fmt_writes_a_module_that_can_be_read_back() {
    let out = tlatools()
        .args(["fmt", &spec_path("BoundedBuffer.tla")])
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(0));
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(
        printed.starts_with("---- MODULE BoundedBuffer ----"),
        "{printed}"
    );

    // The real check: what it wrote is TLA+ this tool reads the same way.
    let first = tla_syntax::parse_module(&printed).expect("the output parses");
    let second = tla_syntax::parse_module(&first.to_string()).expect("and again");
    assert_eq!(first, second);
}

#[test]
fn fmt_takes_exactly_one_file() {
    let out = tlatools().arg("fmt").output().expect("runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("exactly one"));
}
