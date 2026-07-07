//! Syntax data structures produced by the Bray parser.

#![forbid(unsafe_code)]

mod builder;
mod green;
mod kind;
mod list;
mod node;
mod syntax;
#[cfg(test)]
mod test_support;
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
    CopyDirectiveSyntax, CopyDirectiveSyntaxBuilder, DirectiveArgumentListSyntax,
    DirectiveArgumentListSyntaxBuilder, EntrypointDirectiveSyntax,
    EntrypointDirectiveSyntaxBuilder, ExportDeclarationSyntax, ExportDeclarationSyntaxBuilder,
    FunctionDeclarationSyntax, FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntax,
    FunctionDirectivesSyntaxBuilder, FunctionModifiersSyntax, FunctionModifiersSyntaxBuilder,
    IdentifierListItemSyntax, IdentifierListItemSyntaxBuilder, IdentifierListSyntax,
    IdentifierListSyntaxBuilder, ImplementationSubjectSyntax, ImplementationSubjectSyntaxBuilder,
    InherentImplementationBodySyntax, InherentImplementationBodySyntaxBuilder,
    InherentImplementationDeclarationSyntax, InherentImplementationDeclarationSyntaxBuilder,
    LayoutDirectiveSyntax, LayoutDirectiveSyntaxBuilder, LinkDirectiveSyntax,
    LinkDirectiveSyntaxBuilder, ModuleBodySyntax, ModuleBodySyntaxBuilder, ModuleDirectivesSyntax,
    ModuleDirectivesSyntaxBuilder, ModuleModifiersSyntax, ModuleModifiersSyntaxBuilder,
    NamedTraitImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntaxBuilder,
    ParameterListSyntax, ParameterListSyntaxBuilder, ParameterModifiersSyntax,
    ParameterModifiersSyntaxBuilder, ParameterSyntax, ParameterSyntaxBuilder, PathSyntax,
    PathSyntaxBuilder, SkippedSyntax, SourceUnitModuleDeclarationSyntax,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntax, SourceUnitSyntaxBuilder,
    StructBodySyntax, StructBodySyntaxBuilder, StructDeclarationSyntax,
    StructDeclarationSyntaxBuilder, SymbolDirectiveSyntax, SymbolDirectiveSyntaxBuilder,
    TargetDirectiveSyntax, TargetDirectiveSyntaxBuilder, TestDirectiveSyntax,
    TestDirectiveSyntaxBuilder, TraitApplicationSyntax, TraitApplicationSyntaxBuilder,
    TraitBodySyntax, TraitBodySyntaxBuilder, TraitDeclarationSyntax, TraitDeclarationSyntaxBuilder,
    TraitImplementationBodySyntax, TraitImplementationBodySyntaxBuilder, TraitModifiersSyntax,
    TraitModifiersSyntaxBuilder, TypeDirectivesSyntax, TypeDirectivesSyntaxBuilder,
    TypeModifiersSyntax, TypeModifiersSyntaxBuilder, UnionBodySyntax, UnionBodySyntaxBuilder,
    UnionDeclarationSyntax, UnionDeclarationSyntaxBuilder,
    UnnamedTraitImplementationDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntaxBuilder, UsingDeclarationSyntax,
    UsingDeclarationSyntaxBuilder,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
