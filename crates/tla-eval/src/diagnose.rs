//! Why a step is not allowed.
//!
//! Rejecting a transition is easy; saying what the implementation was *trying*
//! to do is the useful part. A `Next` is a disjunction of actions, and an
//! action is a conjunction of a guard and an effect, so the interesting fact
//! about a rejected step is which action came closest and which of its
//! conjuncts stopped it.
//!
//! A model checker cannot report this, because it never evaluates the
//! specification at the offending pair of states — it searches for the pair and
//! fails to find it. Evaluating gets the answer for free.

use std::collections::BTreeMap;

use tla_syntax::token::Op;
use tla_syntax::{Def, Expr, QuantKind};

use crate::error::Result;
use crate::eval::{Ctx, Evaluator, Local, State, push};
use crate::value::Value;

/// How close one action came to permitting the step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    /// The action as it was reached, with arguments at the values they took:
    /// `Timeout(s = "s1")`.
    pub action: String,
    /// How many of the action's conjuncts hold. Counted over all of them, not
    /// just up to the first failure: an action wanting one guard it does not
    /// have is much closer to firing than one whose every clause is wrong, and
    /// stopping early cannot tell them apart.
    pub satisfied: usize,
    pub total: usize,
    /// The first conjunct that does not hold.
    pub conjunct: String,
    /// Whether that conjunct constrains the successor state. False means the
    /// action was not available; true means it was, but produces a different
    /// next state than the one taken.
    pub about_next_state: bool,
    /// Set when the conjunct could not be evaluated rather than evaluating to
    /// FALSE — a guard that would have protected it has already failed.
    pub error: Option<String>,
}

impl Blocked {
    /// Closeness as a fraction of the action's conjuncts, compared without
    /// floating point.
    fn closer_than(&self, other: &Self) -> std::cmp::Ordering {
        (self.satisfied * other.total).cmp(&(other.satisfied * self.total))
    }
}

/// Actions are not followed deeper than this; a specification that nests
/// disjunctions further is reported on as far as it was explored.
const MAX_DEPTH: usize = 16;

#[derive(Default)]
struct Probe {
    /// The closest attempt at each named action. Keyed by name alone, so an
    /// action quantified over several bindings is reported once, at its best.
    best: BTreeMap<String, Blocked>,
    /// Set when some action was found to permit the step after all.
    allowed: bool,
}

impl<'m> Evaluator<'m> {
    /// The actions that came closest to permitting `from -> to`, closest
    /// first.
    ///
    /// Empty when the step is allowed. Every action other than the one taken
    /// fails at such a step, and reporting those would be noise rather than a
    /// diagnosis.
    pub fn why_not(&self, name: &str, from: &State, to: &State) -> Result<Vec<Blocked>> {
        let body = self.body_of(name)?;
        let mut ctx = Self::ctx(from, Some(to));
        let mut found = Probe::default();
        self.probe(body, &mut ctx, None, &mut found, 0)?;
        if found.allowed {
            return Ok(Vec::new());
        }
        let mut out: Vec<Blocked> = found.best.into_values().collect();
        out.sort_by(|a, b| {
            b.closer_than(a)
                .then_with(|| b.satisfied.cmp(&a.satisfied))
                .then_with(|| a.action.cmp(&b.action))
        });
        Ok(out)
    }

