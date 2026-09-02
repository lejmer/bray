use bray_diagnostics::DiagnosticFailureField;

use super::diagnostic_context::{
    boolean_field, count_field, identity_field, push_symbol, text_field,
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

            fields.push(count_field(
                "source_version",
                source.source_version().raw(),
            ));

            fields.push(text_field(
                "target_requirement_kind",
                match request.requirement() {
                    bray_checker::TargetValidityRequirement::Representation(_) => {
                        "representation"
                    }
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
            fields.push(identity_field("constant_instance_query", key));

            "constant_instance"
        }
        Fact::ConstantCall(key) => {
            fields.push(identity_field("constant_call_query", key));

            "constant_call"
        }
        Fact::ConstantCallCycle(key) => {
            fields.push(identity_field("constant_call_dependency", key));

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
            fields.extend([
                identity_field("codegen_artifact_query", key),
                identity_field("codegen_unit", key.unit()),
            ]);

            "codegen_artifact"
        }
        Fact::NativeProduct(key) => {
            fields.extend([
                identity_field("native_product_query", key),
                identity_field("product", key.product()),
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
            fields.push(count_field("imported_interface", u64::from(interface.raw())));

            "dependency_interface"
        }
        Fact::DependencyImplementation(interface) => {
            fields.push(count_field("imported_interface", u64::from(interface.raw())));

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
                identity_field("generic_constraint_substitution", &obligation.substitution()),
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
            fields.push(count_field("imported_interface", u64::from(interface.raw())));

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
        count_field(
            "source_end",
            u64::from(syntax.full_range().end().bytes()),
        ),
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
        let fields = super::semantic_query_context(&crate::compilation::SemanticQueryContext::Fact(
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
}
