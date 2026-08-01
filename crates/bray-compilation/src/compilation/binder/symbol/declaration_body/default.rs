use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey, CheckedTemplate, CheckedTemplateKind};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableParameterDefaultFact, CallableParameterDefaultSurface,
    CallableParameterDefaultTemplateFact, CallableParameterDefaultValue, CallableParameterSymbolId,
    CallableSignatureFact, CheckedCallableParameterDefault, CheckedStructFieldDefault,
    CheckedUnionPayloadDefault, ErrorCallableParameterDefault, ErrorStructFieldDefault,
    ErrorUnionPayloadDefault, ExactSymbolId, GenericOwnerId, GenericParameterSymbolId,
    GenericSubstitutionData, RuntimeDefaultBehavior,
    RuntimeDefaultGenericContext, RuntimeDefaultOwnership, RuntimeDefaultTemplateReference,
    StructFieldDefaultFact, StructFieldDefaultSurface, StructFieldDefaultTemplateFact,
    StructFieldDefaultValue, StructFieldSymbolId, SymbolFactRequest, SymbolFactResult,
    TrustedCapabilitySymbolId, TypeData, TypeId, UnevaluatedDefaultTemplate,
    UnionPayloadDefaultSurface, UnionPayloadDefaultValue, UnionPayloadFieldDefaultFact,
    UnionPayloadFieldDefaultTemplateFact, UnionPayloadFieldSymbolId,
};

use super::super::binding::CompilationSymbolFactBinding;
use super::super::cache::CompilationSymbolFacts;
use super::super::environment::visible_generic_parameters;
use super::super::imported::imported_declaration_template;
use super::lookup::{
    callable_parameter, runtime_default_provider, struct_field, union_payload_field, union_variant,
};
use super::shared::{checked_source_expression, syntax_diagnostics};
use crate::compilation::binder::CompilationBinderFacts;
use crate::compilation::substitution::generic_parameter_argument;
use crate::fact::SymbolFactCache;

impl_declaration_body_fact!(
    CallableParameterDefaultFact,
    callable_parameter_defaults,
    bind_callable_parameter_default
);
impl_declaration_body_fact!(
    StructFieldDefaultFact,
    struct_field_defaults,
    bind_struct_field_default
);
impl_declaration_body_fact!(
    UnionPayloadFieldDefaultFact,
    union_payload_field_defaults,
    bind_union_payload_field_default
);

struct RuntimeDefaultSummary {
    provider: AnySymbolId,
    result: TypeId,
    generic_context: RuntimeDefaultGenericContext,
    behavior: RuntimeDefaultBehavior,
    template: RuntimeDefaultTemplateReference,
    is_recovered: bool,
}

fn bind_callable_parameter_default(
    context: &CompilationBinderFacts<'_>,
    owner: CallableParameterSymbolId,
) -> BinderFactResult<DiagnosticResult<CheckedCallableParameterDefault>> {
    let template = context
        .symbol_fact(SymbolFactRequest::<CallableParameterDefaultTemplateFact>::new(owner))?;

    let (default, diagnostics) = checked_runtime_default(
        context,
        owner.into(),
        *template.value(),
        template.diagnostics(),
    )?
    .into_parts();

    let provider =
        bray_symbols::CallableParameterDefaultProviderSymbolId::try_from_any(default.provider)
            .ok_or(BinderFactError::DependencyUnavailable)?;

    let parameter = callable_parameter(context, owner)?;

    let signature = context.symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
        parameter.owner(),
    ))?;

    let earlier = signature
        .value()
        .parameters()
        .iter()
        .copied()
        .take(parameter.ordinal() as usize);

    let receiver = signature
        .value()
        .receiver()
        .map(|receiver| receiver.parameter());

    let value = if default.is_recovered {
        CallableParameterDefaultValue::Error(ErrorCallableParameterDefault)
    } else {
        CallableParameterDefaultValue::Valid(CallableParameterDefaultSurface::new(
            receiver,
            earlier,
            default.generic_context,
            default.result,
            default.behavior,
            default.template,
        ))
    };

    Ok(DiagnosticResult::new(
        CheckedCallableParameterDefault::new(owner, provider, value),
        diagnostics,
    ))
}

