//! Parse a corpus of `.tla` files and group what failed, so the gaps between
//! this parser and the language are a list rather than an impression.
//!
//! ```text
//! cargo run --release --example audit -p tla-syntax -- $(find corpus -name '*.tla')
//! ```

use std::collections::BTreeMap;

struct Failure {
    path: String,
    line: u32,
    source_line: String,
}

fn main() {
    let mut parsed = 0usize;
    let mut by_reason: BTreeMap<String, Vec<Failure>> = BTreeMap::new();

    for path in std::env::args().skip(1) {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        match tla_syntax::parse_module(&src) {
            Ok(_) => parsed += 1,
            Err(e) => {
                let source_line = src
                    .lines()
                    .nth(e.line as usize - 1)
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(96)
                    .collect();
                by_reason.entry(reason(&e)).or_default().push(Failure {
                    path,
                    line: e.line,
                    source_line,
                });
            }
        }
    }

    let total = parsed + by_reason.values().map(Vec::len).sum::<usize>();
    println!("parsed {parsed} / {total}");

    let mut groups: Vec<_> = by_reason.into_iter().collect();
    groups.sort_by_key(|(_, failures)| std::cmp::Reverse(failures.len()));
    for (reason, failures) in groups {
        println!("\n{:>4}  {reason}", failures.len());
        for f in failures.iter().take(4) {
            let file = f.path.rsplit('/').next().unwrap_or(&f.path);
            println!("      {file}:{}  {}", f.line, f.source_line);
        }
    }
}

fn reason(e: &tla_syntax::Error) -> String {
    let body = e.message.split(", found").next().unwrap_or(&e.message);
    body.to_string()
}
