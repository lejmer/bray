use std::collections::{BTreeMap, BTreeSet};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticModuleTrust, DiagnosticVisibility, SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::SyntaxKind;

use crate::name::DeclarationName;
use crate::record::{ContainerKind, DeclarationKind, DeclarationRecord, ModulePartRecord};
use crate::table::DeclarationTable;
use crate::validation::{declaration_form_diagnostics, lifecycle_slot};

#[derive(Clone, Copy)]
pub(crate) enum DirectiveDiagnostics {
    Indeterminate,
    Selected,
}

impl DirectiveDiagnostics {
    pub(crate) const fn validates(self) -> bool {
        matches!(self, Self::Selected)
    }
}

pub(crate) fn declaration_diagnostics(
    table: &DeclarationTable,
    directives: DirectiveDiagnostics,
) -> DiagnosticBag {
    let mut diagnostics = duplicate_name_diagnostics(table, directives);

    diagnostics.extend(module_surface_diagnostics(table, directives));
    diagnostics.extend(declaration_form_diagnostics(table, directives));

    diagnostics.sort_by_key(|pending| {
        (
            pending.span.source_id(),
            pending.span.start(),
            pending.span.end(),
            pending.diagnostic.kind(),
        )
    });

    DiagnosticBag::from(
        diagnostics
            .into_iter()
            .map(|pending| pending.diagnostic)
            .collect::<Vec<_>>(),
    )
}

fn duplicate_name_diagnostics(
    table: &DeclarationTable,
    directives: DirectiveDiagnostics,
) -> Vec<PendingDiagnostic> {
    let mut diagnostics = Vec::new();

    let (excluded_declarations, excluded_containers) =
        declaration_diagnostic_exclusions(table, directives);

    for container in table.containers() {
        if excluded_containers.contains(&container.id()) {
            continue;
        }

        let mut first_by_name: BTreeMap<_, &DeclarationRecord> = BTreeMap::new();

        for declaration_id in container.declarations() {
            let declaration = declaration(table, *declaration_id);

            if excluded_declarations.contains(&declaration.id()) {
                continue;
            }

            if lifecycle_slot(declaration).is_some() {
                continue;
            }

            let Some(domain) = declaration_name_domain(container.kind(), declaration.kind()) else {
                continue;
            };

            let Some(name) = declaration.name() else {
                continue;
            };

            let Some(name_text) = declaration_name_text(name) else {
                continue;
            };

            match first_by_name.get(&(domain, name)) {
                Some(first) => {
                    diagnostics.push(duplicate_name_diagnostic(first, declaration, name_text))
                }
                None => {
                    first_by_name.insert((domain, name), declaration);
                }
            }
        }
    }

    diagnostics
}

fn declaration_diagnostic_exclusions(
    table: &DeclarationTable,
    directives: DirectiveDiagnostics,
) -> (BTreeSet<crate::DeclarationId>, BTreeSet<crate::ContainerId>) {
    let mut declarations = table
        .declarations()
        .iter()
        .filter(|declaration| declaration_diagnostics_are_indeterminate(declaration, directives))
        .map(DeclarationRecord::id)
        .collect::<BTreeSet<_>>();

    for part in table.module_parts() {
        if module_part_diagnostics_are_indeterminate(part, directives) {
            declarations.extend(part.declarations());
        }
    }

    let mut containers = declarations
        .iter()
        .filter_map(|id| declaration(table, *id).child_container())
        .collect::<BTreeSet<_>>();

    loop {
        let previous_len = containers.len();

        let descendants = table
            .containers()
            .iter()
            .filter(|container| {
                container
                    .parent()
                    .is_some_and(|id| containers.contains(&id))
            })
            .map(|container| container.id())
            .collect::<Vec<_>>();

        containers.extend(descendants);

        if containers.len() == previous_len {
            break;
        }
    }

    (declarations, containers)
}

pub(crate) fn declaration_diagnostics_are_indeterminate(
    declaration: &DeclarationRecord,
    directives: DirectiveDiagnostics,
) -> bool {
    declaration.is_recovered()
        || (!directives.validates() && !declaration.surface().directives().is_empty())
}

pub(crate) fn module_part_diagnostics_are_indeterminate(
    part: &ModulePartRecord,
    directives: DirectiveDiagnostics,
) -> bool {
    part.is_recovered() || (!directives.validates() && !part.surface().directives().is_empty())
}