fn bind_struct_field_default(
    context: &CompilationBinderFacts<'_>,
    owner: StructFieldSymbolId,
) -> BinderFactResult<DiagnosticResult<CheckedStructFieldDefault>> {
    let template = context.symbol_fact(
        SymbolFactRequest::<StructFieldDefaultTemplateFact>::new(owner),
    )?;

    let (default, diagnostics) = checked_runtime_default(
        context,
        owner.into(),
        *template.value(),
        template.diagnostics(),
    )?
    .into_parts();

    let provider = bray_symbols::StructFieldDefaultProviderSymbolId::try_from_any(default.provider)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let value = if default.is_recovered {
        StructFieldDefaultValue::Error(ErrorStructFieldDefault)
    } else {
        StructFieldDefaultValue::Valid(StructFieldDefaultSurface::new(
            default.generic_context,
            default.result,
            default.behavior,
            default.template,
        ))
    };

    Ok(DiagnosticResult::new(
        CheckedStructFieldDefault::new(owner, provider, value),
        diagnostics,
    ))
}

fn bind_union_payload_field_default(
    context: &CompilationBinderFacts<'_>,
    owner: UnionPayloadFieldSymbolId,
) -> BinderFactResult<DiagnosticResult<CheckedUnionPayloadDefault>> {
    let template = context
        .symbol_fact(SymbolFactRequest::<UnionPayloadFieldDefaultTemplateFact>::new(owner))?;

    let (default, diagnostics) = checked_runtime_default(
        context,
        owner.into(),
        *template.value(),
        template.diagnostics(),
    )?
    .into_parts();

    let provider =
        bray_symbols::UnionPayloadDefaultProviderSymbolId::try_from_any(default.provider)
            .ok_or(BinderFactError::DependencyUnavailable)?;

    let value = if default.is_recovered {
        UnionPayloadDefaultValue::Error(ErrorUnionPayloadDefault)
    } else {
        UnionPayloadDefaultValue::Valid(UnionPayloadDefaultSurface::new(
            default.generic_context,
            default.result,
            default.behavior,
            default.template,
        ))
    };

    Ok(DiagnosticResult::new(
        CheckedUnionPayloadDefault::new(owner, provider, value),
        diagnostics,
    ))
}

