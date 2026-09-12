use bray_diagnostics::DiagnosticFailureField;

use super::diagnostic_context::{
    boolean_field, count_field, identity_field, identity_list_field, path_field, push_symbol,
    text_field, text_list_field,
};
use crate::compilation::SemanticQueryContext;

pub(super) fn semantic_query_context(
    context: &SemanticQueryContext,
) -> Vec<DiagnosticFailureField> {
    use SemanticQueryContext as Context;

    let mut fields = Vec::new();

    let kind = match context {
        Context::Fact(fact) => {
            push_fact_context(&mut fields, fact);

            "fact"
        }
        Context::SymbolQuery(query) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", query.symbol());
            fields.push(text_field("query_kind", query.kind().as_str()));

            "symbol_query"
        }
        Context::Symbol(symbol) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);

            "symbol"
        }
        Context::SymbolKey(key) => {
            fields.extend([
                text_field("symbol_key_kind", key.kind().as_str()),
                identity_field("symbol_key", key),
            ]);

            "symbol_key"
        }
        Context::Unit(unit) => {
            push_bound_unit_key(&mut fields, unit);

            "unit"
        }
        Context::Expression { unit, expression } => {
            push_bound_unit_key(&mut fields, unit);
            push_bound_expression(&mut fields, *expression);

            "expression"
        }
        Context::BoundExpression { unit, expression } => {
            fields.push(count_field("bound_unit", u64::from(unit.raw())));
            push_bound_expression(&mut fields, *expression);

            "bound_expression"
        }
        Context::CompilerKnownDeclaration(declaration) => {
            fields.push(text_field(
                "compiler_known_declaration",
                declaration.as_str(),
            ));

            "compiler_known_declaration"
        }
        Context::CompilerKnownDeclarationName(name) => {
            fields.push(text_field("declaration_name", *name));

            "compiler_known_declaration_name"
        }
        Context::CompilerKnownRepresentation(role) => {
            fields.push(text_field("representation_role", role.as_str()));

            "compiler_known_representation"
        }
        Context::Declaration(declaration) => {
            fields.push(identity_field("declaration", declaration));

            "declaration"
        }
        Context::LocalReference {
            unit,
            expression,
            local,
        } => {
            push_bound_unit_key(&mut fields, unit);
            push_bound_expression(&mut fields, *expression);
            push_local_symbol(&mut fields, *local);

            "local_reference"
        }
        Context::Source(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "source"
        }
        Context::Type(ty) => {
            fields.push(identity_field("semantic_type", ty));

            "type"
        }
        Context::ImplementationDomain(domain) => {
            fields.push(text_field("coherence_package", domain.package().as_str()));

            "implementation_domain"
        }
    };

    fields.insert(0, text_field("context_kind", kind));

    fields
}

