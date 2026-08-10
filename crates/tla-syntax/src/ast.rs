use crate::token::Op;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub extends: Vec<String>,
    pub units: Vec<Unit>,
}

impl Module {
    pub fn definition(&self, name: &str) -> Option<&Def> {
        self.units.iter().find_map(|u| match u {
            Unit::Def(d) if d.name == name => Some(d),
            _ => None,
        })
    }

    pub fn constants(&self) -> impl Iterator<Item = &Decl> {
        self.units
            .iter()
            .filter_map(|u| match u {
                Unit::Constants(ds) => Some(ds),
                _ => None,
            })
            .flatten()
    }

    pub fn variables(&self) -> impl Iterator<Item = &String> {
        self.units
            .iter()
            .filter_map(|u| match u {
                Unit::Variables(vs) => Some(vs),
                _ => None,
            })
            .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unit {
    Constants(Vec<Decl>),
    Variables(Vec<String>),
    Recursive(Vec<Decl>),
    Def(Def),
    /// `S == INSTANCE M WITH x <- e` — named when it introduces a prefix,
    /// anonymous when the module's definitions are pulled in directly.
    Instance {
        name: Option<String>,
        module: String,
        subs: Vec<(String, Expr)>,
    },
    Assume(Expr),
    Theorem(Expr),
}

/// A declared name together with its arity; zero for a plain constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub name: String,
    pub arity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
    pub local: bool,
}

/// One `x, y \in S` group of a quantifier, function or set constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub names: Vec<String>,
    /// Absent for the unbounded forms, as in `\E x : P`.
    pub domain: Option<Expr>,
    /// True for `<<x, y>> \in S`, which destructures each element.
    pub destructure: bool,
}

