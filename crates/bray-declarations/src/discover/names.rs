use bray_syntax::{
    CallableContractDeclarationSyntax, CallableOverloadDeclarationSyntax,
    ConstantDeclarationSyntax, DestructorMemberDeclarationSyntax, ExportDeclarationSyntax,
    FinalizerMemberDeclarationSyntax, FunctionDeclarationSyntax,
    ImplementationOverloadDeclarationSyntax, ImplementationTypeMemberBindingSyntax,
    InherentImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntax, PathSyntax,
    PredicateDeclarationSyntax, ScopeEnterMemberDeclarationSyntax,
    ScopeExitMemberDeclarationSyntax, SourceSyntaxNode, StructDeclarationSyntax,
    StructFieldDeclarationSyntax, SyntaxKind, SyntaxNodeView, SyntaxToken,
    TraitCallableMemberDeclarationSyntax, TraitConstantMemberDeclarationSyntax,
    TraitDeclarationSyntax, TraitDestructorRequirementDeclarationSyntax,
    TraitFinalizerRequirementDeclarationSyntax, TraitPredicateMemberDeclarationSyntax,
    TraitScopeEnterRequirementDeclarationSyntax, TraitScopeExitRequirementDeclarationSyntax,
    TraitTypeMemberDeclarationSyntax, TypeCallableMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntax, UnionDeclarationSyntax, UnionVariantDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntax, UsingDeclarationSyntax,
};

use super::syntax::cast_node;
use crate::name::{DeclarationName, ImplementationDeclarationName, ModulePath};
use crate::record::DeclarationKind;

pub(super) fn declaration_kind_for_syntax(kind: SyntaxKind) -> Option<DeclarationKind> {
    match kind {
        SyntaxKind::UsingDeclaration => Some(DeclarationKind::Using),
        SyntaxKind::ExportDeclaration => Some(DeclarationKind::Export),
        SyntaxKind::ConstantDeclaration => Some(DeclarationKind::Constant),
        SyntaxKind::FunctionDeclaration => Some(DeclarationKind::Function),
        SyntaxKind::PredicateDeclaration => Some(DeclarationKind::Predicate),
        SyntaxKind::CallableContractDeclaration => Some(DeclarationKind::CallableContract),
        SyntaxKind::CallableOverloadDeclaration => Some(DeclarationKind::CallableOverload),
        SyntaxKind::ImplementationOverloadDeclaration => {
            Some(DeclarationKind::ImplementationOverload)
        }
        SyntaxKind::StructDeclaration => Some(DeclarationKind::Struct),
        SyntaxKind::UnionDeclaration => Some(DeclarationKind::Union),
        SyntaxKind::TraitDeclaration => Some(DeclarationKind::Trait),
        SyntaxKind::InherentImplementationDeclaration => {
            Some(DeclarationKind::InherentImplementation)
        }
        SyntaxKind::UnnamedTraitImplementationDeclaration => {
            Some(DeclarationKind::UnnamedTraitImplementation)
        }
        SyntaxKind::NamedTraitImplementationDeclaration => {
            Some(DeclarationKind::NamedTraitImplementation)
        }
        SyntaxKind::StructFieldDeclaration => Some(DeclarationKind::StructField),
        SyntaxKind::UnionVariantDeclaration => Some(DeclarationKind::UnionVariant),
        SyntaxKind::TraitConstantMemberDeclaration => Some(DeclarationKind::TraitConstantMember),
        SyntaxKind::TraitTypeMemberDeclaration => Some(DeclarationKind::TraitTypeMember),
        SyntaxKind::TraitPredicateMemberDeclaration => Some(DeclarationKind::TraitPredicateMember),
        SyntaxKind::TraitCallableMemberDeclaration => Some(DeclarationKind::TraitCallableMember),
        SyntaxKind::TraitFinalizerRequirementDeclaration => {
            Some(DeclarationKind::TraitFinalizerRequirement)
        }
        SyntaxKind::TraitDestructorRequirementDeclaration => {
            Some(DeclarationKind::TraitDestructorRequirement)
        }
        SyntaxKind::TraitScopeEnterRequirementDeclaration => {
            Some(DeclarationKind::TraitScopeEnterRequirement)
        }
        SyntaxKind::TraitScopeExitRequirementDeclaration => {
            Some(DeclarationKind::TraitScopeExitRequirement)
        }
        SyntaxKind::ImplementationTypeMemberBinding => {
            Some(DeclarationKind::ImplementationTypeMemberBinding)
        }
        SyntaxKind::TypeConstructorMemberDeclaration => {
            Some(DeclarationKind::TypeConstructorMember)
        }
        SyntaxKind::FinalizerMemberDeclaration => Some(DeclarationKind::FinalizerMember),
        SyntaxKind::DestructorMemberDeclaration => Some(DeclarationKind::DestructorMember),
        SyntaxKind::ScopeEnterMemberDeclaration => Some(DeclarationKind::ScopeEnterMember),
        SyntaxKind::ScopeExitMemberDeclaration => Some(DeclarationKind::ScopeExitMember),
        SyntaxKind::TypeCallableMemberDeclaration => Some(DeclarationKind::TypeCallableMember),
        _ => None,
    }
}