fn module_surface_diagnostics(
    table: &DeclarationTable,
    directives: DirectiveDiagnostics,
) -> Vec<PendingDiagnostic> {
    let mut diagnostics = Vec::new();

    for module in table.module_containers() {
        let mut parts = module
            .module_parts()
            .iter()
            .map(|id| module_part(table, *id))
            .filter(|part| !module_part_diagnostics_are_indeterminate(part, directives));

        let Some(first) = parts.next() else {
            continue;
        };

        let expected_visibility = module_visibility(first);
        let expected_trust = module_trust(first);

        let module_name = match module.module_path() {
            Some(path) => path.dotted(),
            None => panic!("module container must have a module path"),
        };

        for part in parts {
            let actual_visibility = module_visibility(part);

            if actual_visibility != expected_visibility {
                diagnostics.push(conflicting_module_visibility_diagnostic(
                    first,
                    part,
                    &module_name,
                    expected_visibility,
                    actual_visibility,
                ));
            }

            let actual_trust = module_trust(part);

            if actual_trust != expected_trust {
                diagnostics.push(conflicting_module_trust_diagnostic(
                    first,
                    part,
                    &module_name,
                    expected_trust,
                    actual_trust,
                ));
            }
        }
    }

    diagnostics
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum DeclarationNameDomain {
    Module,
    TypeMember,
    TraitMember,
    ImplementationMember,
    GenericParameter,
    CallableParameter,
    VariantPayload,
}

fn declaration_name_domain(
    container: ContainerKind,
    declaration: DeclarationKind,
) -> Option<DeclarationNameDomain> {
    match declaration {
        DeclarationKind::GenericTypeParameter | DeclarationKind::GenericConstParameter => {
            return Some(DeclarationNameDomain::GenericParameter);
        }
        DeclarationKind::CallableParameter | DeclarationKind::PredicateParameter => {
            return Some(DeclarationNameDomain::CallableParameter);
        }
        DeclarationKind::UnionPayloadField => {
            return Some(DeclarationNameDomain::VariantPayload);
        }
        DeclarationKind::Module => return None,
        DeclarationKind::Using
        | DeclarationKind::Export
        | DeclarationKind::Constant
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
        | DeclarationKind::StructField
        | DeclarationKind::UnionVariant
        | DeclarationKind::TraitConstantMember
        | DeclarationKind::TraitTypeMember
        | DeclarationKind::TraitPredicateMember
        | DeclarationKind::TraitCallableMember
        | DeclarationKind::TraitFinalizerRequirement
        | DeclarationKind::TraitDestructorRequirement
        | DeclarationKind::TraitScopeEnterRequirement
        | DeclarationKind::TraitScopeExitRequirement
        | DeclarationKind::ImplementationTypeMemberBinding
        | DeclarationKind::TypeConstructorMember
        | DeclarationKind::FinalizerMember
        | DeclarationKind::DestructorMember
        | DeclarationKind::ScopeEnterMember
        | DeclarationKind::ScopeExitMember
        | DeclarationKind::TypeCallableMember => {}
    }

    match container {
        ContainerKind::Module => Some(DeclarationNameDomain::Module),
        ContainerKind::Type => Some(DeclarationNameDomain::TypeMember),
        ContainerKind::Trait => Some(DeclarationNameDomain::TraitMember),
        ContainerKind::Implementation => Some(DeclarationNameDomain::ImplementationMember),
        ContainerKind::Root | ContainerKind::Signature | ContainerKind::Variant => None,
    }
}

fn duplicate_name_diagnostic(
    first: &DeclarationRecord,
    duplicate: &DeclarationRecord,
    name: &str,
) -> PendingDiagnostic {
    let first_span = declaration_span(first);
    let duplicate_span = declaration_span(duplicate);

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(duplicate_span.start().bytes()),
        DiagnosticKind::DeclarationDuplicateName,
        SeverityKind::Error,
    )
    .with_primary_span(duplicate_span)
    .with_arg(DiagnosticArg::declaration_name(name))
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::DuplicateDeclaration,
        duplicate_span,
    ))
    .with_label(DiagnosticLabel::secondary(
        DiagnosticLabelKind::FirstDeclaration,
        first_span,
    ));

    PendingDiagnostic::new(duplicate_span, diagnostic)
}

fn conflicting_module_visibility_diagnostic(
    first: &ModulePartRecord,
    conflicting: &ModulePartRecord,
    module_name: &str,
    expected: DiagnosticVisibility,
    actual: DiagnosticVisibility,
) -> PendingDiagnostic {
    conflicting_module_diagnostic(
        first,
        conflicting,
        DiagnosticKind::DeclarationConflictingModuleVisibility,
        [
            DiagnosticArg::declaration_name(module_name),
            DiagnosticArg::expected_visibility(expected),
            DiagnosticArg::actual_visibility(actual),
        ],
    )
}

