use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::SyntaxKind;

use crate::diagnostic::{
    DirectiveDiagnostics, PendingDiagnostic, declaration,
    declaration_diagnostics_are_indeterminate, declaration_span,
    module_part_diagnostics_are_indeterminate, module_part_span,
};
use crate::name::DeclarationName;
use crate::record::{ContainerKind, ContainerRecord, DeclarationKind, DeclarationRecord};
use crate::surface::DeclarationBodyKind;
use crate::table::DeclarationTable;

pub(crate) fn declaration_form_diagnostics(
    table: &DeclarationTable,
    directives: DirectiveDiagnostics,
) -> Vec<PendingDiagnostic> {
    let mut diagnostics = Vec::new();

    let container_owners = table
        .declarations()
        .iter()
        .filter_map(|declaration| {
            declaration
                .child_container()
                .map(|container| (container, declaration.kind()))
        })
        .collect::<BTreeMap<_, _>>();

    for declaration in table.declarations() {
        if declaration_diagnostics_are_indeterminate(declaration, directives) {
            continue;
        }

        if declaration.kind() != DeclarationKind::Module {
            diagnostics.extend(modifier_occurrence_diagnostics(
                declaration_span(declaration),
                declaration.surface().modifier_occurrences(),
            ));
        }

        diagnostics.extend(context_modifier_diagnostics(
            table,
            declaration,
            &container_owners,
        ));

        diagnostics.extend(body_form_diagnostics(table, declaration));

        diagnostics.extend(member_placement_diagnostics(
            table,
            declaration,
            &container_owners,
        ));

        if directives.validates() {
            diagnostics.extend(directive_diagnostics(declaration));
        }
    }

    for part in table.module_parts() {
        if module_part_diagnostics_are_indeterminate(part, directives) {
            continue;
        }

        diagnostics.extend(modifier_occurrence_diagnostics(
            module_part_span(part),
            part.surface().modifier_occurrences(),
        ));
    }

    diagnostics.extend(lifecycle_slot_diagnostics(table));
    diagnostics.extend(parameter_order_diagnostics(table));

    diagnostics
}

fn modifier_occurrence_diagnostics(
    span: SourceSpan,
    modifiers: &[SyntaxKind],
) -> Vec<PendingDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for &modifier in modifiers {
        if !seen.insert(modifier) {
            diagnostics.push(anchored_declaration_diagnostic(
                span,
                DiagnosticKind::DeclarationDuplicateModifier,
                [DiagnosticArg::modifier_kind(modifier)],
            ));
        }
    }

    for (modifier, conflicting) in [
        (SyntaxKind::PublicKeyword, SyntaxKind::InternalKeyword),
        (SyntaxKind::StaticKeyword, SyntaxKind::ConsumeKeyword),
        (SyntaxKind::StaticKeyword, SyntaxKind::MutKeyword),
    ] {
        if seen.contains(&modifier) && seen.contains(&conflicting) {
            diagnostics.push(anchored_declaration_diagnostic(
                span,
                DiagnosticKind::DeclarationIncompatibleModifiers,
                [
                    DiagnosticArg::modifier_kind(modifier),
                    DiagnosticArg::conflicting_modifier_kind(conflicting),
                ],
            ));
        }
    }

    diagnostics
}

fn context_modifier_diagnostics(
    table: &DeclarationTable,
    declaration: &DeclarationRecord,
    container_owners: &BTreeMap<crate::ContainerId, DeclarationKind>,
) -> Vec<PendingDiagnostic> {
    let Some(container) = table.container(declaration.owning_container()) else {
        return Vec::new();
    };

    let owner = container_owners.get(&container.id()).copied();

    if container.kind() != ContainerKind::Implementation
        || !matches!(
            owner,
            Some(
                DeclarationKind::UnnamedTraitImplementation
                    | DeclarationKind::NamedTraitImplementation
            )
        )
    {
        return Vec::new();
    }

    declaration
        .surface()
        .modifier_occurrences()
        .iter()
        .copied()
        .filter(|modifier| modifier.is_visibility_modifier())
        .map(|modifier| {
            declaration_diagnostic(
                declaration,
                DiagnosticKind::DeclarationInvalidModifier,
                [DiagnosticArg::modifier_kind(modifier)],
            )
        })
        .collect()
}

