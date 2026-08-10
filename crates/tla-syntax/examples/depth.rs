//! Where the parser's nesting limit comes from.
//!
//! Two numbers decide it: how deeply real specifications nest, and how much
//! stack one level costs. Both are measured here rather than guessed.
//!
//! ```text
//! cargo run --example depth -p tla-syntax -- $(find CORPUS -name '*.tla')
//! ```
//!
//! Stack exhaustion aborts the process rather than unwinding, so it cannot be
//! caught in-process. Each stack size is therefore tried in a child, and the
//! parent reads the exit status.

use std::process::Command;

use tla_syntax::{Expr, Unit, parse_module};

/// Set in the child to say how much stack to give the probing thread.
const PROBE: &str = "TLA_DEPTH_PROBE_KIB";

fn main() {
    if let Ok(kib) = std::env::var(PROBE) {
        let kib: usize = kib.parse().expect("a number of KiB");
        let survived = std::thread::Builder::new()
            .stack_size(kib * 1024)
            .spawn(probe)
            .expect("thread")
            .join()
            .unwrap_or(false);
        std::process::exit(i32::from(!survived));
    }

    report_corpus_depth();
    report_stack_need();
}

/// Parse an expression nested one past the parser's own limit, so the limit is
/// what stops it rather than the input running out.
fn probe() -> bool {
    let depth = tla_syntax::DEFAULT_NESTING_LIMIT + 44;
    let src = format!(
        "---- MODULE M ----\nX == {}1{}\n====",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    parse_module(&src).is_err()
}

fn report_corpus_depth() {
    let mut deepest = (0usize, String::new());
    let mut files = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(module) = parse_module(&src) else {
            continue;
        };
        files += 1;
        for unit in &module.units {
            if let Unit::Def(def) = unit {
                let d = depth(&def.body);
                if d > deepest.0 {
                    deepest = (d, format!("{}: {}", module.name, def.name));
                }
            }
        }
    }
    println!(
        "{files} modules; deepest expression nests {} ({})",
        deepest.0, deepest.1
    );
}

fn report_stack_need() {
    let profile = if cfg!(debug_assertions) {
        "unoptimised"
    } else {
        "optimised"
    };
    let me = std::env::current_exe().expect("own path");
    for kib in [
        128usize, 192, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 5120, 6144, 7168, 8192,
        12288, 16384,
    ] {
        let status = Command::new(&me)
            .env(PROBE, kib.to_string())
            .status()
            .expect("the child runs");
        if status.success() {
            println!("{profile}: reaching the nesting limit needs {kib} KiB of stack");
            return;
        }
    }
    println!("{profile}: reaching the nesting limit needs more than 32 MiB of stack");
}

fn depth(e: &Expr) -> usize {
    1 + children(e).iter().map(|c| depth(c)).max().unwrap_or(0)
}

fn children(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Prime(x) | Expr::Field(x, _) | Expr::Unary(_, x) => vec![x],
        Expr::Binary(_, a, b)
        | Expr::FnSet {
            domain: a,
            range: b,
        } => vec![a, b],
        Expr::Apply(h, args) | Expr::FnApply(h, args) => {
            let mut v = vec![&**h];
            v.extend(args);
            v
        }
        Expr::Tuple(xs) | Expr::SetEnum(xs) => xs.iter().collect(),
        Expr::Record(fs) | Expr::RecordSet(fs) => fs.iter().map(|(_, v)| v).collect(),
        Expr::SetFilter { pred, .. } => vec![pred],
        Expr::SetMap { expr, .. } => vec![expr],
        Expr::FnDef { body, .. }
        | Expr::Quant { body, .. }
        | Expr::Choose { body, .. }
        | Expr::Lambda { body, .. }
        | Expr::Let { body, .. } => vec![body],
        Expr::Except { base, updates } => {
            let mut v = vec![&**base];
            v.extend(updates.iter().map(|(_, e)| e));
            v
        }
        Expr::If {
            cond,
            then,
            otherwise,
        } => vec![cond, then, otherwise],
        Expr::Case { arms, .. } => arms.iter().flat_map(|(g, r)| [g, r]).collect(),
        Expr::ActionBox { action, .. }
        | Expr::ActionAngle { action, .. }
        | Expr::Fairness { action, .. } => vec![action],
        Expr::Qualified { args, .. } => args.iter().collect(),
        _ => Vec::new(),
    }
}
