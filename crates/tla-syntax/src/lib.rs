//! Lexer, parser and AST for the fragment of TLA+ that specifications are
//! written in: modules, declarations, definitions and expressions.
//!
//! Temporal formulas are parsed into the AST but carry no meaning here — the
//! evaluator that consumes this crate answers questions about concrete states,
//! not about behaviours.

pub mod ast;
pub mod error;
mod lexer;
mod parser;
mod print;
pub mod token;

pub use ast::{Bound, Decl, Def, ExceptPath, Expr, Module, QuantKind, Unit};
pub use error::{Error, Result, Stage};
pub use lexer::lex;
pub use parser::{parse_expression, parse_module};
