//! Syntax data structures produced by the Bray parser.

#![forbid(unsafe_code)]

mod builder;
mod green;
mod kind;
mod syntax;
mod text;
mod token;
mod tree;
mod trivia;

pub use kind::SyntaxKind;
pub use syntax::{
    CompilationUnitSyntax, CompilationUnitSyntaxBuilder, SkippedSyntax, SourceSyntaxNode,
    SourceUnitSyntax, SourceUnitSyntaxBuilder, SyntaxNode,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