// rust-style: allow(function-too-large, reason = "fact variants form one exhaustive flat conversion into typed diagnostic fields")
fn push_fact_context(
    fields: &mut Vec<DiagnosticFailureField>,
    fact: &crate::fact::CompilationFactKey,
) {
    use crate::fact::CompilationFactKey as Fact;

    let kind = match fact {
        Fact::TargetValidity(request) => {
            let source = request.source();

            push_syntax_anchor(fields, source.syntax());

            fields.push(count_field("source_version", source.source_version().raw()));

            fields.push(text_field(
                "target_requirement_kind",
                match request.requirement() {
                    bray_checker::TargetValidityRequirement::Representation(_) => "representation",
                    bray_checker::TargetValidityRequirement::CallableAbi(_) => "callable_abi",
                    bray_checker::TargetValidityRequirement::Layout(_) => "layout",
                },
            ));

            fields.push(identity_field(
                "target_validity_requirement",
                request.requirement(),
            ));

            "target_validity"
        }
        Fact::ModuleContributionGate(module) => {
            fields.push(count_field("module_part", u64::from(module.raw())));

            "module_contribution_gate"
        }
        Fact::CallableTypeDirectives(key) => {
            push_symbol(fields, "symbol_kind", "directive_owner", key.owner());
            push_syntax_anchor(fields, key.syntax());

            "callable_type_directives"
        }
        Fact::BoundUnit(unit) => {
            push_bound_unit_key(fields, unit);

            "bound_unit"
        }
        Fact::CheckDiagnostics => "check_diagnostics",
        Fact::ConstantTemplateKeys => "constant_template_keys",
        Fact::CallableBodyKeys => "callable_body_keys",
        Fact::PredicateDefinitionKeys => "predicate_definition_keys",
        Fact::ConstantInstance(key) => {
            let instance = key.instance();

            fields.extend([
                identity_field("constant_definition", &instance.definition()),
                identity_field("constant_substitution", &instance.substitution()),
                identity_field("constant_evaluation_limits", &key.limits()),
            ]);

            push_selected_implementation(fields, instance.selected_implementation());
            super::target_diagnostic::push_target_profile(fields, key.target());

            "constant_instance"
        }
        Fact::ConstantCall(key) => {
            push_constant_call_context(
                fields,
                key.callable(),
                key.selected_implementation(),
                key.arguments(),
                key.result_type(),
                key.target(),
            );

            fields.push(identity_field("constant_evaluation_limits", &key.limits()));

            "constant_call"
        }
        Fact::ConstantCallCycle(key) => {
            push_constant_call_context(
                fields,
                key.callable(),
                key.selected_implementation(),
                key.arguments(),
                key.result_type(),
                key.target(),
            );

            "constant_call_cycle"
        }
        Fact::CheckedControlFlow(unit) => {
            push_bound_unit_key(fields, unit);

            "checked_control_flow"
        }
        Fact::CheckedPatterns(unit) => {
            push_bound_unit_key(fields, unit);

            "checked_patterns"
        }
        Fact::StoragePlan(unit) => {
            push_bound_unit_key(fields, unit);

            "storage_plan"
        }
        Fact::MemoryOperations(unit) => {
            push_bound_unit_key(fields, unit);

            "memory_operations"
        }
        Fact::ExecutionCandidates(unit) => {
            push_bound_unit_key(fields, unit);

            "execution_candidates"
        }
        Fact::CertifiedExecution(unit) => {
            push_bound_unit_key(fields, unit);

            "certified_execution"
        }
        Fact::BodySemantics(unit) => {
            push_bound_unit_key(fields, unit);

            "body_semantics"
        }
        Fact::CheckedBodyBehavior(unit) => {
            push_bound_unit_key(fields, unit);

            "checked_body_behavior"
        }
        Fact::LoweredUnit(unit) => {
            push_bound_unit_key(fields, unit);

            "lowered_unit"
        }
        Fact::CodegenArtifact(key) => {
            super::codegen_context::push_codegen_artifact_fact_context(fields, key);

            "codegen_artifact"
        }
        Fact::NativeProduct(key) => {
            fields.extend([
                identity_field("product", key.product()),
                text_field("product_package", key.product().package().as_str()),
                text_field("product_name", key.product().name()),
            ]);

            push_build_configuration(fields, key.configuration());
            push_runtime_components(fields, key.runtime());

            fields.push(text_list_field(
                "required_runtime_capabilities",
                key.required_capabilities()
                    .iter()
                    .map(|capability| capability.as_str()),
            ));

            fields.extend([
                identity_list_field("linker_drivers", key.linker_drivers()),
                text_list_field(
                    "linker_driver_kinds",
                    key.linker_drivers()
                        .iter()
                        .map(|driver| linker_driver_kind(driver.kind())),
                ),
                text_list_field(
                    "linker_driver_names",
                    key.linker_drivers().iter().map(|driver| driver.name()),
                ),
                text_list_field(
                    "linker_driver_capability_revisions",
                    key.linker_drivers()
                        .iter()
                        .map(|driver| driver.capability_revision()),
                ),
                text_list_field(
                    "linker_driver_toolchain_revisions",
                    key.linker_drivers()
                        .iter()
                        .map(|driver| driver.toolchain_revision()),
                ),
            ]);

            "native_product"
        }
        Fact::DeclaredValueTypeTemplates(unit) => {
            push_bound_unit_key(fields, unit);

            "declared_value_type_templates"
        }
        Fact::ExpressionSemantics(unit) => {
            push_bound_unit_key(fields, unit);

            "expression_semantics"
        }
        Fact::ProvisionalExpressionSemantics(unit) => {
            push_bound_unit_key(fields, unit);

            "provisional_expression_semantics"
        }
        Fact::SymbolicConstantTerm(unit) => {
            push_bound_unit_key(fields, unit);

            "symbolic_constant_term"
        }
        Fact::DeclarationChunk(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "declaration_chunk"
        }
        Fact::DeclarationTable => "declaration_table",
        Fact::ProductSourceGraph => "product_source_graph",
        Fact::ProductSemantics => "product_semantics",
        Fact::TestDiscovery(product) => {
            fields.push(identity_field("product", product));

            "test_discovery"
        }
        Fact::DiscoverySymbolGraph => "discovery_symbol_graph",
        Fact::DependencyInterface(interface) => {
            fields.push(count_field(
                "imported_interface",
                u64::from(interface.raw()),
            ));

            "dependency_interface"
        }
        Fact::DependencyImplementation(interface) => {
            fields.push(count_field(
                "imported_interface",
                u64::from(interface.raw()),
            ));

            "dependency_implementation"
        }
        Fact::ImportedDiagnostics => "imported_diagnostics",
        Fact::ImplementationHeaderIndex => "implementation_header_index",
        Fact::ImplementationCandidateSet(requirement) => {
            push_implementation_requirement(fields, *requirement);

            "implementation_candidate_set"
        }
        Fact::TraitImplementationConformance(implementation) => {
            push_symbol(
                fields,
                "symbol_kind",
                "implementation",
                implementation.into_any(),
            );

            "trait_implementation_conformance"
        }
        Fact::GenericConstraintSatisfaction(obligation) => {
            fields.extend([
                identity_field("generic_constraint_owner", &obligation.owner()),
                identity_field(
                    "generic_constraint_substitution",
                    &obligation.substitution(),
                ),
            ]);

            "generic_constraint_satisfaction"
        }
        Fact::ImplementationSelection(requirement) => {
            push_implementation_requirement(fields, *requirement);

            "implementation_selection"
        }
        Fact::IterationSource(key) => {
            push_bound_unit_key(fields, key.unit());
            push_bound_expression(fields, key.expression());

            "iteration_source"
        }
        Fact::OperationSelection(key) => {
            push_bound_unit_key(fields, key.unit());
            push_bound_expression(fields, key.expression());

            "operation_selection"
        }
        Fact::ImportedSemanticGraph(interface) => {
            fields.push(count_field(
                "imported_interface",
                u64::from(interface.raw()),
            ));

            "imported_semantic_graph"
        }
        Fact::ImportedSemanticRecord(record) => {
            fields.extend([
                count_field("imported_interface", u64::from(record.interface().raw())),
                count_field("interface_symbol", u64::from(record.owner().raw())),
                text_field("semantic_record_kind", record.kind().as_str()),
            ]);

            "imported_semantic_record"
        }
        Fact::ImportedConstantCallableBody(address) => {
            fields.extend([
                count_field("imported_interface", u64::from(address.interface().raw())),
                count_field("interface_symbol", u64::from(address.symbol().raw())),
            ]);

            "imported_constant_callable_body"
        }
        Fact::ImportedExecutableTemplate(address) => {
            fields.extend([
                identity_field("imported_semantic_address", &address.symbol()),
                count_field("executable_template", u64::from(address.template().raw())),
            ]);

            "imported_executable_template"
        }
        Fact::ImplementationParticipation(domain) => {
            fields.push(identity_field("coherence_package", domain.package()));

            "implementation_participation"
        }
        Fact::ImplementationCoherence => "implementation_coherence",
        Fact::CallableOverloadValidation => "callable_overload_validation",
        Fact::ForeignCallableContract(function) => {
            push_symbol(fields, "symbol_kind", "function", (*function).into());

            "foreign_callable_contract"
        }
        Fact::ForeignStaticContract(value) => {
            push_symbol(fields, "symbol_kind", "static", (*value).into());

            "foreign_static_contract"
        }
        Fact::ForeignCallableValidation => "foreign_callable_validation",
        Fact::TypeAssociatedSurface(ty) => {
            push_symbol(fields, "symbol_kind", "named_type", ty.into_any());

            "type_associated_surface"
        }
        Fact::DeclaredTypeRepresentation(ty) => {
            push_symbol(fields, "symbol_kind", "named_type", ty.into_any());

            "declared_type_representation"
        }
        Fact::TypeAssociatedImplementationIndex => "type_associated_implementation_index",
        Fact::ImportedSymbolSkeleton => "imported_symbol_skeleton",
        Fact::PackageInterfaceExportBundle => "package_interface_export_bundle",
        Fact::SemanticDiagnostics => "semantic_diagnostics",
        Fact::SourceUnitSyntax(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "source_unit_syntax"
        }
        Fact::SourceReferenceIndex(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "source_reference_index"
        }
        Fact::SymbolGraph => "symbol_graph",
        Fact::Symbol(query) => {
            push_symbol(fields, "symbol_kind", "symbol", query.symbol());
            fields.push(text_field("query_kind", query.kind().as_str()));

            "symbol"
        }
        Fact::SyntaxTree => "syntax_tree",
    };

    fields.insert(0, text_field("fact_kind", kind));
}