impl Bound {
    pub fn mentions_next_state(&self) -> bool {
        self.domain.as_ref().is_some_and(Expr::mentions_next_state)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantKind {
    Forall,
    Exists,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExceptPath {
    Index(Expr),
    Field(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Num(i64),
    Str(String),
    Bool(bool),
    Ident(String),
    /// `x'`
    Prime(Box<Expr>),
    /// `@`, legal only inside an `EXCEPT` update.
    At,
    /// `Op(a, b)` — application of a defined operator.
    Apply(Box<Expr>, Vec<Expr>),
    /// `f[a]` — function application.
    FnApply(Box<Expr>, Vec<Expr>),
    /// `r.field`
    Field(Box<Expr>, String),
    /// `Inst!Name(args)`
    Qualified {
        instance: String,
        name: String,
        args: Vec<Expr>,
    },
    Unary(Op, Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Tuple(Vec<Expr>),
    SetEnum(Vec<Expr>),
    /// `{x \in S : P}`
    SetFilter {
        bound: Box<Bound>,
        pred: Box<Expr>,
    },
    /// `{e : x \in S, y \in T}`
    SetMap {
        expr: Box<Expr>,
        bounds: Vec<Bound>,
    },
    /// `[a |-> 1, b |-> 2]`
    Record(Vec<(String, Expr)>),
    /// `[a : S, b : T]`
    RecordSet(Vec<(String, Expr)>),
    /// `[x \in S |-> e]`
    FnDef {
        bounds: Vec<Bound>,
        body: Box<Expr>,
    },
    /// `[S -> T]`
    FnSet {
        domain: Box<Expr>,
        range: Box<Expr>,
    },
    /// `[f EXCEPT ![a] = e, !.g = e2]`
    Except {
        base: Box<Expr>,
        updates: Vec<(Vec<ExceptPath>, Expr)>,
    },
    Quant {
        kind: QuantKind,
        bounds: Vec<Bound>,
        body: Box<Expr>,
    },
    Choose {
        bound: Box<Bound>,
        body: Box<Expr>,
    },
    Let {
        defs: Vec<Def>,
        body: Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    Case {
        arms: Vec<(Expr, Expr)>,
        other: Option<Box<Expr>>,
    },
    /// `[A]_vars`
    ActionBox {
        action: Box<Expr>,
        subscript: Box<Expr>,
    },
    /// `<<A>>_vars`
    ActionAngle {
        action: Box<Expr>,
        subscript: Box<Expr>,
    },
    /// `WF_vars(A)` / `SF_vars(A)`
    Fairness {
        strong: bool,
        subscript: Box<Expr>,
        action: Box<Expr>,
    },
}

impl Expr {
    /// Does this constrain the successor state? Distinguishes an action's
    /// guard, which says when it may happen, from its effect, which says what
    /// it does — the two call for different advice when one of them fails.
    pub fn mentions_next_state(&self) -> bool {
        match self {
            Expr::Prime(_)
            | Expr::ActionBox { .. }
            | Expr::ActionAngle { .. }
            | Expr::Fairness { .. } => true,
            Expr::Unary(op, inner) => {
                *op == Op::Unchanged || *op == Op::Enabled || inner.mentions_next_state()
            }
            Expr::Binary(_, lhs, rhs) => lhs.mentions_next_state() || rhs.mentions_next_state(),
            Expr::Apply(head, args) | Expr::FnApply(head, args) => {
                head.mentions_next_state() || args.iter().any(Expr::mentions_next_state)
            }
            Expr::Field(inner, _) => inner.mentions_next_state(),
            Expr::Qualified { args, .. } => args.iter().any(Expr::mentions_next_state),
            Expr::Tuple(items) | Expr::SetEnum(items) => {
                items.iter().any(Expr::mentions_next_state)
            }
            Expr::SetFilter { bound, pred } => {
                bound.mentions_next_state() || pred.mentions_next_state()
            }
            Expr::SetMap { expr, bounds } => {
                expr.mentions_next_state() || bounds.iter().any(Bound::mentions_next_state)
            }
            Expr::Record(fields) | Expr::RecordSet(fields) => {
                fields.iter().any(|(_, v)| v.mentions_next_state())
            }
            Expr::FnSet { domain, range } => {
                domain.mentions_next_state() || range.mentions_next_state()
            }
            Expr::Except { base, updates } => {
                base.mentions_next_state()
                    || updates.iter().any(|(path, value)| {
                        value.mentions_next_state()
                            || path.iter().any(|step| match step {
                                ExceptPath::Index(e) => e.mentions_next_state(),
                                ExceptPath::Field(_) => false,
                            })
                    })
            }
            Expr::FnDef { bounds, body } | Expr::Quant { bounds, body, .. } => {
                body.mentions_next_state() || bounds.iter().any(Bound::mentions_next_state)
            }
            Expr::Choose { bound, body } => {
                bound.mentions_next_state() || body.mentions_next_state()
            }
            Expr::Let { defs, body } => {
                body.mentions_next_state() || defs.iter().any(|d| d.body.mentions_next_state())
            }
            Expr::If {
                cond,
                then,
                otherwise,
            } => {
                cond.mentions_next_state()
                    || then.mentions_next_state()
                    || otherwise.mentions_next_state()
            }
            Expr::Case { arms, other } => {
                arms.iter()
                    .any(|(g, r)| g.mentions_next_state() || r.mentions_next_state())
                    || other.as_ref().is_some_and(|o| o.mentions_next_state())
            }
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Ident(_) | Expr::At => false,
        }
    }

    pub fn conjunction(items: Vec<Expr>) -> Expr {
        Self::fold(items, Op::And)
    }

    pub fn disjunction(items: Vec<Expr>) -> Expr {
        Self::fold(items, Op::Or)
    }

    fn fold(items: Vec<Expr>, op: Op) -> Expr {
        let mut iter = items.into_iter();
        let first = iter.next().expect("junction list has at least one item");
        iter.fold(first, |acc, e| Expr::Binary(op, Box::new(acc), Box::new(e)))
    }
}
