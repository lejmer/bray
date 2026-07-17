use bray_syntax::{
    BlockModuleDeclarationSyntax, CallableContractDeclarationSyntax,
    CallableOverloadDeclarationSyntax, ConstantDeclarationSyntax,
    DestructorMemberDeclarationSyntax, FinalizerMemberDeclarationSyntax, FunctionDeclarationSyntax,
    ImplementationOverloadDeclarationSyntax, InherentImplementationDeclarationSyntax,
    NamedTraitImplementationDeclarationSyntax, PredicateDeclarationSyntax,
    PredicateParameterSyntax, ScopeEnterMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntax,
    SourceSyntaxNode, SourceUnitModuleDeclarationSyntax, StructDeclarationSyntax,
    StructFieldDeclarationSyntax, SyntaxKind, SyntaxNodeView, SyntaxToken,
    TraitCallableMemberDeclarationSyntax, TraitDeclarationSyntax,
    TraitDestructorRequirementDeclarationSyntax, TraitFinalizerRequirementDeclarationSyntax,
    TraitPredicateMemberDeclarationSyntax, TraitScopeEnterRequirementDeclarationSyntax,
    TraitScopeExitRequirementDeclarationSyntax, TypeCallableMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntax, UnionDeclarationSyntax, UnionPayloadFieldSyntax,
    UnionVariantDeclarationSyntax, UnnamedTraitImplementationDeclarationSyntax,
};

use super::syntax::cast_node;
use crate::record::DeclarationKind;
use crate::surface::{DeclarationSurface, SyntaxAnchor};

pub(super) fn module_surface(view: SyntaxNodeView<'_>) -> DeclarationSurface {
    match view.kind() {
        SyntaxKind::SourceUnitModuleDeclaration => {
            let declaration =
                cast_node::<SourceUnitModuleDeclarationSyntax>(view, "source-unit module");

            module_declaration_surface(
                declaration.module_modifiers().tokens(),
                module_directives(declaration.module_directives()),
            )
        }
        SyntaxKind::BlockModuleDeclaration => {
            let declaration = cast_node::<BlockModuleDeclarationSyntax>(view, "block module");

            module_declaration_surface(
                declaration.module_modifiers().tokens(),
                module_directives(declaration.module_directives()),
            )
        }
        _ => panic!("module surface view must be a module declaration"),
    }
}

