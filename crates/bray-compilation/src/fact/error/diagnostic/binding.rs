use bray_diagnostics::{DiagnosticBindingFailure, DiagnosticFailureField, DiagnosticFailureValue};

use super::diagnostic_context::{
    boolean_field, count_field, identity_field, natural_field, push_symbol, text_field,
};
use super::diagnostic_semantic_value_failure;
use super::semantic_diagnostic::{
    callable_signature_reason, generic_substitution_reason, push_generic_substitution_failure,
    push_semantic_value_failure,
};

pub(crate) fn diagnostic_binding_failure(
    error: &bray_binder::BoundUnitBindingError,
) -> DiagnosticBindingFailure {
    use bray_binder::BoundUnitBindingError as Error;

    let (reason, context) = match error {
        Error::Cancelled | Error::CheckerInfrastructure(_) => {
            unreachable!("fact binding failures contain only binding-owned causes")
        }
        Error::Upstream(error) => match *error {},
        Error::InvalidUnitKey => ("binding_invalid_unit_key", Vec::new()),
        Error::MissingSyntax { source } => (
            "binding_missing_syntax",
            diagnostic_bound_source("source", *source),
        ),
        Error::MissingOwner {
            source,
            owner,
            symbol,
        } => {
            let mut context = diagnostic_bound_source("source", *source);
            context.push(identity_field("owner", owner));

            if let Some(symbol) = symbol {
                push_symbol(
                    &mut context,
                    "related_symbol_kind",
                    "related_symbol",
                    *symbol,
                );
            }

            ("binding_missing_owner", context)
        }
        Error::MissingModule { source, owner } => {
            let mut context = diagnostic_bound_source("source", *source);
            push_symbol(&mut context, "owner_kind", "owner", *owner);

            ("binding_missing_module", context)
        }
        Error::InvalidSurfaceName { source, symbol } => {
            let mut context = diagnostic_bound_source("source", *source);
            push_symbol(&mut context, "symbol_kind", "symbol", *symbol);

            ("binding_invalid_surface_name", context)
        }
        Error::SemanticValue(error) => {
            return DiagnosticBindingFailure::semantic_value(diagnostic_semantic_value_failure(
                *error,
            ));
        }
        Error::Construction(error) => diagnostic_bound_unit_construction_failure(error),
        Error::Binding(error) => diagnostic_nested_binding_failure(error),
        Error::Assembly(error) => diagnostic_bound_unit_assembly_failure(error),
    };

    DiagnosticBindingFailure::new(reason, context)
}

fn diagnostic_bound_source(
    name: &'static str,
    source: bray_bound_tree::BoundSourceAnchor,
) -> Vec<DiagnosticFailureField> {
    let syntax = source.syntax();
    let mut context = vec![identity_field(name, &source)];

    context.push(DiagnosticFailureField::new(
        "source_id",
        DiagnosticFailureValue::Count(u64::from(syntax.source_id().raw())),
    ));

    context.push(DiagnosticFailureField::new(
        "source_start",
        DiagnosticFailureValue::Count(u64::from(syntax.full_range().start().bytes())),
    ));

    context.push(DiagnosticFailureField::new(
        "source_end",
        DiagnosticFailureValue::Count(u64::from(syntax.full_range().end().bytes())),
    ));

    context.push(DiagnosticFailureField::new(
        "source_version",
        DiagnosticFailureValue::Count(source.source_version().raw()),
    ));

    context
}

fn diagnostic_bound_unit_construction_failure(
    error: &bray_binder::BoundUnitConstructionError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    use bray_binder::BoundUnitConstructionError as Error;

    match error {
        Error::BoundTree(error) => diagnostic_bound_tree_build_failure(*error),
        Error::LocalSymbol(error) => (
            local_symbol_build_reason(*error),
            vec![text_field(
                "local_symbol_cause",
                local_symbol_build_cause(*error),
            )],
        ),
        Error::LocalAlreadyActivated(symbol) => {
            let mut context = vec![identity_field("local_symbol", symbol)];
            context.push(text_field("local_symbol_kind", symbol.kind().as_str()));

            ("binding_construction_local_already_activated", context)
        }
        Error::UnknownSurfaceSymbol(symbol) => {
            let mut context = Vec::new();
            push_symbol(&mut context, "symbol_kind", "symbol", *symbol);

            ("binding_construction_unknown_surface_symbol", context)
        }
        Error::AnonymousCallableBoundaryMismatch => (
            "binding_construction_anonymous_callable_boundary_mismatch",
            Vec::new(),
        ),
        Error::AnonymousCallableAlreadyAssigned {
            introduction_scope,
            ordinal,
        } => {
            let mut context = vec![identity_field("introduction_scope", introduction_scope)];

            if let Some(ordinal) = ordinal {
                context.push(count_field("ordinal", u64::from(ordinal.raw())));
            }

            (
                "binding_construction_anonymous_callable_already_assigned",
                context,
            )
        }
        Error::AnonymousCallableParameterAlreadyAssigned { callable, ordinal } => (
            "binding_construction_anonymous_callable_parameter_already_assigned",
            vec![
                identity_field("callable", callable),
                count_field("ordinal", u64::from(ordinal.raw())),
            ],
        ),
        Error::AnonymousCallableSourceMismatch { expected, actual } => (
            "binding_construction_anonymous_callable_source_mismatch",
            vec![
                count_field("expected_source", u64::from(expected.raw())),
                count_field("actual_source", u64::from(actual.raw())),
            ],
        ),
        Error::AnonymousCallableSourceVersionMismatch { expected, actual } => (
            "binding_construction_anonymous_callable_source_version_mismatch",
            vec![
                count_field("expected_source_version", expected.raw()),
                count_field("actual_source_version", actual.raw()),
            ],
        ),
    }
}

