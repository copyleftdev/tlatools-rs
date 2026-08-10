use std::collections::{BTreeMap, BTreeSet};

use tla_syntax::token::Op;
use tla_syntax::{Bound, Def, ExceptPath, Expr, Param, QuantKind};

use crate::builtin;
use crate::error::{Error, Result, type_error};
use crate::spec::Spec;
use crate::value::{Infinite, Value};

/// Nothing enumerable is materialized beyond this many elements. The bound
/// exists so a `SUBSET` or `[S -> T]` over an unexpectedly large set reports a
/// limit instead of exhausting memory.
pub const MAX_ELEMENTS: usize = 1 << 20;

const MAX_DEPTH: usize = 512;

/// A binding of every variable the specification declares.
pub type State = BTreeMap<String, Value>;

#[derive(Debug)]
pub struct Evaluator<'m> {
    pub(crate) spec: &'m Spec,
    constants: BTreeMap<String, Value>,
}

/// What a name in scope stands for.
///
/// A parameter declared `f(_)` is an operator, not a value, so what it binds to
/// has to be something that can be *applied* — and TLA+ lets that be any of
/// four things.
#[derive(Clone)]
pub(crate) enum Local<'m> {
    Val(Value),
    /// A `LET` definition, together with how much of the local stack it may
    /// see — everything pushed after that is the caller's, not its own.
    Def {
        def: &'m Def,
        scope: usize,
    },
    /// A `LAMBDA`, or a definition passed by name.
    Closure {
        params: &'m [Param],
        body: &'m Expr,
        scope: usize,
    },
    /// An operator symbol passed by itself, as in `FoldSet(+, 0, S)`.
    Symbol(Op),
    /// An operator of a standard module passed by name, as in `FoldSet(Len, ...)`.
    Builtin(String),
    /// What an instantiated module's declared name stands for. Held as an
    /// expression rather than a value because priming has to reach through it:
    /// under `x <- y`, an `x'` inside the instance means `y'`.
    Subst {
        expr: &'m Expr,
        module: usize,
        scope: usize,
    },
}