pub(super) fn declaration_surface(
    view: SyntaxNodeView<'_>,
    declaration_kind: DeclarationKind,
) -> DeclarationSurface {
    match declaration_kind {
        DeclarationKind::Constant => {
            let declaration = cast_node::<ConstantDeclarationSyntax>(view, "constant declaration");

            modifier_token_surface(declaration.constant_modifiers().tokens())
        }
        DeclarationKind::Function => {
            let declaration = cast_node::<FunctionDeclarationSyntax>(view, "function declaration");

            declaration_surface_from_parts(
                declaration.function_modifiers().tokens(),
                function_directives(declaration.function_directives()),
                [],
                contract_clauses(
                    declaration.requires_clauses(),
                    declaration.ensures_clauses(),
                    declaration.with_clauses(),
                    declaration.uses_clauses(),
                ),
            )
        }
        DeclarationKind::Predicate => {
            let declaration =
                cast_node::<PredicateDeclarationSyntax>(view, "predicate declaration");

            modifier_token_surface(declaration.predicate_modifiers().tokens())
        }
        DeclarationKind::CallableContract => {
            let declaration = cast_node::<CallableContractDeclarationSyntax>(
                view,
                "callable contract declaration",
            );

            declaration_surface_from_parts(
                declaration.callable_contract_modifiers().tokens(),
                [],
                syntax_anchors(declaration.with_clauses()),
                [],
            )
        }
        DeclarationKind::CallableOverload => {
            let declaration = cast_node::<CallableOverloadDeclarationSyntax>(
                view,
                "callable overload declaration",
            );

            overload_surface(
                modifier_token_surface(declaration.overload_modifiers().tokens()),
                declaration.overload_arm_list(),
            )
        }
        DeclarationKind::ImplementationOverload => {
            let declaration = cast_node::<ImplementationOverloadDeclarationSyntax>(
                view,
                "implementation overload declaration",
            );

            overload_surface(
                modifier_token_surface(declaration.overload_modifiers().tokens()),
                declaration.overload_arm_list(),
            )
        }
        DeclarationKind::Struct => {
            let declaration = cast_node::<StructDeclarationSyntax>(view, "struct declaration");

            declaration_surface_from_parts(
                declaration.type_modifiers().tokens(),
                type_directives(declaration.type_directives()),
                syntax_anchors(declaration.with_clauses()),
                [],
            )
        }
        DeclarationKind::Union => {
            let declaration = cast_node::<UnionDeclarationSyntax>(view, "union declaration");

            declaration_surface_from_parts(
                declaration.type_modifiers().tokens(),
                type_directives(declaration.type_directives()),
                syntax_anchors(declaration.with_clauses()),
                [],
            )
        }
        DeclarationKind::Trait => {
            let declaration = cast_node::<TraitDeclarationSyntax>(view, "trait declaration");

            declaration_surface_from_parts(
                declaration.trait_modifiers().tokens(),
                [],
                syntax_anchors(declaration.with_clauses()),
                [],
            )
        }
        DeclarationKind::InherentImplementation => {
            let declaration = cast_node::<InherentImplementationDeclarationSyntax>(
                view,
                "inherent implementation declaration",
            );

            constraint_surface(declaration.with_clauses())
        }
        DeclarationKind::UnnamedTraitImplementation => {
            let declaration = cast_node::<UnnamedTraitImplementationDeclarationSyntax>(
                view,
                "unnamed trait implementation declaration",
            );

            constraint_surface(declaration.with_clauses())
        }
        DeclarationKind::NamedTraitImplementation => {
            let declaration = cast_node::<NamedTraitImplementationDeclarationSyntax>(
                view,
                "named trait implementation declaration",
            );

            constraint_surface(declaration.with_clauses())
        }
        DeclarationKind::StructField => {
            let declaration =
                cast_node::<StructFieldDeclarationSyntax>(view, "struct field declaration");

            runtime_default_surface(
                modifier_token_surface(declaration.field_modifiers().tokens()),
                &declaration,
                declaration.equals_token(),
                declaration.expression(),
            )
        }
        DeclarationKind::UnionVariant => {
            let declaration =
                cast_node::<UnionVariantDeclarationSyntax>(view, "union variant declaration");

            declaration_surface_from_parts(
                [],
                variant_directives(declaration.variant_directives()),
                [],
                [],
            )
        }
        DeclarationKind::TraitPredicateMember => {
            let declaration = cast_node::<TraitPredicateMemberDeclarationSyntax>(
                view,
                "trait predicate member declaration",
            );

            modifier_token_surface(declaration.trait_predicate_member_modifiers().tokens())
        }
        DeclarationKind::TraitCallableMember => {
            let declaration = cast_node::<TraitCallableMemberDeclarationSyntax>(
                view,
                "trait callable member declaration",
            );

            declaration_surface_from_parts(
                declaration.trait_callable_member_modifiers().tokens(),
                [],
                [],
                contract_clauses(
                    declaration.requires_clauses(),
                    declaration.ensures_clauses(),
                    declaration.with_clauses(),
                    declaration.uses_clauses(),
                ),
            )
        }
        DeclarationKind::TraitFinalizerRequirement => {
            let declaration = cast_node::<TraitFinalizerRequirementDeclarationSyntax>(
                view,
                "trait finalizer requirement declaration",
            );

            lifecycle_surface(
                declaration
                    .async_capable_lifecycle_member_modifiers()
                    .tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::TraitDestructorRequirement => {
            let declaration = cast_node::<TraitDestructorRequirementDeclarationSyntax>(
                view,
                "trait destructor requirement declaration",
            );

            lifecycle_surface(
                declaration.sync_lifecycle_member_modifiers().tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::TraitScopeEnterRequirement => {
            let declaration = cast_node::<TraitScopeEnterRequirementDeclarationSyntax>(
                view,
                "trait scope-enter requirement declaration",
            );

            lifecycle_surface(
                declaration.scope_enter_member_modifiers().tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::TraitScopeExitRequirement => {
            let declaration = cast_node::<TraitScopeExitRequirementDeclarationSyntax>(
                view,
                "trait scope-exit requirement declaration",
            );

            lifecycle_surface(
                declaration
                    .async_capable_lifecycle_member_modifiers()
                    .tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::TypeConstructorMember => {
            let declaration = cast_node::<TypeConstructorMemberDeclarationSyntax>(
                view,
                "type constructor member declaration",
            );

            declaration_surface_from_parts(
                declaration.constructor_member_modifiers().tokens(),
                [],
                [],
                contract_clauses(
                    declaration.requires_clauses(),
                    declaration.ensures_clauses(),
                    declaration.with_clauses(),
                    declaration.uses_clauses(),
                ),
            )
        }
        DeclarationKind::FinalizerMember => {
            let declaration =
                cast_node::<FinalizerMemberDeclarationSyntax>(view, "finalizer member declaration");

            lifecycle_surface(
                declaration
                    .async_capable_lifecycle_member_modifiers()
                    .tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::DestructorMember => {
            let declaration = cast_node::<DestructorMemberDeclarationSyntax>(
                view,
                "destructor member declaration",
            );

            lifecycle_surface(
                declaration.sync_lifecycle_member_modifiers().tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::ScopeEnterMember => {
            let declaration = cast_node::<ScopeEnterMemberDeclarationSyntax>(
                view,
                "scope-enter member declaration",
            );

            lifecycle_surface(
                declaration.scope_enter_member_modifiers().tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::ScopeExitMember => {
            let declaration = cast_node::<ScopeExitMemberDeclarationSyntax>(
                view,
                "scope-exit member declaration",
            );

            lifecycle_surface(
                declaration
                    .async_capable_lifecycle_member_modifiers()
                    .tokens(),
                declaration.requires_clauses(),
                declaration.ensures_clauses(),
                declaration.with_clauses(),
                declaration.uses_clauses(),
            )
        }
        DeclarationKind::TypeCallableMember => {
            let declaration = cast_node::<TypeCallableMemberDeclarationSyntax>(
                view,
                "type callable member declaration",
            );

            declaration_surface_from_parts(
                declaration.type_callable_member_modifiers().tokens(),
                [],
                [],
                contract_clauses(
                    declaration.requires_clauses(),
                    declaration.ensures_clauses(),
                    declaration.with_clauses(),
                    declaration.uses_clauses(),
                ),
            )
        }
        DeclarationKind::CallableParameter => {
            let declaration = cast_node::<bray_syntax::ParameterSyntax>(view, "parameter");

            runtime_default_surface(
                modifier_token_surface(declaration.parameter_modifiers().tokens()),
                &declaration,
                declaration.equals_token(),
                declaration.expression(),
            )
        }
        DeclarationKind::PredicateParameter => {
            let _ = cast_node::<PredicateParameterSyntax>(view, "predicate parameter");

            DeclarationSurface::empty()
        }
        DeclarationKind::UnionPayloadField => {
            let declaration = cast_node::<UnionPayloadFieldSyntax>(view, "union payload field");

            runtime_default_surface(
                modifier_token_surface(declaration.payload_field_modifiers().tokens()),
                &declaration,
                declaration.equals_token(),
                declaration.expression(),
            )
        }
        DeclarationKind::Module
        | DeclarationKind::Using
        | DeclarationKind::Export
        | DeclarationKind::TraitConstantMember
        | DeclarationKind::TraitTypeMember
        | DeclarationKind::ImplementationTypeMemberBinding
        | DeclarationKind::GenericTypeParameter
        | DeclarationKind::GenericConstParameter => DeclarationSurface::empty(),
    }
}

pub(super) fn modifier_token_surface(
    tokens: impl IntoIterator<Item = SyntaxToken>,
) -> DeclarationSurface {
    declaration_surface_from_parts(tokens, [], [], [])
}

pub(super) fn runtime_default_surface<N, E>(
    surface: DeclarationSurface,
    owner: &N,
    equals_token: Option<SyntaxToken>,
    expression: Option<E>,
) -> DeclarationSurface
where
    N: SourceSyntaxNode,
    E: SourceSyntaxNode,
{
    let has_equals = equals_token.is_some_and(|token| !token.is_missing());
    let runtime_default = match expression.as_ref() {
        Some(expression) if has_equals && !expression.is_recovered() => {
            Some(SyntaxAnchor::from_node(expression))
        }
        Some(_) => Some(SyntaxAnchor::from_node(owner)),
        None if has_equals => Some(SyntaxAnchor::from_node(owner)),
        None => None,
    };

    surface.with_runtime_default(runtime_default)
}

fn overload_surface(
    surface: DeclarationSurface,
    arms: bray_syntax::OverloadArmListSyntax,
) -> DeclarationSurface {
    surface.with_overload_arms(
        arms.overload_arms()
            .map(|arm| SyntaxAnchor::from_node(&arm.path())),
    )
}

fn module_declaration_surface(
    tokens: impl IntoIterator<Item = SyntaxToken>,
    directives: Vec<SyntaxAnchor>,
) -> DeclarationSurface {
    declaration_surface_from_parts(tokens, directives, [], [])
}

fn constraint_surface<N>(constraints: impl IntoIterator<Item = N>) -> DeclarationSurface
where
    N: SourceSyntaxNode,
{
    declaration_surface_from_parts([], [], syntax_anchors(constraints), [])
}

fn lifecycle_surface<R, E, W, U>(
    modifiers: impl IntoIterator<Item = SyntaxToken>,
    requires_clauses: impl IntoIterator<Item = R>,
    ensures_clauses: impl IntoIterator<Item = E>,
    with_clauses: impl IntoIterator<Item = W>,
    uses_clauses: impl IntoIterator<Item = U>,
) -> DeclarationSurface
where
    R: SourceSyntaxNode,
    E: SourceSyntaxNode,
    W: SourceSyntaxNode,
    U: SourceSyntaxNode,
{
    declaration_surface_from_parts(
        modifiers,
        [],
        [],
        contract_clauses(
            requires_clauses,
            ensures_clauses,
            with_clauses,
            uses_clauses,
        ),
    )
}

fn declaration_surface_from_parts(
    modifier_tokens: impl IntoIterator<Item = SyntaxToken>,
    directives: impl IntoIterator<Item = SyntaxAnchor>,
    constraints: impl IntoIterator<Item = SyntaxAnchor>,
    contract_clauses: impl IntoIterator<Item = SyntaxAnchor>,
) -> DeclarationSurface {
    let (visibility, modifiers) = split_modifier_tokens(modifier_tokens);

    DeclarationSurface::new(
        visibility,
        modifiers,
        sorted_anchors(directives),
        sorted_anchors(constraints),
        sorted_anchors(contract_clauses),
    )
}

fn split_modifier_tokens(
    tokens: impl IntoIterator<Item = SyntaxToken>,
) -> (Option<SyntaxKind>, Vec<SyntaxKind>) {
    let mut visibility = None;
    let mut modifiers = Vec::new();

    for token in tokens {
        if token.is_missing() {
            continue;
        }

        let kind = token.kind();

        if kind.is_visibility_modifier() {
            if visibility.is_none() {
                visibility = Some(kind);
            }

            continue;
        }

        modifiers.push(kind);
    }

    (visibility, modifiers)
}

fn module_directives(directives: bray_syntax::ModuleDirectivesSyntax) -> Vec<SyntaxAnchor> {
    let mut anchors = syntax_anchors(directives.target_directives());

    anchors.extend(syntax_anchors(directives.test_directives()));
    anchors.extend(syntax_anchors(directives.link_directives()));

    sorted_anchors(anchors)
}

fn function_directives(directives: bray_syntax::FunctionDirectivesSyntax) -> Vec<SyntaxAnchor> {
    let mut anchors = syntax_anchors(directives.abi_directives());

    anchors.extend(syntax_anchors(directives.link_directives()));
    anchors.extend(syntax_anchors(directives.symbol_directives()));
    anchors.extend(syntax_anchors(directives.entrypoint_directives()));
    anchors.extend(syntax_anchors(directives.test_directives()));

    sorted_anchors(anchors)
}

fn type_directives(directives: bray_syntax::TypeDirectivesSyntax) -> Vec<SyntaxAnchor> {
    let mut anchors = syntax_anchors(directives.layout_directives());

    anchors.extend(syntax_anchors(directives.copy_directives()));

    sorted_anchors(anchors)
}

fn variant_directives(directives: bray_syntax::VariantDirectivesSyntax) -> Vec<SyntaxAnchor> {
    sorted_anchors(syntax_anchors(directives.tag_directives()))
}

fn contract_clauses<R, E, W, U>(
    requires_clauses: impl IntoIterator<Item = R>,
    ensures_clauses: impl IntoIterator<Item = E>,
    with_clauses: impl IntoIterator<Item = W>,
    uses_clauses: impl IntoIterator<Item = U>,
) -> Vec<SyntaxAnchor>
where
    R: SourceSyntaxNode,
    E: SourceSyntaxNode,
    W: SourceSyntaxNode,
    U: SourceSyntaxNode,
{
    let mut clauses = syntax_anchors(requires_clauses);

    clauses.extend(syntax_anchors(ensures_clauses));
    clauses.extend(syntax_anchors(with_clauses));
    clauses.extend(syntax_anchors(uses_clauses));

    sorted_anchors(clauses)
}

fn syntax_anchors<N>(nodes: impl IntoIterator<Item = N>) -> Vec<SyntaxAnchor>
where
    N: SourceSyntaxNode,
{
    nodes
        .into_iter()
        .map(|node| SyntaxAnchor::from_node(&node))
        .collect()
}

fn sorted_anchors(anchors: impl IntoIterator<Item = SyntaxAnchor>) -> Vec<SyntaxAnchor> {
    let mut anchors = anchors.into_iter().collect::<Vec<_>>();

    anchors.sort_by_key(|anchor| anchor.full_range());

    anchors
}
