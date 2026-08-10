use std::collections::{BTreeMap, BTreeSet};

use tla_syntax::token::Op;
use tla_syntax::{Bound, Def, ExceptPath, Expr, Module, QuantKind, parse_module};

use crate::builtin;
use crate::error::{Error, Result, type_error};
use crate::value::{Infinite, Value};

/// Nothing enumerable is materialized beyond this many elements. The bound
/// exists so a `SUBSET` or `[S -> T]` over an unexpectedly large set reports a
/// limit instead of exhausting memory.
pub const MAX_ELEMENTS: usize = 1 << 20;

const MAX_DEPTH: usize = 512;

/// A binding of every variable the specification declares.
pub type State = BTreeMap<String, Value>;

#[derive(Debug)]
pub struct Spec {
    pub(crate) module: Module,
    variables: BTreeSet<String>,
    constants: BTreeSet<String>,
}

impl Spec {
    pub fn parse(src: &str) -> Result<Self> {
        let module = parse_module(src)?;
        let variables = module.variables().cloned().collect();
        let constants = module.constants().map(|d| d.name.clone()).collect();
        Ok(Self {
            module,
            variables,
            constants,
        })
    }

    pub fn name(&self) -> &str {
        &self.module.name
    }

    pub fn variables(&self) -> impl Iterator<Item = &str> {
        self.variables.iter().map(String::as_str)
    }

    pub fn constants(&self) -> impl Iterator<Item = &str> {
        self.constants.iter().map(String::as_str)
    }

    pub fn defines(&self, name: &str) -> bool {
        self.module.definition(name).is_some()
    }
}

#[derive(Debug)]
pub struct Evaluator<'m> {
    pub(crate) spec: &'m Spec,
    constants: BTreeMap<String, Value>,
}

#[derive(Clone)]
pub(crate) enum Local<'m> {
    Val(Value),
    /// A `LET` definition, together with how much of the local stack it may
    /// see — everything pushed after that is the caller's, not its own.
    Def {
        def: &'m Def,
        scope: usize,
    },
}

pub(crate) struct Ctx<'m, 'a> {
    pub(crate) state: &'a State,
    pub(crate) next: Option<&'a State>,
    pub(crate) primed: bool,
    pub(crate) locals: Vec<(String, Local<'m>)>,
    pub(crate) at: Vec<Value>,
    pub(crate) depth: usize,
}

impl<'m> Evaluator<'m> {
    /// Every declared constant must be given a value; a specification with a
    /// free constant has no determinate meaning at a state.
    pub fn new(spec: &'m Spec, constants: BTreeMap<String, Value>) -> Result<Self> {
        let missing: Vec<&str> = spec
            .constants()
            .filter(|c| !constants.contains_key(*c))
            .collect();
        if !missing.is_empty() {
            return Err(Error::Undefined(format!(
                "constants without a value: {}",
                missing.join(", ")
            )));
        }
        Ok(Self { spec, constants })
    }

    /// Does the named state predicate hold at `state`?
    pub fn holds_at(&self, name: &str, state: &State) -> Result<bool> {
        let body = self.body_of(name)?;
        self.eval_bool(body, &mut Self::ctx(state, None))
    }

    /// Is `from -> to` a step the named action permits?
    pub fn step_allowed(&self, name: &str, from: &State, to: &State) -> Result<bool> {
        let body = self.body_of(name)?;
        self.eval_bool(body, &mut Self::ctx(from, Some(to)))
    }

    pub fn value_of(&self, name: &str, state: &State) -> Result<Value> {
        let body = self.body_of(name)?;
        self.eval(body, &mut Self::ctx(state, None))
    }

    pub fn eval_at(&self, expr: &'m Expr, from: &State, to: Option<&State>) -> Result<Value> {
        self.eval(expr, &mut Self::ctx(from, to))
    }

    pub(crate) fn body_of(&self, name: &str) -> Result<&'m Expr> {
        let def = self
            .spec
            .module
            .definition(name)
            .ok_or_else(|| Error::Undefined(name.to_string()))?;
        if def.params.is_empty() {
            Ok(&def.body)
        } else {
            Err(Error::Malformed(format!(
                "`{name}` takes {} argument(s) and is not a predicate",
                def.params.len()
            )))
        }
    }

