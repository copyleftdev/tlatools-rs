//! Render an expression back to TLA+ source.
//!
//! Written so the parser can read the result: parentheses are placed by
//! precedence rather than preserved from the input, and where TLA+ offers
//! several spellings of an operator the ASCII one is used. A round-trip
//! through `parse_expression` gives back the same tree.

use std::fmt;

use crate::ast::{Bound, Def, ExceptPath, Expr, QuantKind};
use crate::token::Op;

/// The binding power of the constructs that run to the end of the expression.
/// They are parenthesized inside any operator, which is what makes
/// `(\A x \in S : P) /\ Q` come back as itself.
const TRAILING: u8 = 0;
const ATOM: u8 = u8::MAX;

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write(self, TRAILING, f)
    }
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.destructure {
            write!(f, "<<{}>>", self.names.join(", "))?;
        } else {
            f.write_str(&self.names.join(", "))?;
        }
        match &self.domain {
            Some(domain) => write!(f, " \\in {domain}"),
            None => Ok(()),
        }
    }
}

fn precedence(e: &Expr) -> u8 {
    match e {
        Expr::Binary(op, ..) => op.infix_prec().unwrap_or(TRAILING),
        Expr::Unary(op, _) => op.prefix_prec().saturating_sub(1),
        Expr::Quant { .. }
        | Expr::Choose { .. }
        | Expr::Let { .. }
        | Expr::If { .. }
        | Expr::Case { .. } => TRAILING,
        _ => ATOM,
    }
}

fn write(e: &Expr, min: u8, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if precedence(e) < min {
        write!(f, "(")?;
        write(e, TRAILING, f)?;
        return write!(f, ")");
    }
    bare(e, f)
}

#[expect(
    clippy::too_many_lines,
    reason = "one arm per syntactic form; splitting it would only scatter the grammar"
)]
fn bare(e: &Expr, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match e {
        Expr::Num(n) => write!(f, "{n}"),
        Expr::Str(s) => write!(f, "\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Expr::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
        Expr::Ident(name) => f.write_str(name),
        Expr::At => f.write_str("@"),
        Expr::Prime(inner) => {
            write(inner, ATOM, f)?;
            f.write_str("'")
        }
        Expr::Apply(head, args) => {
            write(head, ATOM, f)?;
            write!(f, "({})", list(args))
        }
        Expr::FnApply(head, args) => {
            write(head, ATOM, f)?;
            write!(f, "[{}]", list(args))
        }
        Expr::Field(inner, name) => {
            write(inner, ATOM, f)?;
            write!(f, ".{name}")
        }
        Expr::Qualified {
            instance,
            name,
            args,
        } => {
            write!(f, "{instance}!{name}")?;
            if args.is_empty() {
                Ok(())
            } else {
                write!(f, "({})", list(args))
            }
        }
        Expr::Unary(op, operand) => {
            f.write_str(op.symbol())?;
            if op.is_word() {
                f.write_str(" ")?;
            }
            write(operand, op.prefix_prec(), f)
        }
        Expr::Binary(op, lhs, rhs) => {
            let prec = op.infix_prec().unwrap_or(TRAILING);
            let (left, right) = if op.is_right_assoc() {
                (prec + 1, prec)
            } else {
                (prec, prec + 1)
            };
            write(lhs, left, f)?;
            write!(f, " {} ", op.symbol())?;
            write(rhs, right, f)
        }
        Expr::Tuple(items) => write!(f, "<<{}>>", list(items)),
        Expr::SetEnum(items) => write!(f, "{{{}}}", list(items)),
        Expr::SetFilter { bound, pred } => write!(f, "{{{bound} : {pred}}}"),
        Expr::SetMap { expr, bounds } => write!(f, "{{{expr} : {}}}", bounds_list(bounds)),
        Expr::Record(fields) => write!(f, "[{}]", fields_list(fields, "|->")),
        Expr::RecordSet(fields) => write!(f, "[{}]", fields_list(fields, ":")),
        Expr::FnDef { bounds, body } => {
            write!(f, "[{} |-> {body}]", bounds_list(bounds))
        }
        Expr::FnSet { domain, range } => write!(f, "[{domain} -> {range}]"),
        Expr::Except { base, updates } => {
            write!(f, "[{base} EXCEPT ")?;
            for (i, (path, value)) in updates.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                f.write_str("!")?;
                for step in path {
                    match step {
                        ExceptPath::Index(index) => write!(f, "[{index}]")?,
                        ExceptPath::Field(name) => write!(f, ".{name}")?,
                    }
                }
                write!(f, " = {value}")?;
            }
            f.write_str("]")
        }
        Expr::Quant { kind, bounds, body } => {
            let symbol = match kind {
                QuantKind::Forall => Op::Forall,
                QuantKind::Exists => Op::Exists,
            };
            write!(f, "{} {} : {body}", symbol.symbol(), bounds_list(bounds))
        }
        Expr::Choose { bound, body } => write!(f, "CHOOSE {bound} : {body}"),
        Expr::Let { defs, body } => {
            f.write_str("LET ")?;
            for (i, def) in defs.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{def}")?;
            }
            write!(f, " IN {body}")
        }
        Expr::If {
            cond,
            then,
            otherwise,
        } => write!(f, "IF {cond} THEN {then} ELSE {otherwise}"),
        Expr::Case { arms, other } => {
            f.write_str("CASE ")?;
            for (i, (guard, result)) in arms.iter().enumerate() {
                if i > 0 {
                    f.write_str(" \\/ ")?;
                }
                write!(f, "{guard} -> {result}")?;
            }
            match other {
                Some(value) => write!(f, " \\/ OTHER -> {value}"),
                None => Ok(()),
            }
        }
        Expr::ActionBox { action, subscript } => write!(f, "[{action}]_{subscript}"),
        Expr::ActionAngle { action, subscript } => write!(f, "<<{action}>>_{subscript}"),
        Expr::Fairness {
            strong,
            subscript,
            action,
        } => {
            let kind = if *strong { "SF" } else { "WF" };
            write!(f, "{kind}_{subscript}({action})")
        }
    }
}

impl fmt::Display for Def {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.params.is_empty() {
            write!(f, "({})", self.params.join(", "))?;
        }
        write!(f, " == {}", self.body)
    }
}

fn list(items: &[Expr]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn bounds_list(bounds: &[Bound]) -> String {
    bounds
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn fields_list(fields: &[(String, Expr)], separator: &str) -> String {
    fields
        .iter()
        .map(|(name, value)| format!("{name} {separator} {value}"))
        .collect::<Vec<_>>()
        .join(", ")
}
