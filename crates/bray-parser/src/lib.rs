//! Lexer and parser for Bray source text.

#![forbid(unsafe_code)]

mod cursor;
mod diagnostic;
mod lexer;
mod parser;

#[cfg(test)]
mod test_support;

pub use lexer::{LexResult, LexerCachePolicy, LexerTokenSource, lex_source_unit};
pub use parser::{
    SourceUnitSyntaxResult, SyntaxTreeResult, parse_compilation_unit, parse_source_unit,
};