fn diagnostic_bound_tree_build_failure(
    error: bray_bound_tree::BoundTreeBuildError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    use bray_bound_tree::BoundTreeBuildError as Error;

    match error {
        Error::ArenaCapacityExceeded(kind) => (
            "binding_construction_bound_tree_capacity_exceeded",
            vec![text_field("node_kind", kind.as_str())],
        ),
        Error::ForeignNode {
            expected,
            actual,
            kind,
        } => (
            "binding_construction_bound_tree_foreign_node",
            vec![
                count_field("expected_unit", u64::from(expected.raw())),
                count_field("actual_unit", u64::from(actual.raw())),
                text_field("node_kind", kind.as_str()),
            ],
        ),
        Error::MissingNode { kind, slot } => (
            "binding_construction_bound_tree_missing_node",
            vec![
                text_field("node_kind", kind.as_str()),
                count_field("node_slot", u64::from(slot)),
            ],
        ),
    }
}

fn diagnostic_nested_binding_failure(
    error: &bray_binder::BindingError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    use bray_binder::BindingError as Error;

    match error {
        Error::Construction(error) => return diagnostic_bound_unit_construction_failure(error),
        Error::Assembly(error) => return diagnostic_bound_unit_assembly_failure(error),
        Error::CallableSignature(cause) => {
            let mut context = Vec::new();

            if let bray_symbols::CallableSignatureTemplateError::SemanticValue(cause) = cause {
                push_semantic_value_failure(&mut context, *cause);
            }

            return (callable_signature_reason(cause), context);
        }
        Error::GenericSubstitution(cause) => {
            let mut context = Vec::new();
            push_generic_substitution_failure(&mut context, cause);

            return (generic_substitution_reason(cause), context);
        }
        _ => {}
    }

    let reason = match error {
        Error::Cancelled
        | Error::CheckerInfrastructure(_)
        | Error::SemanticValue(_)
        | Error::Upstream(_) => unreachable!("nested binding retains binding-owned causes"),
        Error::DependencyUnavailable => "binding_dependency_unavailable",
        Error::MissingSyntax { .. } => "binding_missing_syntax",
        Error::MissingOwner { .. } => "binding_missing_owner",
        Error::MissingModule { .. } => "binding_missing_module",
        Error::InvalidSurfaceName { .. } => "binding_invalid_surface_name",
        Error::IdentityCapacityExceeded => "binding_identity_capacity_exceeded",
        Error::RollbackFailed => "binding_rollback_failed",
        Error::TransactionContextMismatch => "binding_transaction_context_mismatch",
        Error::ControlTargetMismatch => "binding_control_target_mismatch",
        Error::UnsupportedSyntax => "binding_unsupported_syntax",
        Error::SyntaxContract(_) => "binding_syntax_contract",
        Error::GenericOwnerUnavailable(_) => "binding_generic_owner_unavailable",
        Error::CompilerKnownRepresentationUnavailable(_) => {
            "binding_compiler_known_representation_unavailable"
        }
        Error::SymbolRecordUnavailable(_) => "binding_symbol_record_unavailable",
        Error::ModulePartRecordUnavailable(_) => "binding_module_part_record_unavailable",
        Error::DeclarationRecordUnavailable(_) => "binding_declaration_record_unavailable",
        Error::ContextualSelfUnavailable(_) => "binding_contextual_self_unavailable",
        Error::UnresolvedTypeTemplate => "binding_unresolved_type_template",
        Error::InvalidUnitKey { .. } => "binding_invalid_unit_key",
        Error::CallableParameterCountMismatch { .. } => "binding_callable_parameter_count_mismatch",
        Error::CallableParameterOwnerMismatch { .. } => "binding_callable_parameter_owner_mismatch",
        Error::ReceiverParameterOwnerMismatch { .. } => "binding_receiver_parameter_owner_mismatch",
        Error::ReceiverContextMismatch { .. } => "binding_receiver_context_mismatch",
        Error::CallableTypeExpected { .. } => "binding_callable_type_expected",
        Error::CallableTypeTemplateExpected(_) => "binding_callable_type_template_expected",
        Error::CompilerKnownHeapStoragePolicyUnavailable => {
            "binding_compiler_known_heap_storage_policy_unavailable"
        }
        Error::ImportedPackageUnavailable(_) => "binding_imported_package_unavailable",
        Error::ImportedPathUnavailable { .. } => "binding_imported_path_unavailable",
        Error::BoundWalkStopped(_) => "binding_bound_walk_stopped",
        Error::Construction(_)
        | Error::Assembly(_)
        | Error::CallableSignature(_)
        | Error::GenericSubstitution(_) => unreachable!("nested causes return above"),
    };

    let mut context = Vec::new();
    push_nested_binding_context(&mut context, error);

    (reason, context)
}

