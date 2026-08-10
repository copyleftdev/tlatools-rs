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
