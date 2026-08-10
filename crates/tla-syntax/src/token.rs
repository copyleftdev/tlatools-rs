#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    Module,
    Extends,
    Constant,
    Variable,
    Let,
    In,
    If,
    Then,
    Else,
    Choose,
    Case,
    Other,
    Assume,
    Theorem,
    Instance,
    With,
    Local,
    Recursive,
    Except,
    True,
    False,
    Domain,
    Subset,
    Union,
    Enabled,
    Unchanged,
}

impl Kw {
    pub fn lookup(word: &str) -> Option<Self> {
        Some(match word {
            "MODULE" => Self::Module,
            "EXTENDS" => Self::Extends,
            "CONSTANT" | "CONSTANTS" => Self::Constant,
            "VARIABLE" | "VARIABLES" => Self::Variable,
            "LET" => Self::Let,
            "IN" => Self::In,
            "IF" => Self::If,
            "THEN" => Self::Then,
            "ELSE" => Self::Else,
            "CHOOSE" => Self::Choose,
            "CASE" => Self::Case,
            "OTHER" => Self::Other,
            "ASSUME" | "ASSUMPTION" => Self::Assume,
            "THEOREM" => Self::Theorem,
            "INSTANCE" => Self::Instance,
            "WITH" => Self::With,
            "LOCAL" => Self::Local,
            "RECURSIVE" => Self::Recursive,
            "EXCEPT" => Self::Except,
            "TRUE" => Self::True,
            "FALSE" => Self::False,
            "DOMAIN" => Self::Domain,
            "SUBSET" => Self::Subset,
            "UNION" => Self::Union,
            "ENABLED" => Self::Enabled,
            "UNCHANGED" => Self::Unchanged,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Implies,
    Equiv,
    Or,
    And,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    In,
    NotIn,
    Subseteq,
    Supseteq,
    AtAt,
    OneTo,
    Cup,
    Cap,
    SetMinus,
    DotDot,
    Plus,
    Minus,
    Times,
    Div,
    Mod,
    Cartesian,
    Concat,
    Pow,
    Not,
    Always,
    Eventually,
    Forall,
    Exists,
    Domain,
    Subset,
    BigUnion,
    Enabled,
    Unchanged,
}

impl Op {
    /// Binding power as an infix operator; `None` for prefix-only operators.
    ///
    /// Ordering follows TLA+'s table where it matters for these specs: `\cup`
    /// binds tighter than `=`, and `:>` tighter than `@@`, so `a \cup {b} = c`
    /// and `("k" :> v) @@ rest` parse without parentheses.
    pub fn infix_prec(self) -> Option<u8> {
        Some(match self {
            Self::Implies => 1,
            Self::Equiv => 2,
            Self::Or => 3,
            Self::And => 4,
            Self::Eq
            | Self::Neq
            | Self::Lt
            | Self::Gt
            | Self::Le
            | Self::Ge
            | Self::In
            | Self::NotIn
            | Self::Subseteq
            | Self::Supseteq => 5,
            Self::AtAt => 6,
            Self::OneTo => 7,
            Self::Cup | Self::Cap | Self::SetMinus => 8,
            Self::DotDot => 9,
            Self::Plus | Self::Minus => 10,
            Self::Times | Self::Div | Self::Mod | Self::Cartesian => 11,
            Self::Concat => 12,
            Self::Pow => 13,
            _ => return None,
        })
    }

    pub fn is_right_assoc(self) -> bool {
        matches!(self, Self::Implies | Self::Pow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Num(i64),
    Str(String),
    Kw(Kw),
    Op(Op),
    /// `WF_vars` / `SF_vars`, which lex as one word but mean an operator
    /// applied to a subscript.
    Fair {
        strong: bool,
        subscript: String,
    },
    ModuleEnd,
    Separator,
    DefEq,
    LParen,
    RParen,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    LTup,
    RTup,
    Comma,
    Colon,
    Dot,
    Bang,
    /// The `@` of an `EXCEPT` update, standing for the value being replaced.
    At,
    Underscore,
    Prime,
    MapsTo,
    Arrow,
    Gets,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
}
