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
    assert!(text.contains("tlatools check"), "{text}");
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