fn checked_runtime_default(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    template: UnevaluatedDefaultTemplate,
    template_diagnostics: &DiagnosticBag,
) -> BinderFactResult<DiagnosticResult<RuntimeDefaultSummary>> {
    match template {
        UnevaluatedDefaultTemplate::Absent => Err(BinderFactError::DependencyUnavailable),
        UnevaluatedDefaultTemplate::Present(expression) => {
            let provider = runtime_default_provider(context, owner)?;

            let key = context
                .compilation()
                .source_runtime_default_key(provider, expression.syntax())
                .map_err(super::super::binding::binder_error)?;

            let checked = checked_source_expression(context, key.clone())?;
            let generic_context = source_generic_context(context, owner)?;

            let body_behavior = context
                .compilation()
                .body_behavior_with_cancellation(key, context.cancellation())
                .map_err(super::super::binding::binder_error)?;

            let behavior = runtime_default_behavior(
                context,
                checked.result,
                checked.dependency_contract,
                body_behavior.result().value(),
            )?;

            let syntax_diagnostics = syntax_diagnostics(context, expression.syntax());

            let diagnostics = DiagnosticBag::merged_all([
                template_diagnostics,
                &checked.diagnostics,
                body_behavior.result().diagnostics(),
                &syntax_diagnostics,
            ]);

            Ok(DiagnosticResult::new(
                RuntimeDefaultSummary {
                    provider,
                    result: checked.result,
                    generic_context,
                    behavior,
                    template: RuntimeDefaultTemplateReference::Source(expression.syntax()),
                    is_recovered: checked.is_recovered || diagnostics.has_errors(),
                },
                diagnostics,
            ))
        }
        UnevaluatedDefaultTemplate::Resolved => {
            let provider = runtime_default_provider(context, owner)?;

            let address = context
                .imported_fact_address(provider)?
                .ok_or(BinderFactError::DependencyUnavailable)?;

            let imported = imported_declaration_template(
                context,
                address,
                CheckedTemplateKind::RuntimeDefault,
            )?;

            let template_fact = imported
                .value()
                .as_ref()
                .ok_or(BinderFactError::DependencyUnavailable)?;

            let checked = template_fact.template();
            let result = checked_template_result(checked)?;
            let generic_context = imported_generic_context(context, owner, checked)?;
            let behavior = imported_runtime_default_behavior(context, checked, result)?;
            let diagnostics = template_diagnostics.merged(imported.diagnostics());

            Ok(DiagnosticResult::new(
                RuntimeDefaultSummary {
                    provider,
                    result,
                    generic_context,
                    behavior,
                    template: RuntimeDefaultTemplateReference::Interface {
                        interface: address.interface(),
                        entity: template_fact.entity(),
                    },
                    is_recovered: diagnostics.has_errors(),
                },
                diagnostics,
            ))
        }
    }
}

impl crate::compilation::Compilation {
    pub(in crate::compilation) fn source_runtime_default_key(
        &self,
        provider: AnySymbolId,
        syntax: bray_declarations::SyntaxAnchor,
    ) -> Result<BoundUnitKey, crate::fact::FactQueryError> {
        let source = self
            .source(syntax.source_id())
            .ok_or(crate::fact::FactQueryError::InfrastructureFailure)?;

        // Bound unit keys own their Arc-backed provider identity independently of the symbol graph.
        let provider = self
            .symbol_graph()?
            .symbol_key(provider)
            .cloned()
            .ok_or(crate::fact::FactQueryError::InfrastructureFailure)?;

        BoundUnitKey::runtime_default(provider, BoundSourceAnchor::new(syntax, source.version()))
            .ok_or(crate::fact::FactQueryError::InfrastructureFailure)
    }
}

fn source_generic_context(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
) -> BinderFactResult<RuntimeDefaultGenericContext> {
    let parameters = visible_generic_parameters(context.symbols(), owner);
    let declaration = runtime_default_declaration(context, owner)?;

    generic_context(context, declaration, parameters)
}

fn imported_generic_context(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
    template: &CheckedTemplate,
) -> BinderFactResult<RuntimeDefaultGenericContext> {
    let imported = context
        .imported_symbols()?
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let parameters = template
        .inputs()
        .iter()
        .filter_map(|input| match input.kind() {
            bray_bound_tree::CheckedTemplateInputKind::GenericType(key)
            | bray_bound_tree::CheckedTemplateInputKind::GenericConstant(key) => {
                imported.symbol_by_external_key(key)
            }
            bray_bound_tree::CheckedTemplateInputKind::Receiver
            | bray_bound_tree::CheckedTemplateInputKind::Parameter(_)
            | bray_bound_tree::CheckedTemplateInputKind::PostconditionResult => None,
        })
        .map(|symbol| {
            GenericParameterSymbolId::try_from_any(symbol)
                .ok_or(BinderFactError::DependencyUnavailable)
        })
        .collect::<BinderFactResult<Vec<_>>>()?;

    let declaration = runtime_default_declaration(context, owner)?;

    generic_context(context, declaration, parameters)
}

