use bray_diagnostics::{DiagnosticFailureField, DiagnosticSemanticQueryFailure};

use super::diagnostic_context::{count_field, identity_field, natural_field, text_field};

pub(crate) fn diagnostic_symbol_graph_failure(
    error: bray_symbols::SymbolGraphBuildError,
) -> DiagnosticSemanticQueryFailure {
    use bray_symbols::SymbolGraphBuildError as Error;

    let (reason, context) = match error {
        Error::CompilerKnown(cause) => diagnostic_compiler_known_symbol_failure(cause),
        Error::SymbolCapacityExceeded { index } => (
            "symbol_graph_capacity_exceeded",
            vec![natural_field("index", index)],
        ),
        Error::MissingContainer {
            declaration,
            container,
        } => (
            "symbol_graph_missing_container",
            vec![
                identity_field("declaration", &declaration),
                identity_field("container", &container),
            ],
        ),
        Error::MissingModuleOwner {
            declaration,
            container,
        } => (
            "symbol_graph_missing_module_owner",
            vec![
                identity_field("declaration", &declaration),
                identity_field("container", &container),
            ],
        ),
        Error::MissingModulePath { container } => (
            "symbol_graph_missing_module_path",
            vec![identity_field("container", &container)],
        ),
        Error::MissingModulePart {
            container,
            module_part,
        } => (
            "symbol_graph_missing_module_part",
            vec![
                identity_field("container", &container),
                identity_field("module_part", &module_part),
            ],
        ),
        Error::MissingContainingDeclaration {
            declaration,
            container,
        } => (
            "symbol_graph_missing_containing_declaration",
            vec![
                identity_field("declaration", &declaration),
                identity_field("container", &container),
            ],
        ),
        Error::MissingContainingSymbol {
            declaration,
            containing_declaration,
        } => (
            "symbol_graph_missing_containing_symbol",
            vec![
                identity_field("declaration", &declaration),
                identity_field("containing_declaration", &containing_declaration),
            ],
        ),
        Error::MissingSourceSymbol { declaration } => (
            "symbol_graph_missing_source_symbol",
            vec![identity_field("declaration", &declaration)],
        ),
        Error::InvalidSourceSymbolKind {
            declaration,
            declaration_kind,
            symbol_kind,
        } => (
            "symbol_graph_invalid_source_symbol_kind",
            vec![
                identity_field("declaration", &declaration),
                text_field("declaration_kind", declaration_kind.as_str()),
                text_field("symbol_kind", symbol_kind.as_str()),
            ],
        ),
        Error::MissingRecoveredModuleAnchor { container } => (
            "symbol_graph_missing_recovered_module_anchor",
            vec![identity_field("container", &container)],
        ),
    };

    DiagnosticSemanticQueryFailure::new("symbol_graph", reason, context)
}

