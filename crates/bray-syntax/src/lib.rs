//! Syntax data structures produced by the Bray parser.

#![forbid(unsafe_code)]

mod kind;
mod syntax;
mod token;
mod trivia;

pub use kind::SyntaxKind;
pub use syntax::{
    CompilationUnitSyntax, CompilationUnitSyntaxBuilder, SourceSyntaxNode, SourceUnitSyntax,
    SourceUnitSyntaxBuilder, SyntaxNode, SyntaxText, SyntaxTree,
};
pub use token::SyntaxToken;
pub use trivia::SyntaxTrivia;
