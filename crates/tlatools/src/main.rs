//! ```text
//! tlatools parse FILE...      read each file and say whether it is TLA+
//! tlatools fmt FILE           write a module back out in canonical form
//! tlatools check [JOB.json]   decide a refinement job
//! ```
//!
//! Exit status is the answer, so a caller can branch without parsing: 0 for
//! yes, 1 for no, 2 for a question that could not be asked.

use std::io::{Read, Write};
use std::process::ExitCode;

use tla_oracle::{Job, Status, check};

const OK: u8 = 0;
const NO: u8 = 1;
const UNUSABLE: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("tlatools: {message}");
            ExitCode::from(UNUSABLE)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check_command(&args[1..]),
        Some("parse") => parse_command(&args[1..]),
        Some("fmt") => format_command(&args[1..]),
        Some("--help" | "-h") => {
            println!("{}", usage());
            Ok(ExitCode::SUCCESS)
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{}", usage())),
        None => Err(format!("no command given\n\n{}", usage())),
    }
}

/// Read files and report which are TLA+ and which are not.
///
/// One line per file so the output can be diffed, which is how the corpus
/// golden files are kept.
fn parse_command(args: &[String]) -> Result<ExitCode, String> {
    if args.is_empty() {
        return Err("parse needs at least one file".to_string());
    }
    let mut failures = 0usize;
    let mut out = std::io::stdout().lock();
    for path in args {
        let line = match std::fs::read_to_string(path) {
            Err(e) => {
                failures += 1;
                format!("unreadable\t{e}")
            }
            Ok(src) => match tla_syntax::parse_module(&src) {
                Ok(module) => format!("ok\t{}\t{} units", module.name, module.units.len()),
                Err(e) => {
                    failures += 1;
                    format!("error\t{}:{}\t{}", e.line, e.col, e.message)
                }
            },
        };
        writeln!(out, "{path}\t{line}").map_err(|e| e.to_string())?;
    }
    Ok(ExitCode::from(if failures == 0 { OK } else { NO }))
}

/// Write a module back out in the one canonical form.
fn format_command(args: &[String]) -> Result<ExitCode, String> {
    let [path] = args else {
        return Err("fmt takes exactly one file".to_string());
    };
    let src = read(path)?;
    let module = tla_syntax::parse_module(&src).map_err(|e| format!("{path}:{e}"))?;
    let mut out = std::io::stdout().lock();
    write!(out, "{module}").map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

fn check_command(args: &[String]) -> Result<ExitCode, String> {
    let source = match args.first().map(String::as_str) {
        None | Some("-") => read_stdin()?,
        Some(path) => read(path)?,
    };
    let job: Job = serde_json::from_str(&source).map_err(|e| format!("reading the job: {e}"))?;
    let report = check(&job);
    let rendered =
        serde_json::to_string(&report).map_err(|e| format!("writing the report: {e}"))?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{rendered}").map_err(|e| e.to_string())?;

    Ok(ExitCode::from(match report.status {
        Status::Pass => OK,
        Status::Error => UNUSABLE,
        _ => NO,
    }))
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))
}

fn read_stdin() -> Result<String, String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| format!("reading stdin: {e}"))?;
    Ok(buffer)
}

fn usage() -> &'static str {
    "usage: tlatools <command>

  parse FILE...      read each file and report, one tab-separated line each
  fmt FILE           write a module back out in canonical form
  check [JOB.json]   decide whether a state graph refines a specification;
                     the job is read from stdin when no path is given

exit status
  0  yes: everything parsed, or the implementation refines the specification
  1  no: something did not parse, or the implementation does not refine
  2  the question could not be asked"
}
