//! Lexer and parser for Bray source text.

#![forbid(unsafe_code)]

mod lexer;

pub use lexer::{LexerCachePolicy, LexerTokenSource};