fn body_form_diagnostics(
    table: &DeclarationTable,
    declaration: &DeclarationRecord,
) -> Vec<PendingDiagnostic> {
    let body = declaration.surface().body_kind();

    let trusted = declaration
        .surface()
        .modifiers()
        .contains(&SyntaxKind::TrustedKeyword);

    let diagnostic_kind = match declaration.kind() {
        DeclarationKind::Function => {
            let external = declaration
                .surface()
                .modifiers()
                .contains(&SyntaxKind::ExternKeyword);

            match (external, body) {
                (true, DeclarationBodyKind::Block) => {
                    Some(DiagnosticKind::DeclarationBodyNotAllowed)
                }
                (false, DeclarationBodyKind::None) => Some(DiagnosticKind::DeclarationBodyRequired),
                _ => None,
            }
        }
        DeclarationKind::Predicate => predicate_body_diagnostic(trusted, body),
        DeclarationKind::TraitPredicateMember => match container_kind(table, declaration) {
            Some(ContainerKind::Trait) if trusted && body == DeclarationBodyKind::Expression => {
                Some(DiagnosticKind::DeclarationBodyNotAllowed)
            }
            Some(ContainerKind::Implementation) => predicate_body_diagnostic(trusted, body),
            _ => None,
        },
        DeclarationKind::Constant | DeclarationKind::Static
            if body == DeclarationBodyKind::None =>
        {
            Some(DiagnosticKind::DeclarationBodyRequired)
        }
        DeclarationKind::TypeCallableMember
        | DeclarationKind::TypeConstructorMember
        | DeclarationKind::FinalizerMember
        | DeclarationKind::DestructorMember
        | DeclarationKind::ScopeEnterMember
        | DeclarationKind::ScopeExitMember
            if body == DeclarationBodyKind::None =>
        {
            Some(DiagnosticKind::DeclarationBodyRequired)
        }
        DeclarationKind::TraitFinalizerRequirement
        | DeclarationKind::TraitDestructorRequirement
        | DeclarationKind::TraitScopeEnterRequirement
        | DeclarationKind::TraitScopeExitRequirement
            if body != DeclarationBodyKind::None =>
        {
            Some(DiagnosticKind::DeclarationBodyNotAllowed)
        }
        _ => None,
    };

    diagnostic_kind.map_or_else(Vec::new, |kind| {
        vec![declaration_diagnostic(
            declaration,
            kind,
            [DiagnosticArg::actual_syntax_kind(declaration.syntax_kind())],
        )]
    })
}

fn predicate_body_diagnostic(trusted: bool, body: DeclarationBodyKind) -> Option<DiagnosticKind> {
    match (trusted, body) {
        (true, DeclarationBodyKind::Expression) => Some(DiagnosticKind::DeclarationBodyNotAllowed),
        (false, DeclarationBodyKind::None) => Some(DiagnosticKind::DeclarationBodyRequired),
        _ => None,
    }
}