pub(crate) struct Ctx<'m, 'a> {
    /// The module whose scope names are read in. Evaluating an instantiated
    /// definition moves it, which is what makes `S!Op` mean `Op` as written in
    /// the instantiated module rather than here.
    pub(crate) module: usize,
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
        self.eval_bool(body, &mut self.ctx(state, None))
    }

    /// Is `from -> to` a step the named action permits?
    pub fn step_allowed(&self, name: &str, from: &State, to: &State) -> Result<bool> {
        let body = self.body_of(name)?;
        self.eval_bool(body, &mut self.ctx(from, Some(to)))
    }

    pub fn value_of(&self, name: &str, state: &State) -> Result<Value> {
        let body = self.body_of(name)?;
        self.eval(body, &mut self.ctx(state, None))
    }

    pub fn eval_at(&self, expr: &'m Expr, from: &State, to: Option<&State>) -> Result<Value> {
        self.eval(expr, &mut self.ctx(from, to))
    }

    pub(crate) fn body_of(&self, name: &str) -> Result<&'m Expr> {
        let (_, def) = self
            .spec
            .definition(self.spec.root(), name)
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

    pub(crate) fn ctx<'a>(&self, state: &'a State, next: Option<&'a State>) -> Ctx<'m, 'a> {
        Ctx {
            module: self.spec.root(),
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
            Expr::Qualified {
                instance,
                name,
                args,
            } => self.qualified(instance, name, args, ctx),
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
                Local::Closure { body, scope, .. } => {
                    self.run(name, body, scope, &[], Vec::new(), ctx)
                }
                Local::Subst {
                    expr,
                    module,
                    scope,
                } => self.substituted(expr, module, scope, ctx),
                Local::Symbol(_) | Local::Builtin(_) => Err(Error::Malformed(format!(
                    "`{name}` is an operator and must be applied to arguments"
                ))),
            };
        }
        if self.spec.declares_variable(ctx.module, name) {
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
        if let Some((module, def)) = self.spec.definition(ctx.module, name) {
            if !def.params.is_empty() {
                return Err(Error::Malformed(format!(
                    "`{name}` takes {} argument(s) but was used as a value",
                    def.params.len()
                )));
            }
            return self.in_module(module, ctx, |me, ctx| me.call(def, 0, &[], ctx));
        }
        if self.spec.declares_constant(ctx.module, name) {
            return Err(Error::Undefined(format!("constant `{name}` has no value")));
        }
        builtin::constant(name).ok_or_else(|| Error::Undefined(name.to_string()))
    }

    fn apply(&self, head: &'m Expr, args: &'m [Expr], ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        let Expr::Ident(name) = head else {
            return Err(Error::Malformed(
                "only a named operator can be applied to arguments".to_string(),
            ));
        };
        match lookup(name, ctx) {
            Some(Local::Def { def, scope }) => return self.invoke(def, scope, args, ctx),
            Some(Local::Closure {
                params,
                body,
                scope,
            }) => return self.enter_closure(name, params, body, scope, args, ctx),
            Some(Local::Symbol(op)) => {
                let values = self.eval_all(args, ctx)?;
                return Self::apply_symbol(op, values);
            }
            Some(Local::Builtin(builtin)) => {
                let values = self.eval_all(args, ctx)?;
                return builtin::call(&builtin, &values);
            }
            // An instance's declared name can itself be an operator, so an
            // application of it applies whatever it stands for.
            Some(Local::Subst {
                expr,
                module,
                scope,
            }) => {
                let Expr::Ident(replacement) = expr else {
                    return Err(Error::Malformed(format!(
                        "`{name}` stands for {expr}, which cannot be applied"
                    )));
                };
                let values = self.eval_all(args, ctx)?;
                let hidden = ctx.locals.split_off(scope);
                let out = self.in_module(module, ctx, |me, ctx| {
                    match me.spec.definition(ctx.module, replacement) {
                        Some((defining, def)) => {
                            me.in_module(defining, ctx, |me, ctx| me.call(def, 0, &values, ctx))
                        }
                        None => builtin::call(replacement, &values),
                    }
                });
                ctx.locals.truncate(scope);
                ctx.locals.extend(hidden);
                return out;
            }
            Some(Local::Val(_)) => {
                return Err(Error::Malformed(format!(
                    "`{name}` is a value, and cannot be applied to arguments"
                )));
            }
            None => {}
        }
        if let Some((module, def)) = self.spec.definition(ctx.module, name) {
            return self.in_module(module, ctx, |me, ctx| me.invoke(def, 0, args, ctx));
        }
        let values = self.eval_all(args, ctx)?;
        builtin::call(name, &values)
    }

    /// An operator symbol used as a value: `+` in `FoldSet(+, 0, S)`.
    fn apply_symbol(op: Op, mut values: Vec<Value>) -> Result<Value> {
        match values.len() {
            2 => {
                let right = values.pop().expect("length checked");
                let left = values.pop().expect("length checked");
                combine(op, left, right)
            }
            _ => Err(Error::Malformed(format!(
                "`{}` takes two arguments, given {}",
                op.symbol(),
                values.len()
            ))),
        }
    }

    /// Call a definition with argument *expressions*, so that an argument for
    /// an operator parameter is bound rather than evaluated.
    fn invoke(
        &self,
        def: &'m Def,
        scope: usize,
        args: &'m [Expr],
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
        let mut bindings = Vec::with_capacity(args.len());
        for (param, arg) in def.params.iter().zip(args) {
            bindings.push(if param.arity == 0 {
                Local::Val(self.eval(arg, ctx)?)
            } else {
                self.operator_argument(arg, ctx)?
            });
        }
        self.run(&def.name, &def.body, scope, &def.params, bindings, ctx)
    }

    fn enter_closure(
        &self,
        name: &str,
        params: &'m [Param],
        body: &'m Expr,
        scope: usize,
        args: &'m [Expr],
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        if params.len() != args.len() {
            return Err(Error::Malformed(format!(
                "`{name}` takes {} argument(s), given {}",
                params.len(),
                args.len()
            )));
        }
        let mut bindings = Vec::with_capacity(args.len());
        for (param, arg) in params.iter().zip(args) {
            bindings.push(if param.arity == 0 {
                Local::Val(self.eval(arg, ctx)?)
            } else {
                self.operator_argument(arg, ctx)?
            });
        }
        self.run(name, body, scope, params, bindings, ctx)
    }

    /// What an argument means when the parameter it fills is an operator.
    fn operator_argument(&self, arg: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Local<'m>> {
        match arg {
            Expr::Lambda { params, body } => Ok(Local::Closure {
                params,
                body,
                scope: ctx.locals.len(),
            }),
            Expr::Ident(name) => {
                if let Some(
                    local @ (Local::Closure { .. } | Local::Symbol(_) | Local::Builtin(_)),
                ) = lookup(name, ctx)
                {
                    return Ok(local);
                }
                if let Some((_, def)) = self.spec.definition(ctx.module, name) {
                    return Ok(Local::Closure {
                        params: &def.params,
                        body: &def.body,
                        scope: 0,
                    });
                }
                if let Some(op) = symbol_operator(name) {
                    return Ok(Local::Symbol(op));
                }
                Ok(Local::Builtin(name.clone()))
            }
            other => Err(Error::Malformed(format!(
                "an operator was expected here, but {other} is an expression"
            ))),
        }
    }

    /// Read the rest of this expression in another module's scope.
    fn in_module<T>(
        &self,
        module: usize,
        ctx: &mut Ctx<'m, '_>,
        f: impl FnOnce(&Self, &mut Ctx<'m, '_>) -> Result<T>,
    ) -> Result<T> {
        let previous = std::mem::replace(&mut ctx.module, module);
        let out = f(self, ctx);
        ctx.module = previous;
        out
    }

    /// A declared name of an instantiated module, which stands for whatever
    /// the `WITH` clause put in its place — read back where the instance was
    /// written, and under the prime in force here.
    fn substituted(
        &self,
        expr: &'m Expr,
        module: usize,
        scope: usize,
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        let hidden = ctx.locals.split_off(scope);
        let previous = std::mem::replace(&mut ctx.module, module);
        let out = self.eval(expr, ctx);
        ctx.module = previous;
        ctx.locals.truncate(scope);
        ctx.locals.extend(hidden);
        out
    }

    /// `S!Op(args)`: `Op` as written in the module `S` instantiates, with that
    /// module's declared names standing for what `S` substituted for them.
    fn qualified(
        &self,
        instance: &str,
        name: &str,
        args: &'m [Expr],
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        // `A!B!C` names a chain of instances; each step moves into the next.
        if let Some((head, rest)) = instance.split_once('!') {
            let outer = self
                .spec
                .instance(ctx.module, head)
                .ok_or_else(|| Error::Undefined(format!("no instance named `{head}`")))?;
            let scope = Self::bind_substitutions(outer, ctx);
            let out = self.in_module(outer.target, ctx, |me, ctx| {
                me.qualified(rest, name, args, ctx)
            });
            ctx.locals.truncate(scope);
            return out;
        }

        let found = self
            .spec
            .instance(ctx.module, instance)
            .ok_or_else(|| Error::Undefined(format!("no instance named `{instance}`")))?;
        let (module, def) = self
            .spec
            .definition(found.target, name)
            .ok_or_else(|| Error::Undefined(format!("`{instance}!{name}`")))?;

        // The substituting expressions belong to the scope the instance was
        // written in, so they are bound before the arguments are read.
        let scope = Self::bind_substitutions(found, ctx);
        let out = self.in_module(module, ctx, |me, ctx| me.invoke(def, scope, args, ctx));
        ctx.locals.truncate(scope);
        out
    }

    fn bind_substitutions(instance: &'m crate::spec::Instance, ctx: &mut Ctx<'m, '_>) -> usize {
        let outer = ctx.locals.len();
        let here = ctx.module;
        for (name, expr) in &instance.subs {
            ctx.locals.push((
                name.clone(),
                Local::Subst {
                    expr,
                    module: here,
                    scope: outer,
                },
            ));
        }
        ctx.locals.len()
    }

    /// Evaluate a body with its parameters bound and the caller's locals
    /// hidden, which is the one rule operator application has to follow.
    fn run(
        &self,
        name: &str,
        body: &'m Expr,
        scope: usize,
        params: &[Param],
        bindings: Vec<Local<'m>>,
        ctx: &mut Ctx<'m, '_>,
    ) -> Result<Value> {
        if ctx.depth >= MAX_DEPTH {
            return Err(Error::Malformed(format!(
                "`{name}` recursed more than {MAX_DEPTH} deep"
            )));
        }
        let hidden = ctx.locals.split_off(scope);
        for (param, binding) in params.iter().zip(bindings) {
            ctx.locals.push((param.name.clone(), binding));
        }
        ctx.depth += 1;
        let out = self.eval(body, ctx);
        ctx.depth -= 1;
        ctx.locals.truncate(scope);
        ctx.locals.extend(hidden);
        out
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
        let bindings = args.iter().cloned().map(Local::Val).collect();
        self.run(&def.name, &def.body, scope, &def.params, bindings, ctx)
    }

    // -------------------------------------------------------------- operators

    fn unary(&self, op: Op, inner: &'m Expr, ctx: &mut Ctx<'m, '_>) -> Result<Value> {
        if let Some((module, def)) = self.spec.definition(ctx.module, op.symbol())
            && def.params.len() == 1
        {
            let values = vec![self.eval(inner, ctx)?];
            return self.in_module(module, ctx, |me, ctx| me.call(def, 0, &values, ctx));
        }
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
        // `\prec`, `\oplus`, `&` and the rest have a symbol and a precedence
        // but no meaning until a specification gives them one.
        if let Op::User(symbol) = op
            && let Some((module, def)) = self.spec.definition(ctx.module, symbol)
        {
            let values = vec![self.eval(lhs, ctx)?, self.eval(rhs, ctx)?];
            return self.in_module(module, ctx, |me, ctx| me.call(def, 0, &values, ctx));
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

/// The operator a bare symbol names, for `FoldSet(+, 0, S)`.
fn symbol_operator(name: &str) -> Option<Op> {
    const CANDIDATES: &[Op] = &[
        Op::Plus,
        Op::Minus,
        Op::Times,
        Op::Div,
        Op::Mod,
        Op::Pow,
        Op::Cup,
        Op::Cap,
        Op::SetMinus,
        Op::Concat,
        Op::AtAt,
        Op::And,
        Op::Or,
        Op::Eq,
        Op::DotDot,
    ];
    CANDIDATES.iter().copied().find(|op| op.symbol() == name)
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

/// Every member of a set, for the operations that have to visit them all.
/// Membership of an infinite set is decidable where enumeration is not, so the
/// two failures are reported differently.
fn elements(v: &Value) -> Result<Vec<Value>> {
    if matches!(v, Value::Infinite(_)) {
        return Err(Error::Unbounded(format!("{v} cannot be enumerated")));
    }
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