fn generic_context(
    context: &CompilationBinderFacts<'_>,
    declaration: AnySymbolId,
    parameters: Vec<GenericParameterSymbolId>,
) -> BinderFactResult<RuntimeDefaultGenericContext> {
    if parameters.is_empty() {
        return Ok(RuntimeDefaultGenericContext::NonGeneric);
    }

    let generic_owner =
        GenericOwnerId::try_new(declaration).ok_or(BinderFactError::DependencyUnavailable)?;

    let arguments = parameters
        .iter()
        .copied()
        .map(|parameter| {
            generic_parameter_argument(context.semantic_values(), parameter)
                .map_err(|_| BinderFactError::DependencyUnavailable)
        })
        .collect::<BinderFactResult<Vec<_>>>()?;

    let substitution =
        GenericSubstitutionData::try_new(generic_owner, parameters.iter().copied(), arguments)
            .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let substitution = context
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    RuntimeDefaultGenericContext::generic(parameters, substitution)
        .ok_or(BinderFactError::DependencyUnavailable)
}

fn runtime_default_declaration(
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
) -> BinderFactResult<AnySymbolId> {
    match owner {
        AnySymbolId::CallableParameter(owner) => {
            callable_parameter(context, owner).map(|record| record.owner().into_any())
        }
        AnySymbolId::StructField(owner) => {
            struct_field(context, owner).map(|record| AnySymbolId::from(record.structure()))
        }
        AnySymbolId::UnionPayloadField(owner) => {
            let field = union_payload_field(context, owner)?;
            let variant = union_variant(context, field.variant())?;

            Ok(variant.union().into())
        }
        _ => Err(BinderFactError::DependencyUnavailable),
    }
}

fn runtime_default_behavior(
    context: &CompilationBinderFacts<'_>,
    result: TypeId,
    dependency: bray_symbols::DependencyContractTemplateId,
    body: &bray_bound_tree::CheckedBodyBehavior,
) -> BinderFactResult<RuntimeDefaultBehavior> {
    let ownership = runtime_default_ownership(context, result)?;

    Ok(RuntimeDefaultBehavior::new(
        ownership,
        body.effects()
            .iter()
            .map(|effect| bray_symbols::RuntimeDefaultEffectRequirement::new(effect.declaration())),
        body.capabilities().iter().map(|capability| {
            bray_symbols::RuntimeDefaultCapabilityRequirement::new(capability.declaration())
        }),
        body.trusted_capabilities()
            .iter()
            .copied()
            .map(bray_symbols::RuntimeDefaultTrustedObligation::new),
        body.lifecycle_obligations().iter().copied(),
        dependency,
    ))
}

fn imported_runtime_default_behavior(
    context: &CompilationBinderFacts<'_>,
    template: &CheckedTemplate,
    result: TypeId,
) -> BinderFactResult<RuntimeDefaultBehavior> {
    let imported = context
        .imported_symbols()?
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let behavior = template.behavior();

    let effects = behavior
        .effects()
        .iter()
        .map(|requirement| imported.symbol_by_external_key(requirement.declaration()))
        .map(|symbol| {
            symbol
                .map(bray_symbols::RuntimeDefaultEffectRequirement::new)
                .ok_or(BinderFactError::DependencyUnavailable)
        })
        .collect::<BinderFactResult<Vec<_>>>()?;

    let capabilities = behavior
        .capabilities()
        .iter()
        .map(|requirement| imported.symbol_by_external_key(requirement.declaration()))
        .map(|symbol| {
            symbol
                .map(bray_symbols::RuntimeDefaultCapabilityRequirement::new)
                .ok_or(BinderFactError::DependencyUnavailable)
        })
        .collect::<BinderFactResult<Vec<_>>>()?;

    let trusted = behavior
        .trusted_obligations()
        .iter()
        .map(|requirement| imported.symbol_by_external_key(requirement.declaration()))
        .map(|symbol| {
            symbol
                .and_then(TrustedCapabilitySymbolId::try_from_any)
                .map(bray_symbols::RuntimeDefaultTrustedObligation::new)
                .ok_or(BinderFactError::DependencyUnavailable)
        })
        .collect::<BinderFactResult<Vec<_>>>()?;

    let ownership = runtime_default_ownership(context, result)?;

    Ok(RuntimeDefaultBehavior::new(
        ownership,
        effects,
        capabilities,
        trusted,
        behavior.lifecycle_obligations().iter().copied(),
        behavior.dependency_contract(),
    ))
}