fn push_bound_unit_key(
    fields: &mut Vec<DiagnosticFailureField>,
    unit: &bray_bound_tree::BoundUnitKey,
) {
    let source = unit.source();
    let syntax = source.syntax();

    fields.extend([
        text_field("unit_kind", unit.kind().as_str()),
        identity_field("unit", unit),
        count_field("unit_source", u64::from(syntax.source_id().raw())),
        count_field(
            "unit_source_start",
            u64::from(syntax.full_range().start().bytes()),
        ),
        count_field(
            "unit_source_end",
            u64::from(syntax.full_range().end().bytes()),
        ),
        count_field("unit_source_version", source.source_version().raw()),
        boolean_field("unit_source_recovered", syntax.is_recovered()),
    ]);
}

fn push_implementation_requirement(
    fields: &mut Vec<DiagnosticFailureField>,
    requirement: bray_symbols::ImplementationRequirementKey,
) {
    fields.extend([
        identity_field("implementation_subject", &requirement.subject()),
        identity_field(
            "implementation_trait_application",
            &requirement.trait_application(),
        ),
    ]);
}

fn push_constant_call_context(
    fields: &mut Vec<DiagnosticFailureField>,
    callable: bray_symbols::CallableInstanceId,
    selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
    arguments: &[bray_symbols::ConstantValueId],
    result_type: bray_symbols::TypeId,
    target: &bray_target::TargetProfile,
) {
    fields.extend([
        identity_field("constant_callable", &callable),
        identity_list_field("constant_arguments", arguments),
        identity_field("constant_result_type", &result_type),
    ]);

    push_selected_implementation(fields, selected_implementation);
    super::target_diagnostic::push_target_profile(fields, target);
}

