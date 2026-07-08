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
    CallableContractModifiersSyntaxBuilder, CallableDirectivesSyntax,
    CallableDirectivesSyntaxBuilder, CallableModifiersSyntax, CallableModifiersSyntaxBuilder,
    CallableOverloadDeclarationSyntax, CallableOverloadDeclarationSyntaxBuilder,
    CallableResultClauseSyntax, CallableResultClauseSyntaxBuilder, CompilationUnitSyntax,
    CompilationUnitSyntaxBuilder, ConstantDeclarationSyntax, ConstantDeclarationSyntaxBuilder,
    ConstantModifiersSyntax, ConstantModifiersSyntaxBuilder, ConstructorMemberModifiersSyntax,
    ConstructorMemberModifiersSyntaxBuilder, CopyDirectiveSyntax, CopyDirectiveSyntaxBuilder,
    DestructorMemberDeclarationSyntax, DestructorMemberDeclarationSyntaxBuilder,
    DirectiveArgumentListSyntax, DirectiveArgumentListSyntaxBuilder, DirectiveArgumentSyntax,
    DirectiveArgumentSyntaxBuilder, EnsuresClauseSyntax, EnsuresClauseSyntaxBuilder,
    EntrypointDirectiveSyntax, EntrypointDirectiveSyntaxBuilder, ExportDeclarationSyntax,
    ExportDeclarationSyntaxBuilder, ExpressionSyntax, ExpressionSyntaxBuilder,
    FieldModifiersSyntax, FieldModifiersSyntaxBuilder, FinalizerMemberDeclarationSyntax,
    FinalizerMemberDeclarationSyntaxBuilder, FunctionDeclarationSyntax,
    FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntax, FunctionDirectivesSyntaxBuilder,
    FunctionModifiersSyntax, FunctionModifiersSyntaxBuilder, GenericArgumentListSyntax,
    GenericArgumentListSyntaxBuilder, GenericArgumentSyntax, GenericArgumentSyntaxBuilder,
    GenericConstParameterSyntax, GenericConstParameterSyntaxBuilder, GenericParameterListSyntax,
    GenericParameterListSyntaxBuilder, GenericTypeParameterSyntax,
    GenericTypeParameterSyntaxBuilder, IdentifierListItemSyntax, IdentifierListItemSyntaxBuilder,
    IdentifierListSyntax, IdentifierListSyntaxBuilder, ImplementationBodySyntax,
    ImplementationBodySyntaxBuilder, ImplementationOverloadDeclarationSyntax,
    ImplementationOverloadDeclarationSyntaxBuilder, ImplementationOverloadSubjectSyntax,
    ImplementationOverloadSubjectSyntaxBuilder, ImplementationSubjectSyntax,
    ImplementationSubjectSyntaxBuilder, ImplementationTypeMemberBindingSyntax,
    ImplementationTypeMemberBindingSyntaxBuilder, InherentImplementationDeclarationSyntax,
    InherentImplementationDeclarationSyntaxBuilder, LayoutDirectiveSyntax,
    LayoutDirectiveSyntaxBuilder, LinkDirectiveSyntax, LinkDirectiveSyntaxBuilder,
    ModuleBodySyntax, ModuleBodySyntaxBuilder, ModuleDirectivesSyntax,
    ModuleDirectivesSyntaxBuilder, ModuleModifiersSyntax, ModuleModifiersSyntaxBuilder,
    NamedTraitImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntaxBuilder,
    OverloadArmListSyntax, OverloadArmListSyntaxBuilder, OverloadArmSyntax,
    OverloadArmSyntaxBuilder, OverloadModifiersSyntax, OverloadModifiersSyntaxBuilder,
    ParameterListSyntax, ParameterListSyntaxBuilder, ParameterModifiersSyntax,
    ParameterModifiersSyntaxBuilder, ParameterSyntax, ParameterSyntaxBuilder, PathSyntax,
    PathSyntaxBuilder, PayloadFieldModifiersSyntax, PayloadFieldModifiersSyntaxBuilder,
    PredicateDeclarationSyntax, PredicateDeclarationSyntaxBuilder, PredicateModifiersSyntax,
    PredicateModifiersSyntaxBuilder, PredicateParameterListSyntax,
    PredicateParameterListSyntaxBuilder, PredicateParameterSyntax, PredicateParameterSyntaxBuilder,
    PrimaryExpressionSyntax, PrimaryExpressionSyntaxBuilder, RequiresClauseSyntax,
    RequiresClauseSyntaxBuilder, ScopeEnterMemberDeclarationSyntax,
    ScopeEnterMemberDeclarationSyntaxBuilder, ScopeExitMemberDeclarationSyntax,
    ScopeExitMemberDeclarationSyntaxBuilder, SkippedSyntax, SourceUnitModuleDeclarationSyntax,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntax, SourceUnitSyntaxBuilder,
    StructBodySyntax, StructBodySyntaxBuilder, StructDeclarationSyntax,
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
    TraitFinalizerRequirementDeclarationSyntaxBuilder, TraitModifiersSyntax,
    TraitModifiersSyntaxBuilder, TraitPredicateMemberDeclarationSyntax,
    TraitPredicateMemberDeclarationSyntaxBuilder, TraitPredicateMemberModifiersSyntax,
    TraitPredicateMemberModifiersSyntaxBuilder, TraitScopeEnterRequirementDeclarationSyntax,
    TraitScopeEnterRequirementDeclarationSyntaxBuilder, TraitScopeExitRequirementDeclarationSyntax,
    TraitScopeExitRequirementDeclarationSyntaxBuilder, TraitTypeMemberDeclarationSyntax,
    TraitTypeMemberDeclarationSyntaxBuilder, TypeAnnotationSyntax, TypeAnnotationSyntaxBuilder,
    TypeCallableMemberDeclarationSyntax, TypeCallableMemberDeclarationSyntaxBuilder,
    TypeCallableMemberModifiersSyntax, TypeCallableMemberModifiersSyntaxBuilder,
    TypeConstructorMemberDeclarationSyntax, TypeConstructorMemberDeclarationSyntaxBuilder,
    TypeDirectivesSyntax, TypeDirectivesSyntaxBuilder, TypeExpressionSyntax,
    TypeExpressionSyntaxBuilder, TypeFormArgumentListSyntax, TypeFormArgumentListSyntaxBuilder,
    TypeFormArgumentSyntax, TypeFormArgumentSyntaxBuilder, TypeModifiersSyntax,
    TypeModifiersSyntaxBuilder, TypedIdentifierSyntax, TypedIdentifierSyntaxBuilder,
    UnionBodySyntax, UnionBodySyntaxBuilder, UnionDeclarationSyntax, UnionDeclarationSyntaxBuilder,
    UnionPayloadFieldSyntax, UnionPayloadFieldSyntaxBuilder, UnionVariantDeclarationSyntax,
    UnionVariantDeclarationSyntaxBuilder, UnionVariantPayloadSyntax,
    UnionVariantPayloadSyntaxBuilder, UnnamedTraitImplementationDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntaxBuilder, UsesClauseSyntax, UsesClauseSyntaxBuilder,
    UsingDeclarationSyntax, UsingDeclarationSyntaxBuilder, VariantDirectivesSyntax,
    VariantDirectivesSyntaxBuilder, WithClauseSyntax, WithClauseSyntaxBuilder,
};
pub use text::SyntaxText;
pub use token::{SyntaxToken, SyntaxTokenPresence};
pub use tree::SyntaxTree;
pub use trivia::SyntaxTrivia;