fn conflicting_module_trust_diagnostic(
    first: &ModulePartRecord,
    conflicting: &ModulePartRecord,
    module_name: &str,
    expected: DiagnosticModuleTrust,
    actual: DiagnosticModuleTrust,
) -> PendingDiagnostic {
    conflicting_module_diagnostic(
        first,
        conflicting,
        DiagnosticKind::DeclarationConflictingModuleTrust,
        [
            DiagnosticArg::declaration_name(module_name),
            DiagnosticArg::expected_module_trust(expected),
            DiagnosticArg::actual_module_trust(actual),
        ],
    )
}

fn conflicting_module_diagnostic(
    first: &ModulePartRecord,
    conflicting: &ModulePartRecord,
    kind: DiagnosticKind,
    args: impl IntoIterator<Item = DiagnosticArg>,
) -> PendingDiagnostic {
    let first_span = module_part_span(first);
    let conflicting_span = module_part_span(conflicting);

    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(conflicting_span.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(conflicting_span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::ConflictingModuleDeclaration,
        conflicting_span,
    ))
    .with_label(DiagnosticLabel::secondary(
        DiagnosticLabelKind::FirstModuleDeclaration,
        first_span,
    ));

    for arg in args {
        diagnostic = diagnostic.with_arg(arg);
    }

    PendingDiagnostic::new(conflicting_span, diagnostic)
}

fn declaration_name_text(name: &DeclarationName) -> Option<&str> {
    match name {
        DeclarationName::Identifier(name) => Some(name),
        DeclarationName::Keyword(kind) => kind.as_str().strip_suffix("_keyword"),
        DeclarationName::Path(_) | DeclarationName::Implementation(_) => None,
    }
}

fn module_visibility(part: &ModulePartRecord) -> DiagnosticVisibility {
    match part.surface().visibility() {
        None | Some(SyntaxKind::PublicKeyword) => DiagnosticVisibility::Public,
        Some(SyntaxKind::InternalKeyword) => DiagnosticVisibility::Internal,
        Some(kind) => panic!("module visibility must be public or internal: {kind:?}"),
    }
}

fn module_trust(part: &ModulePartRecord) -> DiagnosticModuleTrust {
    if part
        .surface()
        .modifiers()
        .contains(&SyntaxKind::TrustedKeyword)
    {
        DiagnosticModuleTrust::Trusted
    } else {
        DiagnosticModuleTrust::Ordinary
    }
}

pub(crate) fn declaration(
    table: &DeclarationTable,
    id: crate::DeclarationId,
) -> &DeclarationRecord {
    match table.declaration(id) {
        Some(declaration) => declaration,
        None => panic!("container declaration ID must exist: {id:?}"),
    }
}

fn module_part(table: &DeclarationTable, id: crate::ModulePartId) -> &ModulePartRecord {
    match table.module_part(id) {
        Some(part) => part,
        None => panic!("module part ID must exist: {id:?}"),
    }
}

pub(crate) fn declaration_span(declaration: &DeclarationRecord) -> SourceSpan {
    SourceSpan::new(declaration.source_id(), declaration.full_range())
}

pub(crate) fn module_part_span(part: &ModulePartRecord) -> SourceSpan {
    SourceSpan::new(part.source_id(), part.full_range())
}

pub(crate) struct PendingDiagnostic {
    span: SourceSpan,
    diagnostic: Diagnostic,
}

impl PendingDiagnostic {
    pub(crate) const fn new(span: SourceSpan, diagnostic: Diagnostic) -> Self {
        Self { span, diagnostic }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticLabelKind,
        DiagnosticLabelStyle, DiagnosticModuleTrust, DiagnosticVisibility,
    };
    use bray_testing::{test_source_at as source, test_source_store as source_store};

    use crate::test_support::{
        parse_recovered_source_unit_for_test, parse_valid_source_unit_for_test,
    };
    use crate::{
        discover_source_unit_declarations, merge_declaration_chunks,
        merge_selected_declaration_chunks,
    };

