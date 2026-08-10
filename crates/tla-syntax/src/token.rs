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
    Lambda,
    /// A keyword that only appears inside a TLAPS proof.
    Proof,
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
            "INSTANCE" => Self::Instance,
            "WITH" => Self::With,
            "LOCAL" => Self::Local,
            "RECURSIVE" => Self::Recursive,
            "LAMBDA" => Self::Lambda,
            "THEOREM" | "LEMMA" | "COROLLARY" | "PROPOSITION" => Self::Theorem,
            "PROOF" | "BY" | "OBVIOUS" | "OMITTED" | "QED" | "DEF" | "DEFS" | "DEFINE"
            | "SUFFICES" | "PICK" | "WITNESS" | "HAVE" | "TAKE" | "USE" | "HIDE" | "PROVE"
            | "NEW" | "ONLY" => Self::Proof,
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
    /// `P ~> Q`: temporal leads-to.
    LeadsTo,
    /// `\AA x : F` and `\EE x : F`: quantification over a hidden variable.
    TemporalForall,
    TemporalExists,
    /// An operator the language reserves a symbol and a precedence for but
    /// gives no meaning to. Every one of these exists to be defined by a
    /// specification; `\prec`, `\oplus` and `&` are all of them.
    User(&'static str),
}

/// The symbols TLA+ sets aside for specifications to define, with the
/// precedence the language fixes for each. Taken from the operator table in
/// *Specifying Systems*; the left binding power is used, which is what matters
/// for reading an expression back the way it was written.
pub(crate) const USER_OPERATORS: &[(&str, u8)] = &[
    ("\\prec", 5),
    ("\\preceq", 5),
    ("\\succ", 5),
    ("\\succeq", 5),
    ("\\sqsubset", 5),
    ("\\sqsubseteq", 5),
    ("\\sqsupset", 5),
    ("\\sqsupseteq", 5),
    ("\\subset", 5),
    ("\\supset", 5),
    ("\\ll", 5),
    ("\\gg", 5),
    ("\\sim", 5),
    ("\\simeq", 5),
    ("\\approx", 5),
    ("\\asymp", 5),
    ("\\cong", 5),
    ("\\doteq", 5),
    ("\\propto", 5),
    ("\\cdot", 5),
    ("|-", 5),
    ("-|", 5),
    ("|=", 5),
    ("=|", 5),
    ("::=", 5),
    ("<:", 7),
    (":=", 5),
    ("\\sqcap", 9),
    ("\\sqcup", 9),
    ("\\uplus", 9),
    ("\\oplus", 10),
    ("\\ominus", 11),
    ("(+)", 10),
    ("(-)", 11),
    ("(.)", 13),
    ("(/)", 13),
    ("(\\X)", 13),
    ("\\odot", 13),
    ("\\oslash", 13),
    ("\\otimes", 13),
    ("\\star", 13),
    ("\\bullet", 13),
    ("\\bigcirc", 13),
    ("\\wr", 14),
    ("&", 13),
    ("&&", 13),
    ("|", 10),
    ("||", 10),
    ("$", 9),
    ("$$", 9),
    ("??", 9),
    ("%%", 11),
    ("##", 9),
    ("!!", 9),
    ("^^", 14),
    ("++", 10),
    ("**", 13),
    ("//", 13),
    ("/", 13),
    ("^+", 15),
    ("^*", 15),
    ("^#", 15),
    ("-+->", 2),
];

pub(crate) fn user_operator(symbol: &str) -> Option<Op> {
    USER_OPERATORS
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(name, _)| Op::User(name))
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
            Self::Equiv | Self::LeadsTo => 2,
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
            Self::User(_) if self.is_postfix() => return None,
            Self::User(symbol) => {
                return USER_OPERATORS
                    .iter()
                    .find(|(name, _)| *name == symbol)
                    .map(|(_, prec)| *prec);
            }
            _ => return None,
        })
    }

    /// `s^+`, `s^*` and `s^#` follow their operand rather than sitting
    /// between two, so they are never infix.
    pub fn is_postfix(self) -> bool {
        matches!(self, Self::User("^+" | "^*" | "^#"))
    }

    pub fn is_right_assoc(self) -> bool {
        matches!(self, Self::Implies | Self::Pow)
    }

    /// How the operator is written. Where TLA+ offers several spellings the
    /// ASCII one is chosen, so printed output can be re-read by the parser.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Implies => "=>",
            Self::Equiv => "<=>",
            Self::Or => "\\/",
            Self::And => "/\\",
            Self::Eq => "=",
            Self::Neq => "#",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::Le => "<=",
            Self::Ge => ">=",
            Self::In => "\\in",
            Self::NotIn => "\\notin",
            Self::Subseteq => "\\subseteq",
            Self::Supseteq => "\\supseteq",
            Self::AtAt => "@@",
            Self::OneTo => ":>",
            Self::Cup => "\\cup",
            Self::Cap => "\\cap",
            Self::SetMinus => "\\",
            Self::DotDot => "..",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Times => "*",
            Self::Div => "\\div",
            Self::Mod => "%",
            Self::Cartesian => "\\X",
            Self::Concat => "\\o",
            Self::Pow => "^",
            Self::Not => "~",
            Self::Always => "[]",
            Self::Eventually => "<>",
            Self::Forall => "\\A",
            Self::Exists => "\\E",
            Self::Domain => "DOMAIN",
            Self::Subset => "SUBSET",
            Self::BigUnion => "UNION",
            Self::Enabled => "ENABLED",
            Self::Unchanged => "UNCHANGED",
            Self::LeadsTo => "~>",
            Self::TemporalForall => "\\AA",
            Self::TemporalExists => "\\EE",
            Self::User(symbol) => symbol,
        }
    }

    /// How tightly the operator holds its operand when used as a prefix, and
    /// so how tightly it binds as a node in printed output.
    pub fn prefix_prec(self) -> u8 {
        match self {
            Self::Minus => 11,
            Self::Domain | Self::Subset | Self::BigUnion => 9,
            _ => 5,
        }
    }

    /// True for the word-shaped prefix operators, which need a space before
    /// their operand where the symbolic ones do not.
    pub fn is_word(self) -> bool {
        matches!(
            self,
            Self::Domain | Self::Subset | Self::BigUnion | Self::Enabled | Self::Unchanged
        )
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
    /// The `::` of a labelled expression.
    ColonColon,
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
