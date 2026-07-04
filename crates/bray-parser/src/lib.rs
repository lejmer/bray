//! Lexer and parser for Bray source text.

#![forbid(unsafe_code)]

mod lexer;
mod parser;

pub use lexer::{LexerCachePolicy, LexerTokenSource};
pub use parser::{ParseResult, parse_compilation_unit};