fn runtime_default_ownership(
    context: &CompilationBinderFacts<'_>,
    result: TypeId,
) -> BinderFactResult<RuntimeDefaultOwnership> {
    let result = context
        .semantic_values()
        .type_data(result)
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    Ok(match result.as_ref() {
        TypeData::Borrow { kind, .. } => RuntimeDefaultOwnership::Borrowed(*kind),
        _ => RuntimeDefaultOwnership::Owned,
    })
}

fn checked_template_result(template: &CheckedTemplate) -> BinderFactResult<TypeId> {
    usize::try_from(template.result().raw())
        .ok()
        .and_then(|index| template.nodes().get(index))
        .map(bray_bound_tree::CheckedTemplateNode::ty)
        .ok_or(BinderFactError::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_symbols::{
        BorrowKind, CallableParameterDefaultValue, DependencyRequirementKind,
        RuntimeDefaultOwnership, RuntimeDefaultProviderInput, RuntimeDefaultTemplateReference,
        StructFieldDefaultValue, SymbolOrigin, UnionPayloadDefaultValue,
    };

    use super::super::super::test_support::assert_parameter_dependency_contract;
    use crate::test_support::compilation;

    #[test]
    fn source_runtime_defaults_publish_typed_cached_surfaces() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func configure(first: i32, second: i32 = first)\n",
            "{\n",
            "}\n",
            "struct Settings\n",
            "{\n",
            "    count: i32 = 1;\n",
            "}\n",
            "union Maybe\n",
            "{\n",
            "    Some(value: i32 = 1);\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let parameters = symbols
            .callable_parameters()
            .iter()
            .filter(|parameter| parameter.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [first, second] = parameters.as_slice() else {
            panic!("test callable must have two parameters: {parameters:?}");
        };

        let parameter_default = compilation
            .callable_parameter_default(second.id())
            .unwrap_or_else(|error| panic!("parameter default must publish: {error:?}"));

        let repeated_parameter_default = compilation
            .callable_parameter_default(second.id())
            .unwrap_or_else(|error| panic!("parameter default must be reusable: {error:?}"));

        assert!(Arc::ptr_eq(&parameter_default, &repeated_parameter_default));

        assert!(
            parameter_default.diagnostics().is_empty(),
            "parameter default diagnostics: {:?}",
            parameter_default.diagnostics()
        );

        let CallableParameterDefaultValue::Valid(parameter_surface) =
            parameter_default.value().value()
        else {
            panic!("parameter default must be valid");
        };

        assert_eq!(
            parameter_surface.inputs(),
            [RuntimeDefaultProviderInput::EarlierParameter(first.id())]
        );

        assert_parameter_dependency_contract(
            &compilation,
            parameter_surface.behavior().dependency_contract(),
            0,
            &[
                DependencyRequirementKind::StorageAlive,
                DependencyRequirementKind::StorageInitialized,
            ],
        );

        assert!(matches!(
            parameter_surface.template_reference(),
            RuntimeDefaultTemplateReference::Source(_)
        ));

        let field = symbols
            .struct_fields()
            .iter()
            .find(|field| field.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("source struct field must exist"));

        let field_default = compilation
            .struct_field_default(field.id())
            .unwrap_or_else(|error| panic!("struct field default must publish: {error:?}"));

        assert!(field_default.diagnostics().is_empty());

        let StructFieldDefaultValue::Valid(field_surface) = field_default.value().value() else {
            panic!("struct field default must be valid");
        };

        assert!(matches!(
            field_surface.template_reference(),
            RuntimeDefaultTemplateReference::Source(_)
        ));

        let payload = symbols
            .union_payload_fields()
            .iter()
            .find(|field| field.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("source union payload field must exist"));

        let payload_default = compilation
            .union_payload_field_default(payload.id())
            .unwrap_or_else(|error| panic!("union payload default must publish: {error:?}"));

        assert!(payload_default.diagnostics().is_empty());

        let UnionPayloadDefaultValue::Valid(payload_surface) = payload_default.value().value()
        else {
            panic!("union payload default must be valid");
        };

        assert!(matches!(
            payload_surface.template_reference(),
            RuntimeDefaultTemplateReference::Source(_)
        ));

        assert_eq!(parameter_surface.result(), field_surface.result());
        assert_eq!(field_surface.result(), payload_surface.result());
    }

    #[test]
    fn generic_runtime_defaults_retain_their_open_substitution() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func choose<T>(first: T, second: T = first)\n",
            "{\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let parameters = symbols
            .callable_parameters()
            .iter()
            .filter(|parameter| parameter.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [_, second] = parameters.as_slice() else {
            panic!("test callable must have two parameters: {parameters:?}");
        };

        let default = compilation
            .callable_parameter_default(second.id())
            .unwrap_or_else(|error| panic!("generic parameter default must publish: {error:?}"));

        let CallableParameterDefaultValue::Valid(surface) = default.value().value() else {
            panic!("generic parameter default must be valid");
        };

        assert_eq!(surface.generic_context().parameters().len(), 1);
        assert!(surface.generic_context().substitution().is_some());
    }

    #[test]
    fn borrowed_runtime_defaults_retain_result_ownership() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func choose(first: &i32, second: &i32 = first)\n",
            "{\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let parameter = symbols
            .callable_parameters()
            .iter()
            .filter(|parameter| parameter.origin() == SymbolOrigin::Source)
            .nth(1)
            .unwrap_or_else(|| panic!("defaulted source parameter must exist"));

        let default = compilation
            .callable_parameter_default(parameter.id())
            .unwrap_or_else(|error| panic!("borrowed parameter default must publish: {error:?}"));

        let CallableParameterDefaultValue::Valid(surface) = default.value().value() else {
            panic!("borrowed parameter default must be valid");
        };

        assert_eq!(
            surface.behavior().ownership(),
            RuntimeDefaultOwnership::Borrowed(BorrowKind::Shared)
        );
    }

    #[test]
    fn expression_created_borrow_defaults_require_the_active_borrow() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func choose(first: i32, second: &i32 = &first)\n",
            "{\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let parameter = symbols
            .callable_parameters()
            .iter()
            .filter(|parameter| parameter.origin() == SymbolOrigin::Source)
            .nth(1)
            .unwrap_or_else(|| panic!("defaulted source parameter must exist"));

        let default = compilation
            .callable_parameter_default(parameter.id())
            .unwrap_or_else(|error| panic!("borrowed parameter default must publish: {error:?}"));

        let CallableParameterDefaultValue::Valid(surface) = default.value().value() else {
            panic!("borrowed parameter default must be valid");
        };

        assert_parameter_dependency_contract(
            &compilation,
            surface.behavior().dependency_contract(),
            0,
            &[
                DependencyRequirementKind::StorageAlive,
                DependencyRequirementKind::StorageInitialized,
                DependencyRequirementKind::BorrowCapabilityActive(BorrowKind::Shared),
            ],
        );
    }

    #[test]
    fn recovered_runtime_defaults_publish_error_values_with_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func broken(value: i32 = @value)\n",
            "{\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let parameter = symbols
            .callable_parameters()
            .iter()
            .find(|parameter| parameter.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("source callable parameter must exist"));

        let default = compilation
            .callable_parameter_default(parameter.id())
            .unwrap_or_else(|error| panic!("recovered default must publish: {error:?}"));

        assert!(default.diagnostics().has_errors());

        assert!(matches!(
            default.value().value(),
            CallableParameterDefaultValue::Error(_)
        ));
    }
}