fn push_nested_binding_context(
    context: &mut Vec<DiagnosticFailureField>,
    error: &bray_binder::BindingError,
) {
    use bray_binder::BindingError as Error;

    match error {
        Error::MissingSyntax { source } => {
            context.extend(diagnostic_bound_source("source", *source))
        }
        Error::MissingOwner {
            source,
            owner,
            symbol,
        } => {
            context.extend(diagnostic_bound_source("source", *source));
            context.push(identity_field("owner", owner));

            if let Some(symbol) = symbol {
                push_symbol(context, "related_symbol_kind", "related_symbol", *symbol);
            }
        }
        Error::MissingModule { source, owner } => {
            context.extend(diagnostic_bound_source("source", *source));
            push_symbol(context, "owner_kind", "owner", *owner);
        }
        Error::InvalidSurfaceName { source, symbol } => {
            context.extend(diagnostic_bound_source("source", *source));
            push_symbol(context, "symbol_kind", "symbol", *symbol);
        }
        Error::SyntaxContract(source)
        | Error::ContextualSelfUnavailable(source)
        | Error::CallableTypeTemplateExpected(source) => {
            push_syntax_anchor(context, "source", *source);
        }
        Error::GenericOwnerUnavailable(symbol) | Error::SymbolRecordUnavailable(symbol) => {
            push_symbol(context, "symbol_kind", "symbol", *symbol);
        }
        Error::CompilerKnownRepresentationUnavailable(role) => {
            context.push(text_field("representation_role", role.as_str()));
        }
        Error::ModulePartRecordUnavailable(part) => {
            context.push(identity_field("module_part", part));
        }
        Error::DeclarationRecordUnavailable(declaration) => {
            context.push(identity_field("declaration", declaration));
        }
        Error::InvalidUnitKey { source, owner } => {
            context.extend(diagnostic_bound_source("source", *source));
            push_symbol(context, "owner_kind", "owner", *owner);
        }
        Error::CallableParameterCountMismatch {
            source,
            callable,
            syntax_count,
            symbol_count,
        } => {
            push_syntax_anchor(context, "source", *source);
            push_symbol(context, "callable_kind", "callable", (*callable).into_any());
            context.push(natural_field("syntax_count", *syntax_count));
            context.push(natural_field("symbol_count", *symbol_count));
        }
        Error::CallableParameterOwnerMismatch {
            source,
            callable,
            parameter,
        } => {
            push_syntax_anchor(context, "source", *source);
            push_symbol(context, "callable_kind", "callable", (*callable).into_any());
            push_symbol(context, "parameter_kind", "parameter", (*parameter).into());
        }
        Error::ReceiverParameterOwnerMismatch {
            source,
            callable,
            receiver,
        } => {
            push_syntax_anchor(context, "source", *source);
            push_symbol(context, "callable_kind", "callable", (*callable).into_any());
            push_symbol(context, "receiver_kind", "receiver", (*receiver).into());
        }
        Error::ReceiverContextMismatch {
            source,
            callable,
            receiver_present,
            mode_present,
            self_type_present,
        } => {
            push_syntax_anchor(context, "source", *source);
            push_symbol(context, "callable_kind", "callable", (*callable).into_any());

            push_receiver_context_flags(
                context,
                *receiver_present,
                *mode_present,
                *self_type_present,
            );
        }
        Error::CallableTypeExpected { source, ty } => {
            push_syntax_anchor(context, "source", *source);
            context.push(identity_field("semantic_type", ty));
        }
        Error::ImportedPackageUnavailable(package) => {
            context.push(text_field("package", package.as_str()));
        }
        Error::ImportedPathUnavailable {
            package,
            component_count,
        } => {
            push_symbol(context, "package_kind", "package", (*package).into());
            context.push(natural_field("component_count", *component_count));
        }
        Error::BoundWalkStopped(root) => context.push(identity_field("root", root)),
        Error::Cancelled
        | Error::CheckerInfrastructure(_)
        | Error::SemanticValue(_)
        | Error::Upstream(_)
        | Error::DependencyUnavailable
        | Error::IdentityCapacityExceeded
        | Error::RollbackFailed
        | Error::TransactionContextMismatch
        | Error::ControlTargetMismatch
        | Error::UnsupportedSyntax
        | Error::UnresolvedTypeTemplate
        | Error::CompilerKnownHeapStoragePolicyUnavailable
        | Error::Construction(_)
        | Error::Assembly(_)
        | Error::CallableSignature(_)
        | Error::GenericSubstitution(_) => {}
    }
}

