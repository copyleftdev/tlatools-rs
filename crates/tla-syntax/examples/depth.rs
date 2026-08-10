//! How deeply do real specifications nest expressions, and how much stack does
//! one level of nesting cost? Both numbers are needed to choose a recursion
//! limit that rejects nothing real and survives a small stack.

use tla_syntax::{Expr, Unit, parse_module};

fn main() {
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

    // How far can the parser actually go on a small stack?
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut survived = 0;
            for probe in 1..=512 {
                // Printed before the attempt: a stack overflow aborts the
                // process, so the last line printed is the answer.
                eprint!("\rprobing {probe}   ");
                let src = format!(
                    "---- MODULE M ----\nX == {}1{}\n====",
                    "(".repeat(probe),
                    ")".repeat(probe)
                );
                if parse_module(&src).is_err() {
                    break;
                }
                survived = probe;
            }
            eprintln!();
            survived
        })
        .expect("thread");
    println!(
        "on a 2 MiB stack the parser reaches {}",
        handle.join().unwrap_or(0)
    );
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
