//! Syntax data structures produced by the Bray parser.

#![forbid(unsafe_code)]

mod builder;
mod green;
mod kind;
mod list;
mod node;
mod syntax;
mod text;
mod token;
mod tree;
mod trivia;

pub use kind::SyntaxKind;
pub use node::{SourceSyntaxNode, SyntaxNode};
pub use syntax::{
    BlockModuleDeclarationSyntax, BlockModuleDeclarationSyntaxBuilder, CompilationUnitSyntax,
    CompilationUnitSyntaxBuilder, DirectiveArgumentListSyntax, DirectiveArgumentListSyntaxBuilder,
    IdentifierListItemSyntax, IdentifierListItemSyntaxBuilder, IdentifierListSyntax,
    IdentifierListSyntaxBuilder, LinkDirectiveSyntax, LinkDirectiveSyntaxBuilder, ModuleBodySyntax,
    ModuleBodySyntaxBuilder, ModuleDirectivesSyntax, ModuleDirectivesSyntaxBuilder,
    ModuleModifiersSyntax, ModuleModifiersSyntaxBuilder, PathSyntax, PathSyntaxBuilder,
    SkippedSyntax, SourceUnitModuleDeclarationSyntax, SourceUnitModuleDeclarationSyntaxBuilder,
    SourceUnitSyntax, SourceUnitSyntaxBuilder, TargetDirectiveSyntax, TargetDirectiveSyntaxBuilder,
    TestDirectiveSyntax, TestDirectiveSyntaxBuilder,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
