use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub stage: Stage,
    pub message: String,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Lex,
    Parse,
}

impl Error {
    pub fn lex(message: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            stage: Stage::Lex,
            message: message.into(),
            line,
            col,
        }
    }

    pub fn parse(message: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            stage: Stage::Parse,
            message: message.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stage = match self.stage {
            Stage::Lex => "lex",
            Stage::Parse => "parse",
        };
        write!(
            f,
            "{stage} error at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}

impl std::error::Error for Error {}
