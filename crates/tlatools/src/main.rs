//! Read a refinement job as JSON, write the verdict as JSON.
//!
//! ```text
//! tlatools check [job.json]     # or the job on stdin
//! ```
//!
//! Exit status is the verdict, so a caller can branch without parsing:
//! 0 the implementation refines the specification, 1 it does not, 2 the job
//! could not be carried out.

use std::io::{Read, Write};
use std::process::ExitCode;

use tla_oracle::{Job, Status, check};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("tlatools: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = match args.first().map(String::as_str) {
        Some("check") => &args[1..],
        Some("--help" | "-h") => {
            println!("{}", usage());
            return Ok(ExitCode::SUCCESS);
        }
        Some(other) => return Err(format!("unknown command `{other}`\n\n{}", usage())),
        None => return Err(format!("no command given\n\n{}", usage())),
    };

    let source = match rest.first().map(String::as_str) {
        None | Some("-") => read_stdin()?,
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?,
    };

    let job: Job = serde_json::from_str(&source).map_err(|e| format!("reading the job: {e}"))?;
    let report = check(&job);
    let rendered =
        serde_json::to_string(&report).map_err(|e| format!("writing the report: {e}"))?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{rendered}").map_err(|e| e.to_string())?;

    Ok(match report.status {
        Status::Pass => ExitCode::SUCCESS,
        Status::Error => ExitCode::from(2),
        _ => ExitCode::from(1),
    })
}

fn read_stdin() -> Result<String, String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| format!("reading stdin: {e}"))?;
    Ok(buffer)
}

fn usage() -> &'static str {
    "usage: tlatools check [JOB.json]

Decide whether an implementation's reachable state graph refines a TLA+
specification. The job is read from JOB.json, or from stdin when the path is
omitted or is `-`. The verdict is written to stdout as JSON.

exit status
  0  the implementation refines the specification
  1  it does not
  2  the job could not be carried out"
}