    /// Walk the disjunctive structure of an action, keeping track of which
    /// named action the current branch belongs to.
    fn probe(
        &self,
        e: &'m Expr,
        ctx: &mut Ctx<'m, '_>,
        label: Option<&str>,
        found: &mut Probe,
        depth: usize,
    ) -> Result<()> {
        if depth >= MAX_DEPTH {
            return Ok(());
        }
        match e {
            Expr::Binary(Op::Or, lhs, rhs) => {
                self.probe(lhs, ctx, label, found, depth + 1)?;
                self.probe(rhs, ctx, label, found, depth + 1)
            }
            Expr::Quant {
                kind: QuantKind::Exists,
                bounds,
                body,
            } => {
                for binding in self.expand(bounds, ctx)? {
                    let restore = push(ctx, &binding);
                    let walked = self.probe(body, ctx, label, found, depth + 1);
                    ctx.locals.truncate(restore);
                    walked?;
                }
                Ok(())
            }
            Expr::Let { defs, body } => {
                let base = ctx.locals.len();
                let scope = base + defs.len();
                for def in defs {
                    ctx.locals
                        .push((def.name.clone(), Local::Def { def, scope }));
                }
                let walked = self.probe(body, ctx, label, found, depth + 1);
                ctx.locals.truncate(base);
                walked
            }
            Expr::Apply(head, args) => {
                if let Expr::Ident(name) = &**head
                    && let Some(def) = self.spec.module.definition(name)
                {
                    return self.enter(def, args, ctx, found, depth);
                }
                self.record(e, ctx, label, found);
                Ok(())
            }
            Expr::Ident(name) => {
                if let Some(def) = self.spec.module.definition(name)
                    && def.params.is_empty()
                {
                    return self.enter(def, &[], ctx, found, depth);
                }
                self.record(e, ctx, label, found);
                Ok(())
            }
            _ => {
                self.record(e, ctx, label, found);
                Ok(())
            }
        }
    }

    /// Step into a named action, naming it by the arguments it was given.
    fn enter(
        &self,
        def: &'m Def,
        args: &'m [Expr],
        ctx: &mut Ctx<'m, '_>,
        found: &mut Probe,
        depth: usize,
    ) -> Result<()> {
        if def.params.len() != args.len() {
            return Ok(());
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval(arg, ctx)?);
        }
        let label = render_call(&def.name, &def.params, &values);

        // An operator sees its parameters and the module, never the locals of
        // whoever called it -- the same rule evaluation follows.
        let hidden = std::mem::take(&mut ctx.locals);
        for (param, value) in def.params.iter().zip(values) {
            ctx.locals.push((param.clone(), Local::Val(value)));
        }
        let walked = self.probe(&def.body, ctx, Some(&label), found, depth + 1);
        ctx.locals = hidden;
        walked
    }

    /// Evaluate a leaf action conjunct by conjunct and keep the furthest it got.
    fn record(&self, e: &'m Expr, ctx: &mut Ctx<'m, '_>, label: Option<&str>, found: &mut Probe) {
        let parts = conjuncts(e);
        let mut satisfied = 0;
        let mut first_failure = None;
        for part in &parts {
            // A conjunct after a failed guard may not be evaluable at all --
            // `Head(buf)` once `Len(buf) > 0` is false. That is a failure to
            // satisfy it, not a reason to abandon the whole diagnosis.
            match self.eval_bool(part, ctx) {
                Ok(true) => satisfied += 1,
                Ok(false) => {
                    first_failure.get_or_insert((*part, None));
                }
                Err(e) => {
                    first_failure.get_or_insert((*part, Some(e.to_string())));
                }
            }
        }
        let Some((part, error)) = first_failure else {
            found.allowed = true;
            return;
        };

        let action = label.map_or_else(|| truncate(&e.to_string()), ToString::to_string);
        let candidate = Blocked {
            action: action.clone(),
            satisfied,
            total: parts.len(),
            conjunct: truncate(&part.to_string()),
            about_next_state: part.mentions_next_state(),
            error,
        };
        let key = action.split('(').next().unwrap_or(&action).to_string();
        match found.best.get(&key) {
            Some(existing) if existing.closer_than(&candidate).is_ge() => {}
            _ => {
                found.best.insert(key, candidate);
            }
        }
    }
}

fn conjuncts(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Binary(Op::And, lhs, rhs) => {
            let mut out = conjuncts(lhs);
            out.extend(conjuncts(rhs));
            out
        }
        other => vec![other],
    }
}

fn render_call(name: &str, params: &[String], values: &[Value]) -> String {
    if params.is_empty() {
        return name.to_string();
    }
    let bindings: Vec<String> = params
        .iter()
        .zip(values)
        .map(|(param, value)| format!("{param} = {value}"))
        .collect();
    format!("{name}({})", bindings.join(", "))
}

const MAX_RENDERED: usize = 160;

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RENDERED {
        return text.to_string();
    }
    let head: String = text.chars().take(MAX_RENDERED).collect();
    format!("{head}...")
}