fn push_syntax_anchor(
    context: &mut Vec<DiagnosticFailureField>,
    name: &'static str,
    source: bray_declarations::SyntaxAnchor,
) {
    context.push(identity_field(name, &source));

    context.push(count_field(
        "source_id",
        u64::from(source.source_id().raw()),
    ));

    context.push(count_field(
        "source_start",
        u64::from(source.full_range().start().bytes()),
    ));

    context.push(count_field(
        "source_end",
        u64::from(source.full_range().end().bytes()),
    ));
}

fn diagnostic_bound_unit_assembly_failure(
    error: &bray_binder::BoundUnitAssemblyError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    let bray_binder::BoundUnitAssemblyError::InvalidBoundUnit(error) = error;

    diagnostic_bound_unit_build_failure(*error)
}

fn diagnostic_bound_unit_build_failure(
    error: bray_bound_tree::BoundUnitBuildError,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    use bray_bound_tree::BoundUnitBuildError as Error;

    match error {
        Error::RootKindMismatch => ("binding_assembly_root_kind_mismatch", Vec::new()),
        Error::MissingRoot { unit, kind } => (
            "binding_assembly_missing_root",
            vec![
                count_field("unit", u64::from(unit.raw())),
                text_field("root_kind", kind.as_str()),
            ],
        ),
        Error::LocalSymbolRegionMismatch => {
            ("binding_assembly_local_symbol_region_mismatch", Vec::new())
        }
        Error::AnonymousCallableRegionMismatch { expected, actual } => (
            "binding_assembly_anonymous_callable_region_mismatch",
            vec![
                count_field("expected_region", u64::from(expected.raw())),
                count_field("actual_region", u64::from(actual.raw())),
            ],
        ),
        Error::MissingAnonymousCallable { callable } => (
            "binding_assembly_missing_anonymous_callable",
            vec![identity_field("callable", &callable)],
        ),
        Error::InvalidNestedUnit { index } => (
            "binding_assembly_invalid_nested_unit",
            vec![natural_field("index", index)],
        ),
        Error::NonCanonicalNestedUnits { index } => (
            "binding_assembly_non_canonical_nested_units",
            vec![natural_field("index", index)],
        ),
    }
}

fn push_receiver_context_flags(
    context: &mut Vec<DiagnosticFailureField>,
    receiver_present: bool,
    mode_present: bool,
    self_type_present: bool,
) {
    context.push(boolean_field("receiver_present", receiver_present));
    context.push(boolean_field("mode_present", mode_present));
    context.push(boolean_field("self_type_present", self_type_present));
}