    pub(crate) fn ctx<'a>(state: &'a State, next: Option<&'a State>) -> Ctx<'m, 'a> {
        Ctx {
            state,
            next,
            primed: false,
            locals: Vec::new(),
            at: Vec::new(),
            depth: 0,
        }
    }

    // ------------------------------------------------------------ evaluation

    pub(crate) fn eval(&self, e: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        match e {
            Expr::Num(n) => Ok(Value::Int(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Ident(name) => self.ident(name, ctx),
            Expr::Prime(inner) => self.primed(inner, ctx),
            Expr::At => ctx
                .at
                .last()
                .cloned()
                .ok_or_else(|| Error::Malformed("`@` outside an EXCEPT update".to_string())),
            Expr::Apply(head, args) => self.apply(head, args, ctx),
            Expr::FnApply(f, args) => {
                let func = self.eval(f, ctx)?;
                let key = self.key(args, ctx)?;
                func.apply(&key)
                    .ok_or_else(|| Error::Type(format!("{func} is not defined at {key}")))
            }
            Expr::Field(inner, name) => {
                let v = self.eval(inner, ctx)?;
                v.apply(&Value::Str(name.clone()))
                    .ok_or_else(|| Error::Type(format!("{v} has no field `{name}`")))
            }
            Expr::Qualified { instance, name, .. } => Err(Error::Undefined(format!(
                "{instance}!{name}: instance-qualified names are not resolved; \
                 evaluate against the instantiated module directly"
            ))),
            Expr::Unary(op, inner) => self.unary(*op, inner, ctx),
            Expr::Binary(op, l, r) => self.binary(*op, l, r, ctx),
            Expr::Tuple(items) => Ok(Value::Seq(self.eval_all(items, ctx)?)),
            Expr::SetEnum(items) => Ok(Value::set(self.eval_all(items, ctx)?)),
            Expr::SetFilter { bound, pred } => self.set_filter(bound, pred, ctx),
            Expr::SetMap { expr, bounds } => self.set_map(expr, bounds, ctx),
            Expr::Record(fields) => {
                let mut out = BTreeMap::new();
                for (k, v) in fields {
                    out.insert(k.clone(), self.eval(v, ctx)?);
                }
                Ok(Value::Record(out))
            }
            Expr::RecordSet(fields) => self.record_set(fields, ctx),
            Expr::FnDef { bounds, body } => self.fn_def(bounds, body, ctx),
            Expr::FnSet { domain, range } => self.fn_set(domain, range, ctx),
            Expr::Except { base, updates } => {
                let mut v = self.eval(base, ctx)?;
                for (path, rhs) in updates {
                    v = self.update(v, path, rhs, ctx)?;
                }
                Ok(v)
            }
            Expr::Quant { kind, bounds, body } => self.quantify(*kind, bounds, body, ctx),
            Expr::Choose { bound, body } => self.choose(bound, body, ctx),
            Expr::Let { defs, body } => {
                let base = ctx.locals.len();
                let scope = base + defs.len();
                for def in defs {
                    ctx.locals
                        .push((def.name.clone(), Local::Def { def, scope }));
                }
                let out = self.eval(body, ctx);
                ctx.locals.truncate(base);
                out
            }
            Expr::If {
                cond,
                then,
                otherwise,
            } => {
                if self.eval_bool(cond, ctx)? {
                    self.eval(then, ctx)
                } else {
                    self.eval(otherwise, ctx)
                }
            }
            Expr::Case { arms, other } => {
                for (guard, result) in arms {
                    if self.eval_bool(guard, ctx)? {
                        return self.eval(result, ctx);
                    }
                }
                match other {
                    Some(e) => self.eval(e, ctx),
                    None => Err(Error::Malformed("no CASE arm applies".to_string())),
                }
            }
            // `[A]_v` is `A \/ UNCHANGED v`, which one step decides.
            Expr::ActionBox { action, subscript } => {
                if self.eval_bool(action, ctx)? {
                    return Ok(Value::Bool(true));
                }
                self.unchanged(subscript, ctx)
            }
            Expr::Lambda { .. } => Err(Error::Malformed(
                "a LAMBDA is an operator, and can only be passed to one".to_string(),
            )),
            Expr::ActionAngle { .. } | Expr::Fairness { .. } => Err(Error::NotGround(
                "a fairness or angle-bracket formula is about behaviours, not about one step"
                    .to_string(),
            )),
        }
    }

    pub(crate) fn eval_bool(&self, e: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<bool> {
        match self.eval(e, ctx)? {
            Value::Bool(b) => Ok(b),
            other => type_error(format!("expected a boolean, got {other}")),
        }
    }

    fn eval_all(&self, items: &'m [Expr], ctx: &mut Ctx<'m, '_>) -> Result<Vec<Value>> {
        items.iter().map(|e| self.eval(e, ctx)).collect()
    }

    fn key(&self, args: &'m [Expr], ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let mut values = self.eval_all(args, ctx)?;
        Ok(if values.len() == 1 {
            values.remove(0)
        } else {
            Value::Seq(values)
        })
    }

    fn primed(&self, e: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let saved = ctx.primed;
        ctx.primed = true;
        let out = self.eval(e, ctx);
        ctx.primed = saved;
        out
    }

    fn ident(&self, name: &str, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        if let Some(local) = lookup(name, ctx) {
            return match local {
                Local::Val(v) => Ok(v),
                Local::Def { def, scope } => self.call(def, scope, &[], ctx),
            };
        }
        if self.spec.variables.contains(name) {
            let source = if ctx.primed {
                ctx.next.ok_or_else(|| {
                    Error::NoNextState(format!(
                        "`{name}'` needs a successor state, but only one state was given"
                    ))
                })?
            } else {
                ctx.state
            };
            return source.get(name).cloned().ok_or_else(|| {
                Error::Malformed(format!("the state gives no value for variable `{name}`"))
            });
        }
        if let Some(v) = self.constants.get(name) {
            return Ok(v.clone());
        }
        if let Some(def) = self.spec.module.definition(name) {
            if !def.params.is_empty() {
                return Err(Error::Malformed(format!(
                    "`{name}` takes {} argument(s) but was used as a value",
                    def.params.len()
                )));
            }
            return self.call(def, 0, &[], ctx);
        }
        builtin::constant(name).ok_or_else(|| Error::Undefined(name.to_string()))
    }

    fn apply(&self, head: &'m Expr, args: &'m [Expr], ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let Expr::Ident(name) = head else {
            return Err(Error::Malformed(
                "only a named operator can be applied to arguments".to_string(),
            ));
        };
        let values = self.eval_all(args, ctx)?;
        if let Some(Local::Def { def, scope }) = lookup(name, ctx) {
            return self.call(def, scope, &values, ctx);
        }
        if let Some(def) = self.spec.module.definition(name) {
            return self.call(def, 0, &values, ctx);
        }
        builtin::call(name, &values)
    }

    fn call(
        &self,
        def: &'m Def,
        scope: usize,
        args: &[Value],
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        if def.params.len() != args.len() {
            return Err(Error::Malformed(format!(
                "`{}` takes {} argument(s), given {}",
                def.name,
                def.params.len(),
                args.len()
            )));
        }
        if ctx.depth >= MAX_DEPTH {
            return Err(Error::Malformed(format!(
                "`{}` recursed more than {MAX_DEPTH} deep",
                def.name
            )));
        }
        // An operator sees its own parameters and the scope it was written in,
        // never the locals of whoever called it.
        let hidden = ctx.locals.split_off(scope);
        for (param, arg) in def.params.iter().zip(args) {
            ctx.locals
                .push((param.name.clone(), Local::Val(arg.clone())));
        }
        ctx.depth += 1;
        let out = self.eval(&def.body, ctx);
        ctx.depth -= 1;
        ctx.locals.truncate(scope);
        ctx.locals.extend(hidden);
        out
    }

    // -------------------------------------------------------------- operators

    fn unary(&self, op: Op, inner: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        match op {
            Op::Not => Ok(Value::Bool(!self.eval_bool(inner, ctx)?)),
            Op::Minus => match self.eval(inner, ctx)? {
                Value::Int(n) => Ok(Value::Int(-n)),
                other => type_error(format!("cannot negate {other}")),
            },
            Op::Domain => {
                let v = self.eval(inner, ctx)?;
                v.domain()
                    .map(Value::Set)
                    .ok_or_else(|| Error::Type(format!("DOMAIN of {v}, which is not a function")))
            }
            Op::Subset => {
                let v = self.eval(inner, ctx)?;
                powerset(&elements(&v)?)
            }
            Op::BigUnion => {
                let v = self.eval(inner, ctx)?;
                let mut out = BTreeSet::new();
                for member in elements(&v)? {
                    out.extend(elements(&member)?);
                }
                Ok(Value::Set(out))
            }
            Op::Unchanged => self.unchanged(inner, ctx),
            Op::Enabled => Err(Error::NotGround(
                "ENABLED asks whether some successor state exists, which needs a search"
                    .to_string(),
            )),
            Op::Always | Op::Eventually => Err(Error::NotGround(
                "a temporal formula is about behaviours, not about one state".to_string(),
            )),
            other => Err(Error::Malformed(format!(
                "{other:?} is not a prefix operator"
            ))),
        }
    }

    fn unchanged(&self, e: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let before = self.eval(e, ctx)?;
        let after = self.primed(e, ctx)?;
        Ok(Value::Bool(before == after))
    }

    fn binary(&self, op: Op, lhs: &'m Expr, rhs: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        // The propositional connectives must not evaluate their right operand
        // when the left already decides the answer: a guard like
        // `Len(buf) > 0 /\ Head(buf) = x` relies on it.
        match op {
            Op::And => {
                return Ok(Value::Bool(
                    self.eval_bool(lhs, ctx)? && self.eval_bool(rhs, ctx)?,
                ));
            }
            Op::Or => {
                return Ok(Value::Bool(
                    self.eval_bool(lhs, ctx)? || self.eval_bool(rhs, ctx)?,
                ));
            }
            Op::Implies => {
                return Ok(Value::Bool(
                    !self.eval_bool(lhs, ctx)? || self.eval_bool(rhs, ctx)?,
                ));
            }
            _ => {}
        }
        let left = self.eval(lhs, ctx)?;
        let right = self.eval(rhs, ctx)?;
        combine(op, left, right)
    }
}

/// The operators whose meaning depends only on their operands' values.
fn combine(op: Op, a: Value, b: Value) -> Result<Value> {
    match op {
        Op::Equiv => Ok(Value::Bool(as_bool(&a)? == as_bool(&b)?)),
        Op::Eq => Ok(Value::Bool(a == b)),
        Op::Neq => Ok(Value::Bool(a != b)),
        Op::Lt => Ok(Value::Bool(as_int(&a)? < as_int(&b)?)),
        Op::Gt => Ok(Value::Bool(as_int(&a)? > as_int(&b)?)),
        Op::Le => Ok(Value::Bool(as_int(&a)? <= as_int(&b)?)),
        Op::Ge => Ok(Value::Bool(as_int(&a)? >= as_int(&b)?)),
        Op::Plus => arith(&a, &b, i64::checked_add, "+"),
        Op::Minus => arith(&a, &b, i64::checked_sub, "-"),
        Op::Times => arith(&a, &b, i64::checked_mul, "*"),
        Op::Div => arith(&a, &b, i64::checked_div_euclid, "\\div"),
        Op::Mod => arith(&a, &b, i64::checked_rem_euclid, "%"),
        Op::Pow => {
            let exp = u32::try_from(as_int(&b)?)
                .map_err(|_| Error::Type("exponent out of range".to_string()))?;
            as_int(&a)?
                .checked_pow(exp)
                .map(Value::Int)
                .ok_or_else(|| Error::Type("^ overflowed".to_string()))
        }
        Op::DotDot => Ok(Value::interval(as_int(&a)?, as_int(&b)?)),
        Op::In => Ok(Value::Bool(member(&a, &b)?)),
        Op::NotIn => Ok(Value::Bool(!member(&a, &b)?)),
        Op::Subseteq => {
            for item in elements(&a)? {
                if !member(&item, &b)? {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        Op::Supseteq => {
            for item in elements(&b)? {
                if !member(&item, &a)? {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
        Op::Cup => {
            let mut out = set_of(&a)?.clone();
            out.extend(set_of(&b)?.iter().cloned());
            Ok(Value::Set(out))
        }
        Op::Cap => Ok(Value::Set(
            set_of(&a)?.intersection(set_of(&b)?).cloned().collect(),
        )),
        Op::SetMinus => Ok(Value::Set(
            set_of(&a)?.difference(set_of(&b)?).cloned().collect(),
        )),
        Op::Cartesian => {
            let mut out = BTreeSet::new();
            for x in set_of(&a)? {
                for y in set_of(&b)? {
                    out.insert(Value::Seq(vec![x.clone(), y.clone()]));
                }
            }
            Ok(Value::Set(out))
        }
        Op::Concat => match (&a, &b) {
            (Value::Seq(x), Value::Seq(y)) => Ok(Value::Seq(x.iter().chain(y).cloned().collect())),
            _ => type_error(format!("\\o expects two sequences, got {a} and {b}")),
        },
        Op::OneTo => Ok(Value::function(BTreeMap::from([(a, b)]))),
        Op::AtAt => {
            let mut left = a
                .entries()
                .ok_or_else(|| Error::Type(format!("@@ expects functions, got {a}")))?;
            let right = b
                .entries()
                .ok_or_else(|| Error::Type(format!("@@ expects functions, got {b}")))?;
            for (k, v) in right {
                left.entry(k).or_insert(v);
            }
            Ok(Value::function(left))
        }
        other => Err(Error::Malformed(format!(
            "{other:?} is not an infix operator"
        ))),
    }
}

impl<'m> Evaluator<'m> {
    // ----------------------------------------------------------------- sets

    fn set_filter(&self, bound: &'m Bound, pred: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let mut out = BTreeSet::new();
        for binding in self.expand(std::slice::from_ref(bound), ctx)? {
            let restore = push(ctx, &binding);
            let keep = self.eval_bool(pred, ctx);
            ctx.locals.truncate(restore);
            if keep? {
                out.insert(element_of(&binding));
            }
        }
        Ok(Value::Set(out))
    }

    fn set_map(&self, expr: &'m Expr, bounds: &'m [Bound], ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let mut out = BTreeSet::new();
        for binding in self.expand(bounds, ctx)? {
            let restore = push(ctx, &binding);
            let v = self.eval(expr, ctx);
            ctx.locals.truncate(restore);
            out.insert(v?);
        }
        Ok(Value::Set(out))
    }

    fn record_set(&self, fields: &'m [(String, Expr)], ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let mut out: Vec<BTreeMap<String, Value>> = vec![BTreeMap::new()];
        for (name, domain) in fields {
            let choices = elements(&self.eval(domain, ctx)?)?;
            check_size(out.len().saturating_mul(choices.len()), "a record set")?;
            out = out
                .into_iter()
                .flat_map(|partial| {
                    choices.iter().map(move |c| {
                        let mut next = partial.clone();
                        next.insert(name.clone(), c.clone());
                        next
                    })
                })
                .collect();
        }
        Ok(Value::set(out.into_iter().map(Value::Record)))
    }

    fn fn_def(&self, bounds: &'m [Bound], body: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let mut entries = BTreeMap::new();
        for binding in self.expand(bounds, ctx)? {
            let key = element_of(&binding);
            let restore = push(ctx, &binding);
            let v = self.eval(body, ctx);
            ctx.locals.truncate(restore);
            entries.insert(key, v?);
        }
        Ok(Value::function(entries))
    }

    fn fn_set(&self, domain: &'m Expr, range: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let keys = elements(&self.eval(domain, ctx)?)?;
        let range = self.eval(range, ctx)?;
        let values = elements(&range)?;
        let count = values
            .len()
            .checked_pow(u32::try_from(keys.len()).unwrap_or(u32::MAX))
            .unwrap_or(usize::MAX);
        check_size(count, "a function set")?;

        let mut out = BTreeSet::new();
        for mut index in 0..count {
            let mut entries = BTreeMap::new();
            for key in &keys {
                entries.insert(key.clone(), values[index % values.len()].clone());
                index /= values.len();
            }
            out.insert(Value::function(entries));
        }
        if keys.is_empty() {
            out.insert(Value::Seq(Vec::new()));
        }
        Ok(Value::Set(out))
    }

    fn quantify(
        &self,
        kind: QuantKind,
        bounds: &'m [Bound],
        body: &'m Expr,
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        let wanted = kind == QuantKind::Exists;
        for binding in self.expand(bounds, ctx)? {
            let restore = push(ctx, &binding);
            let holds = self.eval_bool(body, ctx);
            ctx.locals.truncate(restore);
            if holds? == wanted {
                return Ok(Value::Bool(wanted));
            }
        }
        Ok(Value::Bool(!wanted))
    }

    /// `CHOOSE` must be deterministic: the same set and predicate always give
    /// the same answer. Taking the least satisfying element in the value order
    /// is one such rule, and sets are already held in that order.
    fn choose(&self, bound: &'m Bound, body: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        for binding in self.expand(std::slice::from_ref(bound), ctx)? {
            let restore = push(ctx, &binding);
            let holds = self.eval_bool(body, ctx);
            ctx.locals.truncate(restore);
            if holds? {
                return Ok(element_of(&binding));
            }
        }
        Err(Error::Malformed(
            "CHOOSE found no value satisfying its predicate".to_string(),
        ))
    }

    /// All the ways the bound variables can be assigned. A later bound's
    /// domain may mention an earlier bound's variable, so they are evaluated
    /// with the bindings so far in scope.
    pub(crate) fn expand(
        &self,
        bounds: &'m [Bound],
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Vec<Vec<(String, Value)>>> {
        let mut out = Vec::new();
        let mut current = Vec::new();
        self.expand_into(bounds, ctx, &mut current, &mut out)?;
        Ok(out)
    }

    fn expand_into(
        &self,
        bounds: &'m [Bound],
        ctx: &mut Ctx<'m, '_>,
        current: &mut Vec<(String, Value)>,
        out: &mut Vec<Vec<(String, Value)>>,
    ) -> Result<()> {
        let Some((first, rest)) = bounds.split_first() else {
            check_size(out.len() + 1, "a quantifier")?;
            out.push(current.clone());
            return Ok(());
        };
        let Some(domain) = &first.domain else {
            return Err(Error::Unbounded(format!(
                "`{}` is quantified over no set, so it cannot be enumerated",
                first.names.join(", ")
            )));
        };
        let items = elements(&self.eval(domain, ctx)?)?;

        if first.destructure {
            for item in items {
                let Value::Seq(parts) = &item else {
                    return type_error(format!("cannot destructure {item}, which is not a tuple"));
                };
                if parts.len() != first.names.len() {
                    return type_error(format!(
                        "cannot destructure {item} into {} names",
                        first.names.len()
                    ));
                }
                let restore = ctx.locals.len();
                for (name, part) in first.names.iter().zip(parts) {
                    current.push((name.clone(), part.clone()));
                    ctx.locals.push((name.clone(), Local::Val(part.clone())));
                }
                self.expand_into(rest, ctx, current, out)?;
                current.truncate(current.len() - first.names.len());
                ctx.locals.truncate(restore);
            }
            return Ok(());
        }
        self.product(&first.names, &items, rest, ctx, current, out)
    }

    fn product(
        &self,
        names: &[String],
        items: &[Value],
        rest: &'m [Bound],
        ctx: &mut Ctx<'m, '_>,
        current: &mut Vec<(String, Value)>,
        out: &mut Vec<Vec<(String, Value)>>,
    ) -> Result<()> {
        let Some((name, more)) = names.split_first() else {
            return self.expand_into(rest, ctx, current, out);
        };
        for item in items {
            let restore = ctx.locals.len();
            current.push((name.clone(), item.clone()));
            ctx.locals.push((name.clone(), Local::Val(item.clone())));
            self.product(more, items, rest, ctx, current, out)?;
            current.pop();
            ctx.locals.truncate(restore);
        }
        Ok(())
    }

    /// One `![a][b] = e` update. `@` inside `e` is the value being replaced.
    fn update(
        &self,
        base: Value,
        path: &'m [ExceptPath],
        rhs: &'m Expr,
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        let Some((step, rest)) = path.split_first() else {
            ctx.at.push(base);
            let out = self.eval(rhs, ctx);
            ctx.at.pop();
            return out;
        };
        let key = match step {
            ExceptPath::Index(e) => self.eval(e, ctx)?,
            ExceptPath::Field(name) => Value::Str(name.clone()),
        };
        let old = base
            .apply(&key)
            .ok_or_else(|| Error::Type(format!("EXCEPT: {base} is not defined at {key}")))?;
        let replacement = self.update(old, rest, rhs, ctx)?;
        let mut entries = base
            .entries()
            .ok_or_else(|| Error::Type(format!("EXCEPT: {base} is not a function")))?;
        entries.insert(key, replacement);
        Ok(Value::function(entries))
    }
}

fn lookup<'m>(name: &str, ctx: &Ctx<'m, '_>) -> Option<Local<'m>> {
    ctx.locals
        .iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, local)| local.clone())
}

pub(crate) fn push(ctx: &mut Ctx<'_, '_>, binding: &[(String, Value)]) -> usize {
    let restore = ctx.locals.len();
    for (name, value) in binding {
        ctx.locals.push((name.clone(), Local::Val(value.clone())));
    }
    restore
}

/// The value a binding contributes to a set or a function's domain: the single
/// bound variable, or the tuple of them.
fn element_of(binding: &[(String, Value)]) -> Value {
    if let [(_, only)] = binding {
        only.clone()
    } else {
        Value::Seq(binding.iter().map(|(_, v)| v.clone()).collect())
    }
}

fn powerset(items: &[Value]) -> Result<Value> {
    let bits = u32::try_from(items.len()).unwrap_or(u32::MAX);
    let count = 1usize.checked_shl(bits).unwrap_or(usize::MAX);
    check_size(count, "SUBSET")?;
    let mut out = BTreeSet::new();
    for mask in 0..count {
        out.insert(Value::set(
            items
                .iter()
                .enumerate()
                .filter(|(i, _)| mask >> i & 1 == 1)
                .map(|(_, v)| v.clone()),
        ));
    }
    Ok(Value::Set(out))
}

fn check_size(count: usize, what: &str) -> Result<()> {
    if count > MAX_ELEMENTS {
        return Err(Error::Unbounded(format!(
            "{what} would have {count} elements, over the {MAX_ELEMENTS} limit"
        )));
    }
    Ok(())
}

fn elements(v: &Value) -> Result<Vec<Value>> {
    Ok(set_of(v)?.iter().cloned().collect())
}

fn member(elem: &Value, set: &Value) -> Result<bool> {
    match set {
        Value::Set(s) => Ok(s.contains(elem)),
        Value::Infinite(Infinite::Nat) => Ok(matches!(elem, Value::Int(n) if *n >= 0)),
        Value::Infinite(Infinite::Int) => Ok(matches!(elem, Value::Int(_))),
        Value::Infinite(Infinite::Strings) => Ok(matches!(elem, Value::Str(_))),
        Value::Infinite(Infinite::Sequences(of)) => {
            let Value::Seq(items) = elem else {
                return Ok(false);
            };
            for item in items {
                if !member(item, of)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        other => type_error(format!("\\in expects a set on the right, got {other}")),
    }
}

fn set_of(v: &Value) -> Result<&BTreeSet<Value>> {
    match v {
        Value::Set(s) => Ok(s),
        Value::Infinite(_) => Err(Error::Unbounded(format!(
            "{v} cannot take part in a set operation"
        ))),
        other => type_error(format!(
            "expected a set, got {} ({other})",
            other.type_name()
        )),
    }
}

fn as_int(v: &Value) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        other => type_error(format!("expected an integer, got {other}")),
    }
}

fn as_bool(v: &Value) -> Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => type_error(format!("expected a boolean, got {other}")),
    }
}

fn arith(a: &Value, b: &Value, f: fn(i64, i64) -> Option<i64>, name: &str) -> Result<Value> {
    f(as_int(a)?, as_int(b)?)
        .map(Value::Int)
        .ok_or_else(|| Error::Type(format!("{a} {name} {b} is undefined or overflows")))
}