fn push_selected_implementation(
    fields: &mut Vec<DiagnosticFailureField>,
    implementation: Option<bray_symbols::ImplementationInstanceId>,
) {
    fields.push(boolean_field(
        "selected_implementation_present",
        implementation.is_some(),
    ));

    if let Some(implementation) = implementation {
        fields.push(identity_field("selected_implementation", &implementation));
    }
}

fn push_build_configuration(
    fields: &mut Vec<DiagnosticFailureField>,
    configuration: crate::BuildConfiguration,
) {
    if let crate::BuildConfiguration::TimedRelease { inner_iterations } = configuration {
        fields.push(count_field(
            "build_inner_iterations",
            inner_iterations.get(),
        ));
    }

    fields.push(text_field("build_configuration", configuration.as_str()));
}

fn push_runtime_components(
    fields: &mut Vec<DiagnosticFailureField>,
    runtime: Option<&[crate::fact::RuntimeComponentQueryIdentity]>,
) {
    fields.push(boolean_field("runtime_selected", runtime.is_some()));

    let Some(runtime) = runtime else {
        return;
    };

    fields.extend([
        count_field(
            "runtime_component_count",
            u64::try_from(runtime.len()).unwrap_or(u64::MAX),
        ),
        text_list_field(
            "runtime_component_identities",
            runtime
                .iter()
                .map(|component| component.component().as_str()),
        ),
        text_list_field(
            "runtime_component_purposes",
            runtime.iter().map(|component| component.purpose().as_str()),
        ),
        text_list_field(
            "runtime_component_digests",
            runtime.iter().map(|component| component.digest().to_hex()),
        ),
    ]);

    for component in runtime {
        fields.push(path_field("runtime_component_archive", component.archive()));
    }
}

const fn linker_driver_kind(value: bray_linker::LinkerDriverKind) -> &'static str {
    match value {
        bray_linker::LinkerDriverKind::EmbeddedLld => "embedded_lld",
        bray_linker::LinkerDriverKind::ExternalLld => "external_lld",
        bray_linker::LinkerDriverKind::System => "system",
        bray_linker::LinkerDriverKind::Archiver => "archiver",
        bray_linker::LinkerDriverKind::TargetSpecific => "target_specific",
    }
}