const fn local_symbol_build_reason(error: bray_symbols::LocalSymbolBuildError) -> &'static str {
    match error {
        bray_symbols::LocalSymbolBuildError::ForeignRegion => {
            "binding_construction_local_symbol_foreign_region"
        }
        bray_symbols::LocalSymbolBuildError::UnknownScope => {
            "binding_construction_local_symbol_unknown_scope"
        }
        bray_symbols::LocalSymbolBuildError::UnknownAnonymousCallable => {
            "binding_construction_local_symbol_unknown_anonymous_callable"
        }
        bray_symbols::LocalSymbolBuildError::UnknownLocalSymbol => {
            "binding_construction_local_symbol_unknown_local_symbol"
        }
        bray_symbols::LocalSymbolBuildError::MissingParentScope => {
            "binding_construction_local_symbol_missing_parent_scope"
        }
        bray_symbols::LocalSymbolBuildError::RootHasParentScope => {
            "binding_construction_local_symbol_root_has_parent_scope"
        }
        bray_symbols::LocalSymbolBuildError::DuplicateRootScope => {
            "binding_construction_local_symbol_duplicate_root_scope"
        }
        bray_symbols::LocalSymbolBuildError::MissingRootScope => {
            "binding_construction_local_symbol_missing_root_scope"
        }
        bray_symbols::LocalSymbolBuildError::SymbolOutsideScope => {
            "binding_construction_local_symbol_outside_scope"
        }
        bray_symbols::LocalSymbolBuildError::DuplicatePostconditionResult => {
            "binding_construction_local_symbol_duplicate_postcondition_result"
        }
        bray_symbols::LocalSymbolBuildError::InvalidScopeBoundary => {
            "binding_construction_local_symbol_invalid_scope_boundary"
        }
        bray_symbols::LocalSymbolBuildError::InvalidAnonymousCallableScope => {
            "binding_construction_local_symbol_invalid_anonymous_callable_scope"
        }
        bray_symbols::LocalSymbolBuildError::AnonymousCallableScopeAlreadyAssigned => {
            "binding_construction_local_symbol_anonymous_callable_scope_already_assigned"
        }
        bray_symbols::LocalSymbolBuildError::AnonymousCallableParameterScopeMismatch => {
            "binding_construction_local_symbol_anonymous_callable_parameter_scope_mismatch"
        }
        bray_symbols::LocalSymbolBuildError::SymbolHasNoOrdinaryName => {
            "binding_construction_local_symbol_has_no_ordinary_name"
        }
        bray_symbols::LocalSymbolBuildError::CapacityExceeded => {
            "binding_construction_local_symbol_capacity_exceeded"
        }
        bray_symbols::LocalSymbolBuildError::MissingSyntaxAnchor => {
            "binding_construction_local_symbol_missing_syntax_anchor"
        }
    }
}

const fn local_symbol_build_cause(error: bray_symbols::LocalSymbolBuildError) -> &'static str {
    match error {
        bray_symbols::LocalSymbolBuildError::ForeignRegion => "foreign_region",
        bray_symbols::LocalSymbolBuildError::UnknownScope => "unknown_scope",
        bray_symbols::LocalSymbolBuildError::UnknownAnonymousCallable => {
            "unknown_anonymous_callable"
        }
        bray_symbols::LocalSymbolBuildError::UnknownLocalSymbol => "unknown_local_symbol",
        bray_symbols::LocalSymbolBuildError::MissingParentScope => "missing_parent_scope",
        bray_symbols::LocalSymbolBuildError::RootHasParentScope => "root_has_parent_scope",
        bray_symbols::LocalSymbolBuildError::DuplicateRootScope => "duplicate_root_scope",
        bray_symbols::LocalSymbolBuildError::MissingRootScope => "missing_root_scope",
        bray_symbols::LocalSymbolBuildError::SymbolOutsideScope => "symbol_outside_scope",
        bray_symbols::LocalSymbolBuildError::DuplicatePostconditionResult => {
            "duplicate_postcondition_result"
        }
        bray_symbols::LocalSymbolBuildError::InvalidScopeBoundary => "invalid_scope_boundary",
        bray_symbols::LocalSymbolBuildError::InvalidAnonymousCallableScope => {
            "invalid_anonymous_callable_scope"
        }
        bray_symbols::LocalSymbolBuildError::AnonymousCallableScopeAlreadyAssigned => {
            "anonymous_callable_scope_already_assigned"
        }
        bray_symbols::LocalSymbolBuildError::AnonymousCallableParameterScopeMismatch => {
            "anonymous_callable_parameter_scope_mismatch"
        }
        bray_symbols::LocalSymbolBuildError::SymbolHasNoOrdinaryName => {
            "symbol_has_no_ordinary_name"
        }
        bray_symbols::LocalSymbolBuildError::CapacityExceeded => "capacity_exceeded",
        bray_symbols::LocalSymbolBuildError::MissingSyntaxAnchor => "missing_syntax_anchor",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;

    use super::push_receiver_context_flags;

    #[test]
    fn receiver_context_presence_flags_remain_booleans() {
        let mut context = Vec::new();
        push_receiver_context_flags(&mut context, true, false, true);

        assert_eq!(context[0].value(), &DiagnosticFailureValue::Boolean(true));
        assert_eq!(context[1].value(), &DiagnosticFailureValue::Boolean(false));
        assert_eq!(context[2].value(), &DiagnosticFailureValue::Boolean(true));
    }
}
