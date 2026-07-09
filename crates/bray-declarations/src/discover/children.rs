use bray_syntax::{
    CallableContractDeclarationSyntax, DestructorMemberDeclarationSyntax,
    FinalizerMemberDeclarationSyntax, FunctionDeclarationSyntax, GenericConstParameterSyntax,
    GenericParameterListSyntax, GenericTypeParameterSyntax, ParameterListSyntax, ParameterSyntax,
    PredicateDeclarationSyntax, PredicateParameterListSyntax, PredicateParameterSyntax,
    ScopeEnterMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntax, SourceSyntaxNode,
    StructDeclarationSyntax, SyntaxCast, SyntaxNodeView, SyntaxToken,
    TraitCallableMemberDeclarationSyntax, TraitDeclarationSyntax,
    TraitDestructorRequirementDeclarationSyntax, TraitFinalizerRequirementDeclarationSyntax,
    TraitPredicateMemberDeclarationSyntax, TraitScopeEnterRequirementDeclarationSyntax,
    TraitScopeExitRequirementDeclarationSyntax, TypeCallableMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntax, UnionDeclarationSyntax, UnionPayloadFieldSyntax,
    UnionVariantDeclarationSyntax,
};

use super::names::identifier_declaration_name;
use super::surface::modifier_token_surface;
use super::syntax::{cast_node, discovered_declaration, discovered_declaration_with_surface};
use crate::chunk::DiscoveredDeclaration;
use crate::name::DeclarationName;
use crate::record::DeclarationKind;
use crate::surface::DeclarationSurface;

