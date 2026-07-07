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
    AbiDirectiveSyntax, AbiDirectiveSyntaxBuilder, AsyncCapableLifecycleMemberModifiersSyntax,
    AsyncCapableLifecycleMemberModifiersSyntaxBuilder, BlockModuleDeclarationSyntax,
    BlockModuleDeclarationSyntaxBuilder, CallableBodyBlockExpressionSyntax,
    CallableBodyBlockExpressionSyntaxBuilder, CallableContractDeclarationSyntax,
    CallableContractDeclarationSyntaxBuilder, CallableContractModifiersSyntax,
    CallableContractModifiersSyntaxBuilder, CallableResultClauseSyntax,
    CallableResultClauseSyntaxBuilder, CompilationUnitSyntax, CompilationUnitSyntaxBuilder,
    ConstantDeclarationSyntax, ConstantDeclarationSyntaxBuilder, ConstantModifiersSyntax,
    ConstantModifiersSyntaxBuilder, ConstructorMemberModifiersSyntax,
    ConstructorMemberModifiersSyntaxBuilder, CopyDirectiveSyntax, CopyDirectiveSyntaxBuilder,
    DestructorMemberDeclarationSyntax, DestructorMemberDeclarationSyntaxBuilder,
    DirectiveArgumentListSyntax, DirectiveArgumentListSyntaxBuilder, EntrypointDirectiveSyntax,
    EntrypointDirectiveSyntaxBuilder, ExportDeclarationSyntax, ExportDeclarationSyntaxBuilder,
    FieldModifiersSyntax, FieldModifiersSyntaxBuilder, FinalizerMemberDeclarationSyntax,
    FinalizerMemberDeclarationSyntaxBuilder, FunctionDeclarationSyntax,
    FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntax, FunctionDirectivesSyntaxBuilder,
    FunctionModifiersSyntax, FunctionModifiersSyntaxBuilder, IdentifierListItemSyntax,
    IdentifierListItemSyntaxBuilder, IdentifierListSyntax, IdentifierListSyntaxBuilder,
    ImplementationSubjectSyntax, ImplementationSubjectSyntaxBuilder,
    InherentImplementationBodySyntax, InherentImplementationBodySyntaxBuilder,
    InherentImplementationDeclarationSyntax, InherentImplementationDeclarationSyntaxBuilder,
    LayoutDirectiveSyntax, LayoutDirectiveSyntaxBuilder, LinkDirectiveSyntax,
    LinkDirectiveSyntaxBuilder, ModuleBodySyntax, ModuleBodySyntaxBuilder, ModuleDirectivesSyntax,
    ModuleDirectivesSyntaxBuilder, ModuleModifiersSyntax, ModuleModifiersSyntaxBuilder,
    NamedTraitImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntaxBuilder,
    ParameterListSyntax, ParameterListSyntaxBuilder, ParameterModifiersSyntax,
    ParameterModifiersSyntaxBuilder, ParameterSyntax, ParameterSyntaxBuilder, PathSyntax,
    PathSyntaxBuilder, PayloadFieldModifiersSyntax, PayloadFieldModifiersSyntaxBuilder,
    PredicateDeclarationSyntax, PredicateDeclarationSyntaxBuilder, PredicateModifiersSyntax,
    PredicateModifiersSyntaxBuilder, PredicateParameterListSyntax,
    PredicateParameterListSyntaxBuilder, PredicateParameterSyntax, PredicateParameterSyntaxBuilder,
    ScopeEnterMemberDeclarationSyntax, ScopeEnterMemberDeclarationSyntaxBuilder,
    ScopeExitMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntaxBuilder, SkippedSyntax,
    SourceUnitModuleDeclarationSyntax, SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntax,
    SourceUnitSyntaxBuilder, StructBodySyntax, StructBodySyntaxBuilder, StructDeclarationSyntax,
    StructDeclarationSyntaxBuilder, StructFieldDeclarationSyntax,
    StructFieldDeclarationSyntaxBuilder, SymbolDirectiveSyntax, SymbolDirectiveSyntaxBuilder,
    SyncLifecycleMemberModifiersSyntax, SyncLifecycleMemberModifiersSyntaxBuilder,
    TagDirectiveSyntax, TagDirectiveSyntaxBuilder, TargetDirectiveSyntax,
    TargetDirectiveSyntaxBuilder, TestDirectiveSyntax, TestDirectiveSyntaxBuilder,
    TraitApplicationSyntax, TraitApplicationSyntaxBuilder, TraitBodySyntax, TraitBodySyntaxBuilder,
    TraitCallableMemberDeclarationSyntax, TraitCallableMemberDeclarationSyntaxBuilder,
    TraitCallableMemberModifiersSyntax, TraitCallableMemberModifiersSyntaxBuilder,
    TraitConstantMemberDeclarationSyntax, TraitConstantMemberDeclarationSyntaxBuilder,
    TraitDeclarationSyntax, TraitDeclarationSyntaxBuilder,
    TraitDestructorRequirementDeclarationSyntax,
    TraitDestructorRequirementDeclarationSyntaxBuilder, TraitFinalizerRequirementDeclarationSyntax,
    TraitFinalizerRequirementDeclarationSyntaxBuilder, TraitImplementationBodySyntax,
    TraitImplementationBodySyntaxBuilder, TraitImplementationConstantMemberDefinitionSyntax,
    TraitImplementationConstantMemberDefinitionSyntaxBuilder,
    TraitImplementationScopeEnterMemberDeclarationSyntax,
    TraitImplementationScopeEnterMemberDeclarationSyntaxBuilder,
    TraitImplementationScopeExitMemberDeclarationSyntax,
    TraitImplementationScopeExitMemberDeclarationSyntaxBuilder, TraitModifiersSyntax,
    TraitModifiersSyntaxBuilder, TraitScopeEnterRequirementDeclarationSyntax,
    TraitScopeEnterRequirementDeclarationSyntaxBuilder, TraitScopeExitRequirementDeclarationSyntax,
    TraitScopeExitRequirementDeclarationSyntaxBuilder, TypeCallableMemberDeclarationSyntax,
    TypeCallableMemberDeclarationSyntaxBuilder, TypeCallableMemberModifiersSyntax,
    TypeCallableMemberModifiersSyntaxBuilder, TypeConstructorMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntaxBuilder, TypeDirectivesSyntax,
    TypeDirectivesSyntaxBuilder, TypeModifiersSyntax, TypeModifiersSyntaxBuilder, UnionBodySyntax,
    UnionBodySyntaxBuilder, UnionDeclarationSyntax, UnionDeclarationSyntaxBuilder,
    UnionPayloadFieldSyntax, UnionPayloadFieldSyntaxBuilder, UnionVariantDeclarationSyntax,
    UnionVariantDeclarationSyntaxBuilder, UnionVariantPayloadSyntax,
    UnionVariantPayloadSyntaxBuilder, UnnamedTraitImplementationDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntaxBuilder, UsingDeclarationSyntax,
    UsingDeclarationSyntaxBuilder, VariantDirectivesSyntax, VariantDirectivesSyntaxBuilder,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
