//! Lexer and parser for Bray source text.

#![forbid(unsafe_code)]

mod lexer;
mod parser;

pub use lexer::{LexResult, LexerCachePolicy, LexerTokenSource, lex_source_unit};
pub use parser::{ParseResult, parse_compilation_unit};