pub(super) fn declaration_children(
    view: SyntaxNodeView<'_>,
    declaration_kind: DeclarationKind,
) -> Vec<DiscoveredDeclaration> {
    match declaration_kind {
        DeclarationKind::CallableContract => {
            generic_parameter_children_from::<CallableContractDeclarationSyntax>(
                view,
                "callable contract declaration",
                CallableContractDeclarationSyntax::generic_parameter_list,
            )
        }
        DeclarationKind::Function => callable_children_from::<FunctionDeclarationSyntax>(
            view,
            "function declaration",
            FunctionDeclarationSyntax::generic_parameter_list,
            FunctionDeclarationSyntax::parameter_list,
        ),
        DeclarationKind::Predicate => predicate_children_from::<PredicateDeclarationSyntax>(
            view,
            "predicate declaration",
            PredicateDeclarationSyntax::generic_parameter_list,
            PredicateDeclarationSyntax::predicate_parameter_list,
        ),
        DeclarationKind::Struct => generic_parameter_children_from::<StructDeclarationSyntax>(
            view,
            "struct declaration",
            StructDeclarationSyntax::generic_parameter_list,
        ),
        DeclarationKind::Union => generic_parameter_children_from::<UnionDeclarationSyntax>(
            view,
            "union declaration",
            UnionDeclarationSyntax::generic_parameter_list,
        ),
        DeclarationKind::Trait => generic_parameter_children_from::<TraitDeclarationSyntax>(
            view,
            "trait declaration",
            TraitDeclarationSyntax::generic_parameter_list,
        ),
        DeclarationKind::UnionVariant => {
            let declaration =
                cast_node::<UnionVariantDeclarationSyntax>(view, "union variant declaration");

            union_variant_children(declaration)
        }
        DeclarationKind::TraitPredicateMember => {
            predicate_parameter_children_from::<TraitPredicateMemberDeclarationSyntax>(
                view,
                "trait predicate member declaration",
                TraitPredicateMemberDeclarationSyntax::predicate_parameter_list,
            )
        }
        DeclarationKind::TraitCallableMember => {
            callable_children_from::<TraitCallableMemberDeclarationSyntax>(
                view,
                "trait callable member declaration",
                TraitCallableMemberDeclarationSyntax::generic_parameter_list,
                TraitCallableMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::TraitFinalizerRequirement => {
            callable_parameter_children_from::<TraitFinalizerRequirementDeclarationSyntax>(
                view,
                "trait finalizer requirement declaration",
                TraitFinalizerRequirementDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::TraitDestructorRequirement => {
            callable_parameter_children_from::<TraitDestructorRequirementDeclarationSyntax>(
                view,
                "trait destructor requirement declaration",
                TraitDestructorRequirementDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::TraitScopeEnterRequirement => {
            callable_parameter_children_from::<TraitScopeEnterRequirementDeclarationSyntax>(
                view,
                "trait scope-enter requirement declaration",
                TraitScopeEnterRequirementDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::TraitScopeExitRequirement => {
            callable_parameter_children_from::<TraitScopeExitRequirementDeclarationSyntax>(
                view,
                "trait scope-exit requirement declaration",
                TraitScopeExitRequirementDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::TypeConstructorMember => {
            callable_parameter_children_from::<TypeConstructorMemberDeclarationSyntax>(
                view,
                "type constructor member declaration",
                TypeConstructorMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::FinalizerMember => {
            callable_parameter_children_from::<FinalizerMemberDeclarationSyntax>(
                view,
                "finalizer member declaration",
                FinalizerMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::DestructorMember => {
            callable_parameter_children_from::<DestructorMemberDeclarationSyntax>(
                view,
                "destructor member declaration",
                DestructorMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::ScopeEnterMember => {
            callable_parameter_children_from::<ScopeEnterMemberDeclarationSyntax>(
                view,
                "scope-enter member declaration",
                ScopeEnterMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::ScopeExitMember => {
            callable_parameter_children_from::<ScopeExitMemberDeclarationSyntax>(
                view,
                "scope-exit member declaration",
                ScopeExitMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::TypeCallableMember => {
            callable_children_from::<TypeCallableMemberDeclarationSyntax>(
                view,
                "type callable member declaration",
                TypeCallableMemberDeclarationSyntax::generic_parameter_list,
                TypeCallableMemberDeclarationSyntax::parameter_list,
            )
        }
        DeclarationKind::Module
        | DeclarationKind::Using
        | DeclarationKind::Export
        | DeclarationKind::Constant
        | DeclarationKind::CallableOverload
        | DeclarationKind::ImplementationOverload
        | DeclarationKind::InherentImplementation
        | DeclarationKind::UnnamedTraitImplementation
        | DeclarationKind::NamedTraitImplementation
        | DeclarationKind::StructField
        | DeclarationKind::TraitConstantMember
        | DeclarationKind::TraitTypeMember
        | DeclarationKind::ImplementationTypeMemberBinding
        | DeclarationKind::GenericTypeParameter
        | DeclarationKind::GenericConstParameter
        | DeclarationKind::CallableParameter
        | DeclarationKind::PredicateParameter
        | DeclarationKind::UnionPayloadField => Vec::new(),
    }
}

fn generic_parameter_children_from<N>(
    view: SyntaxNodeView<'_>,
    description: &'static str,
    generic_parameter_list: impl FnOnce(&N) -> Option<GenericParameterListSyntax>,
) -> Vec<DiscoveredDeclaration>
where
    N: SyntaxCast,
{
    let declaration = cast_node::<N>(view, description);

    generic_parameter_children(generic_parameter_list(&declaration))
}

fn callable_children_from<N>(
    view: SyntaxNodeView<'_>,
    description: &'static str,
    generic_parameter_list: impl FnOnce(&N) -> Option<GenericParameterListSyntax>,
    parameter_list: impl FnOnce(&N) -> ParameterListSyntax,
) -> Vec<DiscoveredDeclaration>
where
    N: SyntaxCast,
{
    let declaration = cast_node::<N>(view, description);

    callable_children(
        generic_parameter_list(&declaration),
        parameter_list(&declaration),
    )
}

fn predicate_children_from<N>(
    view: SyntaxNodeView<'_>,
    description: &'static str,
    generic_parameter_list: impl FnOnce(&N) -> Option<GenericParameterListSyntax>,
    parameter_list: impl FnOnce(&N) -> PredicateParameterListSyntax,
) -> Vec<DiscoveredDeclaration>
where
    N: SyntaxCast,
{
    let declaration = cast_node::<N>(view, description);

    predicate_children(
        generic_parameter_list(&declaration),
        parameter_list(&declaration),
    )
}

fn callable_parameter_children_from<N>(
    view: SyntaxNodeView<'_>,
    description: &'static str,
    parameter_list: impl FnOnce(&N) -> ParameterListSyntax,
) -> Vec<DiscoveredDeclaration>
where
    N: SyntaxCast,
{
    let declaration = cast_node::<N>(view, description);

    callable_parameter_children(parameter_list(&declaration))
}

fn predicate_parameter_children_from<N>(
    view: SyntaxNodeView<'_>,
    description: &'static str,
    parameter_list: impl FnOnce(&N) -> PredicateParameterListSyntax,
) -> Vec<DiscoveredDeclaration>
where
    N: SyntaxCast,
{
    let declaration = cast_node::<N>(view, description);

    predicate_parameter_children(parameter_list(&declaration))
}

fn callable_children(
    generic_parameter_list: Option<GenericParameterListSyntax>,
    parameter_list: ParameterListSyntax,
) -> Vec<DiscoveredDeclaration> {
    let mut children = generic_parameter_children(generic_parameter_list);

    children.extend(callable_parameter_children(parameter_list));

    children
}

fn predicate_children(
    generic_parameter_list: Option<GenericParameterListSyntax>,
    parameter_list: PredicateParameterListSyntax,
) -> Vec<DiscoveredDeclaration> {
    let mut children = generic_parameter_children(generic_parameter_list);

    children.extend(predicate_parameter_children(parameter_list));

    children
}

fn generic_parameter_children(
    generic_parameter_list: Option<GenericParameterListSyntax>,
) -> Vec<DiscoveredDeclaration> {
    let Some(generic_parameter_list) = generic_parameter_list else {
        return Vec::new();
    };

    let mut children = generic_parameter_list
        .generic_type_parameters()
        .map(generic_type_parameter_declaration)
        .chain(
            generic_parameter_list
                .generic_const_parameters()
                .map(generic_const_parameter_declaration),
        )
        .collect::<Vec<_>>();

    children.sort_by_key(DiscoveredDeclaration::full_range);

    children
}

fn callable_parameter_children(parameter_list: ParameterListSyntax) -> Vec<DiscoveredDeclaration> {
    parameter_list
        .parameters()
        .map(callable_parameter_declaration)
        .collect()
}

fn predicate_parameter_children(
    parameter_list: PredicateParameterListSyntax,
) -> Vec<DiscoveredDeclaration> {
    parameter_list
        .predicate_parameters()
        .map(predicate_parameter_declaration)
        .collect()
}

fn union_variant_children(
    declaration: UnionVariantDeclarationSyntax,
) -> Vec<DiscoveredDeclaration> {
    let Some(payload) = declaration.union_variant_payload() else {
        return Vec::new();
    };

    payload
        .union_payload_fields()
        .map(union_payload_field_declaration)
        .collect()
}

fn generic_type_parameter_declaration(
    parameter: GenericTypeParameterSyntax,
) -> DiscoveredDeclaration {
    identifier_node_declaration(
        DeclarationKind::GenericTypeParameter,
        &parameter,
        parameter.identifier_token(),
    )
}

fn generic_const_parameter_declaration(
    parameter: GenericConstParameterSyntax,
) -> DiscoveredDeclaration {
    identifier_node_declaration(
        DeclarationKind::GenericConstParameter,
        &parameter,
        parameter.identifier_token(),
    )
}

fn callable_parameter_declaration(parameter: ParameterSyntax) -> DiscoveredDeclaration {
    identifier_node_declaration_with_surface(
        DeclarationKind::CallableParameter,
        &parameter,
        parameter.identifier_token(),
        modifier_token_surface(parameter.parameter_modifiers().tokens()),
    )
}

fn predicate_parameter_declaration(parameter: PredicateParameterSyntax) -> DiscoveredDeclaration {
    identifier_node_declaration(
        DeclarationKind::PredicateParameter,
        &parameter,
        parameter.identifier_token(),
    )
}

fn union_payload_field_declaration(field: UnionPayloadFieldSyntax) -> DiscoveredDeclaration {
    identifier_node_declaration_with_surface(
        DeclarationKind::UnionPayloadField,
        &field,
        field.identifier_token(),
        modifier_token_surface(field.payload_field_modifiers().tokens()),
    )
}

fn identifier_node_declaration<N>(
    kind: DeclarationKind,
    node: &N,
    token: SyntaxToken,
) -> DiscoveredDeclaration
where
    N: SourceSyntaxNode,
{
    discovered_declaration(kind, identifier_node_name(node, token), node)
}

fn identifier_node_declaration_with_surface<N>(
    kind: DeclarationKind,
    node: &N,
    token: SyntaxToken,
    surface: DeclarationSurface,
) -> DiscoveredDeclaration
where
    N: SourceSyntaxNode,
{
    discovered_declaration_with_surface(kind, identifier_node_name(node, token), node, surface)
}

fn identifier_node_name<N>(node: &N, token: SyntaxToken) -> Option<DeclarationName>
where
    N: SourceSyntaxNode,
{
    identifier_declaration_name(node.source().text(), token)
}
