//! Syntax data structures produced by the Bray parser.

#![forbid(unsafe_code)]

mod builder;
mod green;
mod kind;
mod node;
mod node_support;
mod separated;
mod syntax;
mod text;
mod token;
mod tree;
mod trivia;

pub use kind::SyntaxKind;
pub use node::{SourceSyntaxNode, SyntaxNode};
pub use syntax::{
    BlockModuleDeclarationSyntax, BlockModuleDeclarationSyntaxBuilder, CompilationUnitSyntax,
    CompilationUnitSyntaxBuilder, IdentifierListItemSyntax, IdentifierListItemSyntaxBuilder,
    IdentifierListSyntax, IdentifierListSyntaxBuilder, ModuleBodySyntax, ModuleBodySyntaxBuilder,
    ModuleModifiersSyntax, ModuleModifiersSyntaxBuilder, PathSyntax, PathSyntaxBuilder,
    SkippedSyntax, SourceUnitModuleDeclarationSyntax, SourceUnitModuleDeclarationSyntaxBuilder,
    SourceUnitSyntax, SourceUnitSyntaxBuilder,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