fn push_syntax_anchor(
    fields: &mut Vec<DiagnosticFailureField>,
    syntax: bray_declarations::SyntaxAnchor,
) {
    fields.extend([
        count_field("source", u64::from(syntax.source_id().raw())),
        text_field("syntax_kind", syntax.syntax_kind().as_str()),
        count_field(
            "source_start",
            u64::from(syntax.full_range().start().bytes()),
        ),
        count_field("source_end", u64::from(syntax.full_range().end().bytes())),
        boolean_field("source_recovered", syntax.is_recovered()),
    ]);
}

fn push_bound_expression(
    fields: &mut Vec<DiagnosticFailureField>,
    expression: bray_bound_tree::BoundExpressionId,
) {
    fields.extend([
        count_field("expression_unit", u64::from(expression.unit().raw())),
        count_field("expression", u64::from(expression.ordinal())),
    ]);
}

fn push_local_symbol(
    fields: &mut Vec<DiagnosticFailureField>,
    local: bray_symbols::AnyLocalSymbolId,
) {
    use bray_symbols::AnyLocalSymbolId as Local;

    let ordinal = match local {
        Local::Binding(local) => local.ordinal(),
        Local::Constant(local) => local.ordinal(),
        Local::AnonymousCallable(local) => local.ordinal(),
        Local::AnonymousCallableParameter(local) => local.ordinal(),
        Local::PostconditionResult(local) => local.ordinal(),
    };

    fields.extend([
        text_field("local_kind", local.kind().as_str()),
        count_field("local_region", u64::from(local.region().raw())),
        count_field("local", u64::from(ordinal)),
    ]);
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;

    #[test]
    fn fact_context_preserves_variant_and_typed_component() {
        let fields =
            super::semantic_query_context(&crate::compilation::SemanticQueryContext::Fact(
                crate::fact::CompilationFactKey::SourceUnitSyntax(bray_source::SourceId::new(17)),
            ));

        assert_eq!(fields[0].name(), "context_kind");
        assert_eq!(fields[1].name(), "fact_kind");

        assert_eq!(
            fields[1].value(),
            &DiagnosticFailureValue::Text("source_unit_syntax".to_owned())
        );

        assert_eq!(fields[2].name(), "source");
        assert_eq!(fields[2].value(), &DiagnosticFailureValue::Count(17));
    }

    #[test]
    fn native_product_fact_context_decomposes_every_query_input_group() {
        let package = bray_symbols::PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = bray_symbols::ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let runtime_component = crate::fact::RuntimeComponentQueryIdentity::new(
            bray_runtime_interface::RuntimeArtifactId::try_new("runtime.product")
                .unwrap_or_else(|| panic!("test runtime identity must be valid")),
            bray_runtime_interface::RuntimeArtifactPurpose::Product,
            bray_runtime_interface::RuntimeArtifactDigest::new([9; 32]),
            std::path::PathBuf::from("runtime/product.lib"),
        );

        let linker = bray_linker::LinkerDriverIdentity::try_new(
            bray_linker::LinkerDriverKind::System,
            "system-linker",
            "capabilities-1",
            "toolchain-1",
        )
        .unwrap_or_else(|| panic!("test linker identity must be valid"));

        let key = crate::fact::NativeProductQueryKey::new(
            product,
            crate::BuildConfiguration::TimedRelease {
                inner_iterations: std::num::NonZeroU64::new(3)
                    .unwrap_or_else(|| panic!("test iteration count must be nonzero")),
            },
            Some(vec![runtime_component].into()),
            [bray_runtime_interface::RuntimeCapability::Reactor],
            [linker],
        );

        let fields =
            super::semantic_query_context(&crate::compilation::SemanticQueryContext::Fact(
                crate::fact::CompilationFactKey::NativeProduct(key),
            ));

        let names: Vec<_> = fields.iter().map(|field| field.name()).collect();

        for expected in [
            "product_package",
            "product_name",
            "build_configuration",
            "build_inner_iterations",
            "runtime_selected",
            "runtime_component_identities",
            "runtime_component_purposes",
            "runtime_component_digests",
            "runtime_component_archive",
            "required_runtime_capabilities",
            "linker_drivers",
            "linker_driver_kinds",
            "linker_driver_names",
            "linker_driver_capability_revisions",
            "linker_driver_toolchain_revisions",
        ] {
            assert!(
                names.contains(&expected),
                "missing native-product field {expected}"
            );
        }
    }
}
