//! Syntax data structures produced by the Bray parser.

#![forbid(unsafe_code)]

mod kind;
mod text;
mod token;
mod trivia;

pub use kind::SyntaxKind;
pub use token::SyntaxToken;
pub use trivia::SyntaxTrivia;