pub(super) fn declaration_name(
    view: SyntaxNodeView<'_>,
    declaration_kind: DeclarationKind,
) -> Option<DeclarationName> {
    match declaration_kind {
        DeclarationKind::Using => {
            let declaration = cast_node::<UsingDeclarationSyntax>(view, "using declaration");

            path_declaration_name(&declaration.path())
        }
        DeclarationKind::Export => {
            let declaration = cast_node::<ExportDeclarationSyntax>(view, "export declaration");

            path_declaration_name(&declaration.path())
        }
        DeclarationKind::Constant => {
            let declaration = cast_node::<ConstantDeclarationSyntax>(view, "constant declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Function => {
            let declaration = cast_node::<FunctionDeclarationSyntax>(view, "function declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Predicate => {
            let declaration =
                cast_node::<PredicateDeclarationSyntax>(view, "predicate declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::CallableContract => {
            let declaration = cast_node::<CallableContractDeclarationSyntax>(
                view,
                "callable contract declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Struct => {
            let declaration = cast_node::<StructDeclarationSyntax>(view, "struct declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Union => {
            let declaration = cast_node::<UnionDeclarationSyntax>(view, "union declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Trait => {
            let declaration = cast_node::<TraitDeclarationSyntax>(view, "trait declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::CallableOverload => {
            let declaration = cast_node::<CallableOverloadDeclarationSyntax>(
                view,
                "callable overload declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::ImplementationOverload => {
            let declaration = cast_node::<ImplementationOverloadDeclarationSyntax>(
                view,
                "implementation overload declaration",
            );

            implementation_declaration_name(
                &declaration.implementation_overload_subject().path(),
                Some(&declaration.trait_path()),
            )
        }
        DeclarationKind::InherentImplementation => {
            let declaration = cast_node::<InherentImplementationDeclarationSyntax>(
                view,
                "inherent implementation declaration",
            );

            implementation_declaration_name(&declaration.implementation_subject().path(), None)
        }
        DeclarationKind::UnnamedTraitImplementation => {
            let declaration = cast_node::<UnnamedTraitImplementationDeclarationSyntax>(
                view,
                "unnamed trait implementation declaration",
            );

            implementation_declaration_name(
                &declaration.implementation_subject().path(),
                Some(&declaration.trait_application().path()),
            )
        }
        DeclarationKind::NamedTraitImplementation => {
            let declaration = cast_node::<NamedTraitImplementationDeclarationSyntax>(
                view,
                "named trait implementation declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::StructField => {
            let declaration =
                cast_node::<StructFieldDeclarationSyntax>(view, "struct field declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::UnionVariant => {
            let declaration =
                cast_node::<UnionVariantDeclarationSyntax>(view, "union variant declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::TraitConstantMember => {
            let declaration = cast_node::<TraitConstantMemberDeclarationSyntax>(
                view,
                "trait constant member declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::TraitTypeMember => {
            let declaration = cast_node::<TraitTypeMemberDeclarationSyntax>(
                view,
                "trait type member declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::TraitPredicateMember => {
            let declaration = cast_node::<TraitPredicateMemberDeclarationSyntax>(
                view,
                "trait predicate member declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::TraitCallableMember => {
            let declaration = cast_node::<TraitCallableMemberDeclarationSyntax>(
                view,
                "trait callable member declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::TraitFinalizerRequirement => {
            let declaration = cast_node::<TraitFinalizerRequirementDeclarationSyntax>(
                view,
                "trait finalizer requirement declaration",
            );

            keyword_declaration_name(declaration.finalize_keyword())
        }
        DeclarationKind::TraitDestructorRequirement => {
            let declaration = cast_node::<TraitDestructorRequirementDeclarationSyntax>(
                view,
                "trait destructor requirement declaration",
            );

            keyword_declaration_name(declaration.destruct_keyword())
        }
        DeclarationKind::TraitScopeEnterRequirement => {
            let declaration = cast_node::<TraitScopeEnterRequirementDeclarationSyntax>(
                view,
                "trait scope-enter requirement declaration",
            );

            keyword_declaration_name(declaration.enter_keyword())
        }
        DeclarationKind::TraitScopeExitRequirement => {
            let declaration = cast_node::<TraitScopeExitRequirementDeclarationSyntax>(
                view,
                "trait scope-exit requirement declaration",
            );

            keyword_declaration_name(declaration.exit_keyword())
        }
        DeclarationKind::ImplementationTypeMemberBinding => {
            let declaration = cast_node::<ImplementationTypeMemberBindingSyntax>(
                view,
                "implementation type member binding",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::TypeConstructorMember => {
            let declaration = cast_node::<TypeConstructorMemberDeclarationSyntax>(
                view,
                "type constructor member declaration",
            );

            optional_identifier_declaration_name(
                view.source().text(),
                declaration.identifier_token(),
            )
            .or_else(|| keyword_declaration_name(declaration.construct_keyword()))
        }
        DeclarationKind::FinalizerMember => {
            let declaration =
                cast_node::<FinalizerMemberDeclarationSyntax>(view, "finalizer member declaration");

            keyword_declaration_name(declaration.finalize_keyword())
        }
        DeclarationKind::DestructorMember => {
            let declaration = cast_node::<DestructorMemberDeclarationSyntax>(
                view,
                "destructor member declaration",
            );

            keyword_declaration_name(declaration.destruct_keyword())
        }
        DeclarationKind::ScopeEnterMember => {
            let declaration = cast_node::<ScopeEnterMemberDeclarationSyntax>(
                view,
                "scope-enter member declaration",
            );

            keyword_declaration_name(declaration.enter_keyword())
        }
        DeclarationKind::ScopeExitMember => {
            let declaration = cast_node::<ScopeExitMemberDeclarationSyntax>(
                view,
                "scope-exit member declaration",
            );

            keyword_declaration_name(declaration.exit_keyword())
        }
        DeclarationKind::TypeCallableMember => {
            let declaration = cast_node::<TypeCallableMemberDeclarationSyntax>(
                view,
                "type callable member declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Module
        | DeclarationKind::GenericTypeParameter
        | DeclarationKind::GenericConstParameter
        | DeclarationKind::CallableParameter
        | DeclarationKind::PredicateParameter
        | DeclarationKind::UnionPayloadField => None,
    }
}

fn path_declaration_name(path: &PathSyntax) -> Option<DeclarationName> {
    let path = path_from_syntax(path);

    if path.is_empty() {
        None
    } else {
        Some(DeclarationName::Path(path))
    }
}

pub(super) fn identifier_declaration_name(
    source_text: &str,
    token: SyntaxToken,
) -> Option<DeclarationName> {
    token_text(source_text, token).map(DeclarationName::Identifier)
}

fn optional_identifier_declaration_name(
    source_text: &str,
    token: Option<SyntaxToken>,
) -> Option<DeclarationName> {
    token.and_then(|token| identifier_declaration_name(source_text, token))
}

fn keyword_declaration_name(token: SyntaxToken) -> Option<DeclarationName> {
    if token.is_missing() {
        None
    } else {
        Some(DeclarationName::Keyword(token.kind()))
    }
}

fn implementation_declaration_name(
    subject_path: &PathSyntax,
    trait_path: Option<&PathSyntax>,
) -> Option<DeclarationName> {
    let subject = path_from_syntax(subject_path);

    let trait_path = trait_path
        .map(path_from_syntax)
        .filter(|path| !path.is_empty());

    if subject.is_empty() && trait_path.is_none() {
        return None;
    }

    Some(DeclarationName::Implementation(
        ImplementationDeclarationName::new(subject, trait_path),
    ))
}

pub(super) fn path_from_syntax(path: &PathSyntax) -> ModulePath {
    path_from_identifier_tokens(path.source().text(), path.identifier_tokens())
}

fn path_from_identifier_tokens(
    source_text: &str,
    tokens: impl IntoIterator<Item = SyntaxToken>,
) -> ModulePath {
    ModulePath::new(
        tokens
            .into_iter()
            .filter_map(|token| token_text(source_text, token)),
    )
}

fn token_text(source_text: &str, token: SyntaxToken) -> Option<String> {
    let text = token.text(source_text)?;

    if text.is_empty() {
        return None;
    }

    Some(text.to_owned())
}