fn directive_diagnostics(declaration: &DeclarationRecord) -> Vec<PendingDiagnostic> {
    if declaration.kind() == DeclarationKind::Module {
        return Vec::new();
    }

    let mut seen = BTreeSet::new();
    let mut diagnostics = Vec::new();

    let external = declaration
        .surface()
        .modifiers()
        .contains(&SyntaxKind::ExternKeyword);

    let has_abi = declaration
        .surface()
        .directives()
        .iter()
        .any(|directive| directive.syntax_kind() == SyntaxKind::AbiDirective);

    for directive in declaration.surface().directives() {
        let kind = directive.syntax_kind();

        if kind != SyntaxKind::LinkDirective && !seen.insert(kind) {
            diagnostics.push(declaration_diagnostic(
                declaration,
                DiagnosticKind::DeclarationDuplicateDirective,
                [DiagnosticArg::directive_kind(kind)],
            ));
        }

        let valid_target = match kind {
            SyntaxKind::LinkDirective => external,
            SyntaxKind::SymbolDirective => external || has_abi,
            SyntaxKind::EntrypointDirective | SyntaxKind::TestDirective => !external,
            SyntaxKind::ThreadLocalDirective => declaration.kind() == DeclarationKind::Static,
            _ => true,
        };

        if !valid_target {
            diagnostics.push(declaration_diagnostic(
                declaration,
                DiagnosticKind::DeclarationInvalidDirectiveTarget,
                [DiagnosticArg::directive_kind(kind)],
            ));
        }
    }

    if seen.contains(&SyntaxKind::EntrypointDirective) && seen.contains(&SyntaxKind::TestDirective)
    {
        diagnostics.push(declaration_diagnostic(
            declaration,
            DiagnosticKind::DeclarationIncompatibleDirectives,
            [
                DiagnosticArg::directive_kind(SyntaxKind::EntrypointDirective),
                DiagnosticArg::conflicting_directive_kind(SyntaxKind::TestDirective),
            ],
        ));
    }

    diagnostics
}

fn member_placement_diagnostics(
    table: &DeclarationTable,
    declaration: &DeclarationRecord,
    container_owners: &BTreeMap<crate::ContainerId, DeclarationKind>,
) -> Vec<PendingDiagnostic> {
    let Some(container) = table.container(declaration.owning_container()) else {
        return vec![declaration_diagnostic(
            declaration,
            DiagnosticKind::DeclarationInvalidMemberPlacement,
            [DiagnosticArg::actual_syntax_kind(declaration.syntax_kind())],
        )];
    };

    let owner = container_owners.get(&container.id()).copied();

    if declaration_is_valid_in_container(declaration.kind(), container.kind(), owner) {
        Vec::new()
    } else {
        vec![declaration_diagnostic(
            declaration,
            DiagnosticKind::DeclarationInvalidMemberPlacement,
            [DiagnosticArg::actual_syntax_kind(declaration.syntax_kind())],
        )]
    }
}

fn declaration_is_valid_in_container(
    declaration: DeclarationKind,
    container: ContainerKind,
    owner: Option<DeclarationKind>,
) -> bool {
    match container {
        ContainerKind::Root => declaration == DeclarationKind::Module,
        ContainerKind::Module => matches!(
            declaration,
            DeclarationKind::Module
                | DeclarationKind::Using
                | DeclarationKind::Export
                | DeclarationKind::Constant
                | DeclarationKind::Static
                | DeclarationKind::Function
                | DeclarationKind::Predicate
                | DeclarationKind::CallableContract
                | DeclarationKind::CallableOverload
                | DeclarationKind::ImplementationOverload
                | DeclarationKind::Struct
                | DeclarationKind::Union
                | DeclarationKind::Trait
                | DeclarationKind::InherentImplementation
                | DeclarationKind::UnnamedTraitImplementation
                | DeclarationKind::NamedTraitImplementation
        ),
        ContainerKind::Type => type_member_is_valid(declaration, owner),
        ContainerKind::Trait => matches!(
            declaration,
            DeclarationKind::GenericTypeParameter
                | DeclarationKind::GenericConstParameter
                | DeclarationKind::TraitConstantMember
                | DeclarationKind::TraitTypeMember
                | DeclarationKind::TraitPredicateMember
                | DeclarationKind::TraitCallableMember
                | DeclarationKind::TraitFinalizerRequirement
                | DeclarationKind::TraitDestructorRequirement
                | DeclarationKind::TraitScopeEnterRequirement
                | DeclarationKind::TraitScopeExitRequirement
        ),
        ContainerKind::Implementation => implementation_member_is_valid(declaration, owner),
        ContainerKind::Signature => signature_child_is_valid(declaration, owner),
        ContainerKind::Variant => declaration == DeclarationKind::UnionPayloadField,
    }
}