    #[test]
    fn table_validation_reports_duplicate_names_in_each_declaration_domain() {
        let sources = source_store([
            "module core; struct Point {}",
            concat!(
                "module core {\n",
                "struct Point {}\n",
                "struct Holder<T, T> { left: Int; left: Bool; }\n",
                "func run(value: Int, value: Bool) {}\n",
                "}",
            ),
        ]);

        let first = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 0,
        )));

        let second = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 1,
        )));

        let forward = merge_declaration_chunks([&first, &second]);
        let reverse = merge_declaration_chunks([&second, &first]);

        assert_eq!(forward, reverse);

        let result = forward;

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [
                DiagnosticKind::DeclarationDuplicateName,
                DiagnosticKind::DeclarationDuplicateName,
                DiagnosticKind::DeclarationDuplicateName,
                DiagnosticKind::DeclarationDuplicateName,
            ]
        );

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(declaration_name)
                .collect::<Vec<_>>(),
            ["Point", "T", "left", "value"]
        );

        for diagnostic in result.diagnostics() {
            let [duplicate, first] = diagnostic.labels() else {
                panic!("expected duplicate and first-declaration labels: {diagnostic:?}");
            };

            assert_eq!(duplicate.kind(), DiagnosticLabelKind::DuplicateDeclaration);
            assert_eq!(duplicate.style(), DiagnosticLabelStyle::Primary);
            assert_eq!(first.kind(), DiagnosticLabelKind::FirstDeclaration);
            assert_eq!(first.style(), DiagnosticLabelStyle::Secondary);
            assert!(first.span() < duplicate.span());
        }
    }

    #[test]
    fn table_validation_reports_conflicting_split_module_surfaces() {
        let sources = source_store(["trusted module core;", "internal module core {}"]);

        let first = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 0,
        )));

        let second = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 1,
        )));

        let forward = merge_declaration_chunks([&first, &second]);
        let reverse = merge_declaration_chunks([&second, &first]);

        assert_eq!(forward, reverse);

        let result = forward;

        let [visibility, trust] = result.diagnostics().diagnostics() else {
            panic!(
                "expected visibility and trust diagnostics: {:?}",
                result.diagnostics()
            );
        };

        assert_eq!(
            visibility.kind(),
            DiagnosticKind::DeclarationConflictingModuleVisibility
        );

        assert_eq!(declaration_name(visibility), "core");

        assert_eq!(
            visibility.args(),
            &[
                bray_diagnostics::DiagnosticArg::declaration_name("core"),
                bray_diagnostics::DiagnosticArg::expected_visibility(DiagnosticVisibility::Public),
                bray_diagnostics::DiagnosticArg::actual_visibility(DiagnosticVisibility::Internal),
            ]
        );

        assert_eq!(
            trust.kind(),
            DiagnosticKind::DeclarationConflictingModuleTrust
        );

        assert_eq!(
            trust.args(),
            &[
                bray_diagnostics::DiagnosticArg::declaration_name("core"),
                bray_diagnostics::DiagnosticArg::expected_module_trust(
                    DiagnosticModuleTrust::Trusted
                ),
                bray_diagnostics::DiagnosticArg::actual_module_trust(
                    DiagnosticModuleTrust::Ordinary
                ),
            ]
        );

        for diagnostic in [visibility, trust] {
            let primary_span = match diagnostic.primary_span() {
                Some(span) => span,
                None => panic!("module conflict should have a primary span"),
            };

            assert_eq!(primary_span.source_id(), source(&sources, 1).source_id());

            let [conflicting, first] = diagnostic.labels() else {
                panic!("expected conflicting and first-module labels: {diagnostic:?}");
            };

            assert_eq!(
                conflicting.kind(),
                DiagnosticLabelKind::ConflictingModuleDeclaration
            );

            assert_eq!(first.kind(), DiagnosticLabelKind::FirstModuleDeclaration);
            assert_eq!(first.span().source_id(), source(&sources, 0).source_id());
        }
    }

    #[test]
    fn table_validation_reports_modifier_directive_and_body_form_errors() {
        let sources = source_store([concat!(
            "module app;\n",
            "public public func repeated()\n",
            "{\n",
            "}\n",
            "@entrypoint @entrypoint func directed()\n",
            "{\n",
            "}\n",
            "extern func external_with_body()\n",
            "{\n",
            "}\n",
            "func missing_body();\n",
            "predicate missing_predicate_body();\n",
            "trusted predicate trusted_with_body() = true;\n",
            "struct Resource\n",
            "{\n",
            "    static consume func invalid_receiver()\n",
            "    {\n",
            "    }\n",
            "}\n",
        )]);

        let chunk = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 0,
        )));

        let result = merge_selected_declaration_chunks([&chunk], |_| true, |_| true);

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [
                DiagnosticKind::DeclarationDuplicateModifier,
                DiagnosticKind::DeclarationDuplicateDirective,
                DiagnosticKind::DeclarationBodyNotAllowed,
                DiagnosticKind::DeclarationBodyRequired,
                DiagnosticKind::DeclarationBodyRequired,
                DiagnosticKind::DeclarationBodyNotAllowed,
                DiagnosticKind::DeclarationIncompatibleModifiers,
            ]
        );
    }

    #[test]
    fn table_validation_reports_local_lifecycle_slot_conflicts() {
        let sources = source_store([concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    finalize()\n",
            "    {\n",
            "    }\n",
            "    finalize()\n",
            "    {\n",
            "    }\n",
            "}\n",
        )]);

        let chunk = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 0,
        )));

        let result = merge_declaration_chunks([&chunk]);

        let [diagnostic] = result.diagnostics().diagnostics() else {
            panic!(
                "expected one lifecycle-slot diagnostic: {:?}",
                result.diagnostics()
            );
        };

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::DeclarationDuplicateLifecycleSlot
        );

        let [duplicate, first] = diagnostic.labels() else {
            panic!("expected duplicate and first lifecycle labels: {diagnostic:?}");
        };

        assert_eq!(duplicate.kind(), DiagnosticLabelKind::DuplicateDeclaration);
        assert_eq!(first.kind(), DiagnosticLabelKind::FirstDeclaration);
    }

    #[test]
    fn table_validation_reports_contextual_declaration_form_errors() {
        let sources = source_store([concat!(
            "module app;\n",
            "@link(name = \"native\")\n",
            "func linked()\n",
            "{\n",
            "}\n",
            "@entrypoint @test\n",
            "func conflicting()\n",
            "{\n",
            "}\n",
            "func misplaced(first: i32, pos second: i32)\n",
            "{\n",
            "}\n",
            "impl Resource(Display)\n",
            "{\n",
            "    public func visible()\n",
            "    {\n",
            "    }\n",
            "    construct() -> Self\n",
            "    {\n",
            "    }\n",
            "    overload grouped = {visible};\n",
            "}\n",
        )]);

        let chunk = discover_source_unit_declarations(&parse_valid_source_unit_for_test(source(
            &sources, 0,
        )));

        let result = merge_selected_declaration_chunks([&chunk], |_| true, |_| true);

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [
                DiagnosticKind::DeclarationInvalidDirectiveTarget,
                DiagnosticKind::DeclarationIncompatibleDirectives,
                DiagnosticKind::DeclarationInvalidParameterOrder,
                DiagnosticKind::DeclarationInvalidModifier,
                DiagnosticKind::DeclarationInvalidMemberPlacement,
                DiagnosticKind::DeclarationInvalidMemberPlacement,
            ]
        );
    }

    #[test]
    fn table_validation_does_not_report_recovered_or_distinct_domain_names() {
        let sources = source_store([
            "module core; struct Point\nstruct Point {}",
            "module other; struct Box<T> { T: Int; }",
            "module callable; func run(value: Int value: Bool) {}",
        ]);

        let recovered = discover_source_unit_declarations(&parse_recovered_source_unit_for_test(
            source(&sources, 0),
        ));

        let distinct_domains = discover_source_unit_declarations(
            &parse_valid_source_unit_for_test(source(&sources, 1)),
        );

        let recovered_signature = discover_source_unit_declarations(
            &parse_recovered_source_unit_for_test(source(&sources, 2)),
        );

        let result =
            merge_declaration_chunks([&recovered, &distinct_domains, &recovered_signature]);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn table_validation_defers_directive_bearing_contributions() {
        let sources = source_store([
            "module core { struct Point {} }",
            "@test internal module core { struct Point {} }",
            concat!(
                "module other;\n",
                "struct Value {}\n",
                "@test struct Value {}\n",
                "@test func missing();\n",
            ),
        ]);

        let ordinary = discover_source_unit_declarations(&parse_valid_source_unit_for_test(
            source(&sources, 0),
        ));
        let directed_module = discover_source_unit_declarations(&parse_valid_source_unit_for_test(
            source(&sources, 1),
        ));
        let directed_declaration = discover_source_unit_declarations(
            &parse_valid_source_unit_for_test(source(&sources, 2)),
        );

        let result = merge_declaration_chunks([&ordinary, &directed_module, &directed_declaration]);

        assert!(result.diagnostics().is_empty());
    }

    fn declaration_name(diagnostic: &bray_diagnostics::Diagnostic) -> &str {
        match diagnostic
            .args()
            .iter()
            .find(|arg| arg.name() == DiagnosticArgName::DeclarationName)
            .map(|arg| arg.value())
        {
            Some(DiagnosticArgValue::DeclarationName(name)) => name,
            value => panic!("expected declaration-name argument: {value:?}"),
        }
    }
}
