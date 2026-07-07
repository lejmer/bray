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
    AbiDirectiveSyntax, AbiDirectiveSyntaxBuilder, BlockModuleDeclarationSyntax,
    BlockModuleDeclarationSyntaxBuilder, CallableBodyBlockExpressionSyntax,
    CallableBodyBlockExpressionSyntaxBuilder, CallableResultClauseSyntax,
    CallableResultClauseSyntaxBuilder, CompilationUnitSyntax, CompilationUnitSyntaxBuilder,
    DirectiveArgumentListSyntax, DirectiveArgumentListSyntaxBuilder, EntrypointDirectiveSyntax,
    EntrypointDirectiveSyntaxBuilder, ExportDeclarationSyntax, ExportDeclarationSyntaxBuilder,
    FunctionDeclarationSyntax, FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntax,
    FunctionDirectivesSyntaxBuilder, FunctionModifiersSyntax, FunctionModifiersSyntaxBuilder,
    IdentifierListItemSyntax, IdentifierListItemSyntaxBuilder, IdentifierListSyntax,
    IdentifierListSyntaxBuilder, LinkDirectiveSyntax, LinkDirectiveSyntaxBuilder, ModuleBodySyntax,
    ModuleBodySyntaxBuilder, ModuleDirectivesSyntax, ModuleDirectivesSyntaxBuilder,
    ModuleModifiersSyntax, ModuleModifiersSyntaxBuilder, ParameterListSyntax,
    ParameterListSyntaxBuilder, ParameterModifiersSyntax, ParameterModifiersSyntaxBuilder,
    ParameterSyntax, ParameterSyntaxBuilder, PathSyntax, PathSyntaxBuilder, SkippedSyntax,
    SourceUnitModuleDeclarationSyntax, SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntax,
    SourceUnitSyntaxBuilder, SymbolDirectiveSyntax, SymbolDirectiveSyntaxBuilder,
    TargetDirectiveSyntax, TargetDirectiveSyntaxBuilder, TestDirectiveSyntax,
    TestDirectiveSyntaxBuilder, UsingDeclarationSyntax, UsingDeclarationSyntaxBuilder,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