fn diagnostic_compiler_known_symbol_failure(
    error: bray_symbols::CompilerKnownSymbolBuildError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    use bray_symbols::CompilerKnownSymbolBuildError as Error;

    match error {
        Error::NonCanonicalScopeId { expected, actual } => (
            "symbol_graph_compiler_known_non_canonical_scope_id",
            vec![
                count_field("expected_scope", u64::from(expected.raw())),
                count_field("actual_scope", u64::from(actual.raw())),
            ],
        ),
        Error::NonCanonicalDeclarationId { expected, actual } => (
            "symbol_graph_compiler_known_non_canonical_declaration_id",
            vec![
                count_field("expected_declaration", u64::from(expected.raw())),
                count_field("actual_declaration", u64::from(actual.raw())),
            ],
        ),
        Error::SymbolCapacityExceeded { index } => (
            "symbol_graph_compiler_known_symbol_capacity_exceeded",
            vec![natural_field("index", index)],
        ),
        Error::DuplicateAmbientScope { duplicate } => (
            "symbol_graph_compiler_known_duplicate_ambient_scope",
            vec![count_field("duplicate_scope", u64::from(duplicate.raw()))],
        ),
        Error::MissingAmbientScope => (
            "symbol_graph_compiler_known_missing_ambient_scope",
            Vec::new(),
        ),
        Error::InvalidModulePath { scope } => (
            "symbol_graph_compiler_known_invalid_module_path",
            vec![count_field("scope", u64::from(scope.raw()))],
        ),
        Error::InvalidModuleHierarchy => (
            "symbol_graph_compiler_known_invalid_module_hierarchy",
            Vec::new(),
        ),
        Error::MissingScopeOwner { declaration, scope } => (
            "symbol_graph_compiler_known_missing_scope_owner",
            vec![
                count_field("declaration", u64::from(declaration.raw())),
                count_field("scope", u64::from(scope.raw())),
            ],
        ),
        Error::MissingDeclarationOwner { declaration, owner } => (
            "symbol_graph_compiler_known_missing_declaration_owner",
            vec![
                count_field("declaration", u64::from(declaration.raw())),
                count_field("owner", u64::from(owner.raw())),
            ],
        ),
        Error::MissingRoleDeclarationSymbol { declaration } => (
            "symbol_graph_compiler_known_missing_role_declaration_symbol",
            vec![count_field("declaration", u64::from(declaration.raw()))],
        ),
        Error::InvalidOperationRoleSymbol { declaration } => (
            "symbol_graph_compiler_known_invalid_operation_role_symbol",
            vec![count_field("declaration", u64::from(declaration.raw()))],
        ),
        Error::InvalidIterationRoleSymbol { declaration } => (
            "symbol_graph_compiler_known_invalid_iteration_role_symbol",
            vec![count_field("declaration", u64::from(declaration.raw()))],
        ),
        Error::MissingIterationRole { role } => (
            "symbol_graph_compiler_known_missing_iteration_role",
            vec![text_field("role", role.as_str())],
        ),
        Error::MissingTargetProperty { property } => (
            "symbol_graph_compiler_known_missing_target_property",
            vec![text_field("property", property.as_str())],
        ),
        Error::DeclarationOwnerCycle { declaration } => (
            "symbol_graph_compiler_known_declaration_owner_cycle",
            vec![count_field("declaration", u64::from(declaration.raw()))],
        ),
        Error::InvalidDeclarationKind {
            declaration,
            catalog_kind,
            owner_kind,
        } => (
            "symbol_graph_compiler_known_invalid_declaration_kind",
            vec![
                count_field("declaration", u64::from(declaration.raw())),
                text_field(
                    "catalog_kind",
                    compiler_known_declaration_kind(catalog_kind),
                ),
                text_field("owner_kind", owner_kind.as_str()),
            ],
        ),
        Error::InvalidDeclarationSurface { declaration } => (
            "symbol_graph_compiler_known_invalid_declaration_surface",
            vec![count_field("declaration", u64::from(declaration.raw()))],
        ),
    }
}

const fn compiler_known_declaration_kind(
    kind: bray_compiler_known::CatalogDeclarationKind,
) -> &'static str {
    use bray_compiler_known::CatalogDeclarationKind as Kind;

    match kind {
        Kind::TrustedCapability => "trusted_capability",
        Kind::Constant => "constant",
        Kind::Function => "function",
        Kind::Predicate => "predicate",
        Kind::CallableContract => "callable_contract",
        Kind::CallableOverload => "callable_overload",
        Kind::ImplementationOverload => "implementation_overload",
        Kind::Struct => "struct",
        Kind::Union => "union",
        Kind::Trait => "trait",
        Kind::InherentImplementation => "inherent_implementation",
        Kind::UnnamedTraitImplementation => "unnamed_trait_implementation",
        Kind::NamedTraitImplementation => "named_trait_implementation",
        Kind::StructField => "struct_field",
        Kind::UnionVariant => "union_variant",
        Kind::UnionPayloadField => "union_payload_field",
        Kind::TypeConstructorMember => "type_constructor_member",
        Kind::TypeCallableMember => "type_callable_member",
        Kind::FinalizerMember => "finalizer_member",
        Kind::DestructorMember => "destructor_member",
        Kind::ScopeEnterMember => "scope_enter_member",
        Kind::ScopeExitMember => "scope_exit_member",
        Kind::TraitConstantMember => "trait_constant_member",
        Kind::TraitTypeMember => "trait_type_member",
        Kind::TraitPredicateMember => "trait_predicate_member",
        Kind::TraitCallableMember => "trait_callable_member",
        Kind::TraitFinalizerRequirement => "trait_finalizer_requirement",
        Kind::TraitDestructorRequirement => "trait_destructor_requirement",
        Kind::TraitScopeEnterRequirement => "trait_scope_enter_requirement",
        Kind::TraitScopeExitRequirement => "trait_scope_exit_requirement",
        Kind::ImplementationTypeMemberBinding => "implementation_type_member_binding",
    }
}