fn type_member_is_valid(declaration: DeclarationKind, owner: Option<DeclarationKind>) -> bool {
    let representation_member = match owner {
        Some(DeclarationKind::Struct) => declaration == DeclarationKind::StructField,
        Some(DeclarationKind::Union) => declaration == DeclarationKind::UnionVariant,
        _ => false,
    };

    representation_member
        || matches!(
            declaration,
            DeclarationKind::GenericTypeParameter | DeclarationKind::GenericConstParameter
        )
        || matches!(
            declaration,
            DeclarationKind::Constant
                | DeclarationKind::Predicate
                | DeclarationKind::CallableOverload
                | DeclarationKind::TypeConstructorMember
                | DeclarationKind::FinalizerMember
                | DeclarationKind::DestructorMember
                | DeclarationKind::ScopeEnterMember
                | DeclarationKind::ScopeExitMember
                | DeclarationKind::TypeCallableMember
        )
}

fn implementation_member_is_valid(
    declaration: DeclarationKind,
    owner: Option<DeclarationKind>,
) -> bool {
    let shared = matches!(
        declaration,
        DeclarationKind::Constant
            | DeclarationKind::Predicate
            | DeclarationKind::ImplementationTypeMemberBinding
            | DeclarationKind::TypeCallableMember
    );

    if shared {
        return true;
    }

    match owner {
        Some(DeclarationKind::InherentImplementation) => matches!(
            declaration,
            DeclarationKind::CallableOverload
                | DeclarationKind::TypeConstructorMember
                | DeclarationKind::FinalizerMember
                | DeclarationKind::DestructorMember
                | DeclarationKind::ScopeEnterMember
                | DeclarationKind::ScopeExitMember
        ),
        Some(
            DeclarationKind::UnnamedTraitImplementation | DeclarationKind::NamedTraitImplementation,
        ) => matches!(
            declaration,
            DeclarationKind::ScopeEnterMember | DeclarationKind::ScopeExitMember
        ),
        _ => false,
    }
}

fn signature_child_is_valid(declaration: DeclarationKind, owner: Option<DeclarationKind>) -> bool {
    match owner {
        Some(
            DeclarationKind::Static
            | DeclarationKind::Function
            | DeclarationKind::CallableContract
            | DeclarationKind::TraitCallableMember
            | DeclarationKind::TypeCallableMember,
        ) => matches!(
            declaration,
            DeclarationKind::GenericTypeParameter
                | DeclarationKind::GenericConstParameter
                | DeclarationKind::CallableParameter
        ),
        Some(DeclarationKind::Predicate) => matches!(
            declaration,
            DeclarationKind::GenericTypeParameter
                | DeclarationKind::GenericConstParameter
                | DeclarationKind::PredicateParameter
        ),
        Some(DeclarationKind::TraitPredicateMember) => {
            declaration == DeclarationKind::PredicateParameter
        }
        Some(
            DeclarationKind::TraitFinalizerRequirement
            | DeclarationKind::TraitDestructorRequirement
            | DeclarationKind::TraitScopeEnterRequirement
            | DeclarationKind::TraitScopeExitRequirement
            | DeclarationKind::TypeConstructorMember
            | DeclarationKind::FinalizerMember
            | DeclarationKind::DestructorMember
            | DeclarationKind::ScopeEnterMember
            | DeclarationKind::ScopeExitMember,
        ) => declaration == DeclarationKind::CallableParameter,
        _ => false,
    }
}

fn lifecycle_slot_diagnostics(table: &DeclarationTable) -> Vec<PendingDiagnostic> {
    let mut diagnostics = Vec::new();

    for container in table.containers() {
        if !matches!(
            container.kind(),
            ContainerKind::Type | ContainerKind::Trait | ContainerKind::Implementation
        ) {
            continue;
        }

        let mut first_by_slot: BTreeMap<_, &DeclarationRecord> = BTreeMap::new();

        for declaration_id in container.declarations() {
            let declaration = declaration(table, *declaration_id);

            if declaration.is_recovered() {
                continue;
            }

            let Some(slot) = lifecycle_slot(declaration) else {
                continue;
            };

            if let Some(first) = first_by_slot.get(&slot) {
                let duplicate_span = declaration_span(declaration);
                let diagnostic = duplicate_lifecycle_slot_diagnostic(first, declaration);

                diagnostics.push(PendingDiagnostic::new(duplicate_span, diagnostic));
            } else {
                first_by_slot.insert(slot, declaration);
            }
        }
    }

    diagnostics
}

