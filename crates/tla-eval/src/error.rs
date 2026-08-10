use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A name the specification never declares or defines.
    Undefined(String),
    /// A value used where its type makes no sense — `Len` of an integer, a
    /// non-boolean conjunct, arithmetic on a string.
    Type(String),
    /// `x'` with no successor state supplied: the expression is an action but
    /// was evaluated as a state predicate.
    NoNextState(String),
    /// A construct that cannot be decided by looking at one state or one step:
    /// `[]P`, `<>P`, `WF_v(A)`, `ENABLED A`.
    NotGround(String),
    /// Enumeration of something that cannot be enumerated, or that is larger
    /// than the evaluator will materialize.
    Unbounded(String),
    /// A specification error the language itself forbids: `@` outside EXCEPT,
    /// wrong operator arity, a recursive definition that does not terminate.
    Malformed(String),
    Syntax(tla_syntax::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Undefined(name) => write!(f, "`{name}` is not defined"),
            Error::Type(m)
            | Error::NoNextState(m)
            | Error::NotGround(m)
            | Error::Unbounded(m)
            | Error::Malformed(m) => f.write_str(m),
            Error::Syntax(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<tla_syntax::Error> for Error {
    fn from(e: tla_syntax::Error) -> Self {
        Error::Syntax(e)
    }
}

pub(crate) fn type_error<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Type(msg.into()))
}