fn parameter_order_diagnostics(table: &DeclarationTable) -> Vec<PendingDiagnostic> {
    let mut diagnostics = Vec::new();

    for container in table
        .containers()
        .iter()
        .filter(|container| container.kind() == ContainerKind::Signature)
    {
        let mut saw_named_only = false;

        for declaration_id in container.declarations() {
            let parameter = declaration(table, *declaration_id);

            if parameter.kind() != DeclarationKind::CallableParameter || parameter.is_recovered() {
                continue;
            }

            let positional = parameter
                .surface()
                .modifiers()
                .contains(&SyntaxKind::PosKeyword);

            if positional && saw_named_only {
                diagnostics.push(declaration_diagnostic(
                    parameter,
                    DiagnosticKind::DeclarationInvalidParameterOrder,
                    [],
                ));
            } else if !positional {
                saw_named_only = true;
            }
        }
    }

    diagnostics
}

pub(crate) fn lifecycle_slot(declaration: &DeclarationRecord) -> Option<SyntaxKind> {
    match declaration.kind() {
        DeclarationKind::TypeConstructorMember
            if matches!(
                declaration.name(),
                Some(DeclarationName::Keyword(SyntaxKind::ConstructKeyword))
            ) =>
        {
            Some(SyntaxKind::ConstructKeyword)
        }
        DeclarationKind::FinalizerMember | DeclarationKind::TraitFinalizerRequirement => {
            Some(SyntaxKind::FinalizeKeyword)
        }
        DeclarationKind::DestructorMember | DeclarationKind::TraitDestructorRequirement => {
            Some(SyntaxKind::DestructKeyword)
        }
        DeclarationKind::ScopeEnterMember | DeclarationKind::TraitScopeEnterRequirement => {
            Some(SyntaxKind::EnterKeyword)
        }
        DeclarationKind::ScopeExitMember | DeclarationKind::TraitScopeExitRequirement => {
            Some(SyntaxKind::ExitKeyword)
        }
        _ => None,
    }
}

/// Creates the structured diagnostic for two declarations occupying one lifecycle slot.
pub fn duplicate_lifecycle_slot_diagnostic(
    first: &DeclarationRecord,
    duplicate: &DeclarationRecord,
) -> Diagnostic {
    let first_span = declaration_span(first);
    let duplicate_span = declaration_span(duplicate);

    Diagnostic::new(
        DiagnosticId::new(duplicate_span.start().bytes()),
        DiagnosticKind::DeclarationDuplicateLifecycleSlot,
        SeverityKind::Error,
    )
    .with_primary_span(duplicate_span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::DuplicateDeclaration,
        duplicate_span,
    ))
    .with_related_location(DiagnosticRelatedLocation::new(
        DiagnosticRelatedLocationKind::FirstDeclaration,
        first_span,
    ))
}

fn declaration_diagnostic<const ARG_COUNT: usize>(
    declaration: &DeclarationRecord,
    kind: DiagnosticKind,
    args: [DiagnosticArg; ARG_COUNT],
) -> PendingDiagnostic {
    let span = declaration_span(declaration);

    anchored_declaration_diagnostic(span, kind, args)
}

fn anchored_declaration_diagnostic<const ARG_COUNT: usize>(
    span: SourceSpan,
    kind: DiagnosticKind,
    args: [DiagnosticArg; ARG_COUNT],
) -> PendingDiagnostic {
    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(span.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::InvalidDeclaration,
        span,
    ));

    for arg in args {
        diagnostic = diagnostic.with_arg(arg);
    }

    PendingDiagnostic::new(span, diagnostic)
}

fn container_kind(
    table: &DeclarationTable,
    declaration: &DeclarationRecord,
) -> Option<ContainerKind> {
    table
        .container(declaration.owning_container())
        .map(ContainerRecord::kind)
}
