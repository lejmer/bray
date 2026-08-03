// rust-style: allow(module-too-large, reason = "the recursive interface value tables share one canonicalization state")

use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolFactProvider;
use bray_bound_tree::{
    BoundReferenceTarget, BoundUnitKey, CheckedTemplateConstantUsage, CheckedTemplateKind,
    CheckedTemplateNodeId,
};
use bray_checker::{
    CheckedConstantTerms, CheckerUnitView, ConstantChecker, ConstantEvaluationInput,
    ConstantEvaluationLimits, DefaultConstantChecker, resolve_type_expression_template,
};
use bray_diagnostics::DiagnosticBag;
use bray_package_interface::{
    DependencyInterfaceId, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceCallableParameterDefault, InterfaceCallablePhaseBehavior, InterfaceCallableReceiver,
    InterfaceCallableSignature, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
    InterfaceCheckedTemplateExecution, InterfaceCheckedTemplateId,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCoherenceRecord, InterfaceConstantProjection,
    InterfaceConstantTerm, InterfaceConstantTermId, InterfaceConstantValue,
    InterfaceConstantValueId, InterfaceConstantValueKind, InterfaceConstraint,
    InterfaceDeclarationTemplate, InterfaceDeclaredType, InterfaceDependencyContract,
    InterfaceDependencyContractId, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceGenericBinding,
    InterfaceGenericDeclaration, InterfaceGenericSubstitution, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstance, InterfaceImplementationInstanceId,
    InterfaceImplementationRecord, InterfacePredicateDefinition, InterfacePredicateDefinitionState,
    InterfacePredicateSummary, InterfaceSemanticFacts, InterfaceSupportEntity,
    InterfaceSymbolReference, InterfaceTraitApplication, InterfaceTraitApplicationId,
    InterfaceTrustedCapabilityRequirement, InterfaceType, InterfaceTypeId,
    InterfaceTypeRepresentation, InterfaceUnionTag, PackageInterfaceSurface,
};
use bray_symbols::{
    AnySymbolId, CallableContractTemplate, CallableContractTemplateFact, CallableContractsFact,
    CallableInstanceId, CallableParameterDefaultTemplateFact, CallableParameterDefaultValue,
    CallablePhaseBehavior, CallableSignatureFact, CallableSymbolId, CheckedConstraintKind,
    ConstantField, ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantValueId,
    ConstantValueKind, CurrentRunCancellation, DependencyGuard, DependencyProjection,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    ExternalSymbolKey, GenericArgument, GenericConstraintsFact, GenericDeclarationTemplateFact,
    GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData, GenericSubstitutionId,
    ImplementationCoherenceFact, ImplementationInstanceId, ImplementationSymbolId,
    InterfaceSupportEntityId, NamedTypeSymbolId, PredicateDefinitionFact, PredicateDefinitionState,
    RuntimeDefaultGenericContext, RuntimeDefaultPresence, RuntimeDefaultProviderInput,
    RuntimeDefaultTemplateReference, SemanticValueStore, StructFieldDefaultValue,
    SymbolFactRequest, SymbolKeyData, SymbolKind, TraitApplicationId,
    TraitPredicateFulfillmentDefinitionFact, TraitPredicateMemberDefinitionFact, TypeData,
    TypeExpressionTemplate, TypeId, UnionPayloadDefaultValue,
};

use super::PackageInterfaceExportError;
use super::template::{SourceTemplateInput, export_checked_source_template};
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBinderFacts;
use crate::compilation::checker::{CompilationCheckerContext, checker_result};
use crate::compilation::unit::semantic_unit_context_for;

pub(super) fn build_semantic_facts(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    surface: &PackageInterfaceSurface,
    selected: &BTreeSet<AnySymbolId>,
    keys: &BTreeMap<AnySymbolId, ExternalSymbolKey>,
) -> Result<InterfaceSemanticFacts, PackageInterfaceExportError> {
    let values = compilation
        .semantic_value_store()
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

    let binder = compilation
        .binder_facts(&compilation.state.cancellation)
        .map_err(|_| PackageInterfaceExportError::InvalidCompilation)?;

    let mut export = SemanticExporter::new(graph, surface, keys, values);
    let mut declarations = ExportedDeclarationFacts::default();

    for symbol in selected.iter().copied() {
        export_callable_facts(
            compilation,
            graph,
            &binder,
            symbol,
            &mut export,
            &mut declarations,
        )?;

        export_generic_facts(compilation, &binder, symbol, &mut export, &mut declarations)?;
        export_predicate_fact(&binder, symbol, &export, &mut declarations)?;

        export_default_facts(
            compilation,
            graph,
            &binder,
            symbol,
            &mut export,
            &mut declarations,
        )?;

        export_type_facts(compilation, symbol, &mut export, &mut declarations)?;
    }

    let (implementations, coherence) = implementation_facts(&mut export, &binder, selected)?;

    declarations.generic_declarations.sort_unstable();
    declarations.declaration_templates.sort_unstable();

    Ok(InterfaceSemanticFacts::new()
        .with_applications(
            export.substitutions,
            export.trait_applications,
            export.callable_instances,
            export.implementation_instances,
        )
        .with_values(
            export.dependency_contracts,
            export.types,
            export.constant_values,
            export.constant_terms,
        )
        .with_contracts(declarations.constraints, declarations.callable_contracts)
        .with_declarations(
            declarations.signatures,
            declarations.generic_declarations,
            declarations.parameter_defaults,
            declarations.predicate_definitions,
        )
        .with_declared_types(declarations.declared_types)
        .with_type_representations(declarations.type_representations)
        .with_templates(
            declarations.checked_templates,
            declarations.declaration_templates,
            declarations.support_entities,
        )
        .with_implementations(implementations, coherence))
}

#[derive(Default)]
struct ExportedDeclarationFacts {
    signatures: Vec<InterfaceCallableSignature>,
    generic_declarations: Vec<InterfaceGenericDeclaration>,
    parameter_defaults: Vec<InterfaceCallableParameterDefault>,
    constraints: Vec<InterfaceConstraint>,
    callable_contracts: Vec<InterfaceCallableContract>,
    predicate_definitions: Vec<InterfacePredicateDefinition>,
    declared_types: Vec<InterfaceDeclaredType>,
    type_representations: Vec<InterfaceTypeRepresentation>,
    checked_templates: Vec<InterfaceCheckedTemplate>,
    declaration_templates: Vec<InterfaceDeclarationTemplate>,
    support_entities: Vec<InterfaceSupportEntity>,
}

fn export_callable_facts(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    binder: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    facts: &mut ExportedDeclarationFacts,
) -> Result<(), PackageInterfaceExportError> {
    let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
        return Ok(());
    };

    let signature = binder
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
        .map_err(|_| incomplete(symbol))?;

    if signature.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    facts
        .signatures
        .push(export.callable_signature(symbol, signature.value())?);

    let contracts = binder
        .symbol_fact(SymbolFactRequest::<CallableContractsFact>::new(callable))
        .map_err(|_| incomplete(symbol))?;

    if contracts.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    facts
        .callable_contracts
        .push(export.callable_contract(symbol, contracts.value())?);

    let template = binder
        .symbol_fact(SymbolFactRequest::<CallableContractTemplateFact>::new(
            callable,
        ))
        .map_err(|_| incomplete(symbol))?;

    if template.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let CallableContractTemplate::Source(template) = template.value() else {
        return Ok(());
    };

    for expression in template.expressions() {
        let clause = contracts
            .value()
            .invocation_preconditions()
            .iter()
            .chain(contracts.value().static_constraints())
            .chain(contracts.value().normal_completion_postconditions())
            .find(|clause| clause.ordinal() == expression.ordinal())
            .ok_or_else(|| incomplete(symbol))?;

        let source = compilation
            .bound_source(expression.unit_syntax())
            .map_err(|_| incomplete(symbol))?;

        // The unit key must own its Arc-backed symbol identity beyond this graph borrow.
        let owner = graph
            .symbol_key(symbol)
            .cloned()
            .ok_or_else(|| incomplete(symbol))?;

        let key = BoundUnitKey::contract_clause(owner, source).ok_or_else(|| incomplete(symbol))?;

        let inputs = callable_template_inputs(
            compilation,
            export,
            symbol,
            signature.value(),
            generic_parameters(binder, symbol)?,
        )?;

        let checked = export_checked_source_template(
            compilation,
            export,
            key,
            CheckedTemplateKind::CallableContract,
            expression.expression().syntax(),
            inputs,
            clause.predicate().dependency_contract(),
        )?;

        push_declaration_template(
            export,
            symbol,
            CheckedTemplateKind::CallableContract,
            expression.ordinal(),
            checked,
            &mut facts.checked_templates,
            &mut facts.declaration_templates,
            &mut facts.support_entities,
        )?;
    }

    Ok(())
}

fn export_generic_facts(
    compilation: &Compilation,
    binder: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    facts: &mut ExportedDeclarationFacts,
) -> Result<(), PackageInterfaceExportError> {
    let Some(owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(());
    };

    let generic = binder
        .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
            owner,
        ))
        .map_err(|_| incomplete(symbol))?;

    if generic.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    facts
        .generic_declarations
        .push(export.generic_declaration(symbol, generic.value())?);

    let checked = binder
        .symbol_fact(SymbolFactRequest::<GenericConstraintsFact>::new(owner))
        .map_err(|_| incomplete(symbol))?;

    if checked.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    facts
        .constraints
        .extend(export.generic_constraints(symbol, checked.value())?);

    for constraint in checked.value().constraints() {
        let CheckedConstraintKind::Predicate(predicate) = constraint.kind() else {
            continue;
        };

        let source = generic
            .value()
            .constraints()
            .iter()
            .find(|source| source.ordinal() == constraint.ordinal())
            .ok_or_else(|| incomplete(symbol))?;

        let unit = source.unit_syntax().ok_or_else(|| incomplete(symbol))?;
        let expression = source.expression().ok_or_else(|| incomplete(symbol))?;

        let checked_expression =
            checked_constraint_expression(compilation, generic.value(), unit, expression)?;

        let checked_id = InterfaceCheckedTemplateId::new(index(facts.checked_templates.len())?);
        let entity = InterfaceSupportEntityId::new(index(facts.support_entities.len())?);

        facts
            .checked_templates
            .push(export.checked_constant_template(
                CheckedTemplateKind::GenericConstraint,
                checked_expression,
                predicate.dependency_contract(),
            )?);

        facts
            .support_entities
            .push(InterfaceSupportEntity::CheckedTemplate(checked_id));

        facts
            .declaration_templates
            .push(InterfaceDeclarationTemplate::new(
                export.symbol_reference(symbol)?,
                CheckedTemplateKind::GenericConstraint,
                constraint.ordinal(),
                entity,
            ));
    }

    Ok(())
}

fn export_predicate_fact(
    binder: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
    export: &SemanticExporter<'_>,
    facts: &mut ExportedDeclarationFacts,
) -> Result<(), PackageInterfaceExportError> {
    let Some(state) = predicate_definition(binder, symbol)? else {
        return Ok(());
    };

    facts
        .predicate_definitions
        .push(InterfacePredicateDefinition::new(
            export.symbol_reference(symbol)?,
            state,
        ));

    Ok(())
}

fn export_default_facts(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    binder: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    facts: &mut ExportedDeclarationFacts,
) -> Result<(), PackageInterfaceExportError> {
    match symbol {
        AnySymbolId::CallableParameter(parameter) => {
            let default = binder
                .symbol_fact(
                    SymbolFactRequest::<CallableParameterDefaultTemplateFact>::new(parameter),
                )
                .map_err(|_| incomplete(symbol))?;

            if default.diagnostics().has_errors() {
                return Err(incomplete(symbol));
            }

            facts
                .parameter_defaults
                .push(InterfaceCallableParameterDefault::new(
                    export.symbol_reference(symbol)?,
                    default.value().is_present(),
                ));

            if default.value().is_present() {
                let checked = compilation
                    .callable_parameter_default(parameter)
                    .map_err(|_| incomplete(symbol))?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let CallableParameterDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default_surface(
                    compilation,
                    graph,
                    export,
                    checked.value().provider().into(),
                    surface,
                    facts,
                )?;
            }
        }
        AnySymbolId::StructField(field) => {
            let Some(field) = graph.struct_field(field) else {
                return Err(incomplete(symbol));
            };

            if field.default_presence() != RuntimeDefaultPresence::Absent {
                let checked = compilation
                    .struct_field_default(field.id())
                    .map_err(|_| incomplete(symbol))?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let StructFieldDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default_surface(
                    compilation,
                    graph,
                    export,
                    checked.value().provider().into(),
                    surface,
                    facts,
                )?;
            }
        }
        AnySymbolId::UnionPayloadField(field) => {
            let Some(field) = graph.union_payload_field(field) else {
                return Err(incomplete(symbol));
            };

            if field.default_presence() != RuntimeDefaultPresence::Absent {
                let checked = compilation
                    .union_payload_field_default(field.id())
                    .map_err(|_| incomplete(symbol))?;

                if checked.diagnostics().has_errors() {
                    return Err(incomplete(symbol));
                }

                let UnionPayloadDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(incomplete(symbol));
                };

                export_runtime_default_surface(
                    compilation,
                    graph,
                    export,
                    checked.value().provider().into(),
                    surface,
                    facts,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn export_runtime_default_surface(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    provider: AnySymbolId,
    surface: &impl RuntimeDefaultSurface,
    facts: &mut ExportedDeclarationFacts,
) -> Result<(), PackageInterfaceExportError> {
    export_runtime_default(
        compilation,
        graph,
        export,
        provider,
        surface.inputs(),
        surface.generic_context(),
        surface.template_reference(),
        surface.dependency_contract(),
        &mut facts.checked_templates,
        &mut facts.declaration_templates,
        &mut facts.support_entities,
    )
}

trait RuntimeDefaultSurface {
    fn inputs(&self) -> &[RuntimeDefaultProviderInput];
    fn generic_context(&self) -> &RuntimeDefaultGenericContext;
    fn template_reference(&self) -> &RuntimeDefaultTemplateReference;
    fn dependency_contract(&self) -> bray_symbols::DependencyContractTemplateId;
}

macro_rules! impl_runtime_default_surface {
    ($($surface:ty),+ $(,)?) => {
        $(
            impl RuntimeDefaultSurface for $surface {
                fn inputs(&self) -> &[RuntimeDefaultProviderInput] {
                    self.inputs()
                }

                fn generic_context(&self) -> &RuntimeDefaultGenericContext {
                    self.generic_context()
                }

                fn template_reference(&self) -> &RuntimeDefaultTemplateReference {
                    self.template_reference()
                }

                fn dependency_contract(&self) -> bray_symbols::DependencyContractTemplateId {
                    self.behavior().dependency_contract()
                }
            }
        )+
    };
}

impl_runtime_default_surface!(
    bray_symbols::CallableParameterDefaultSurface,
    bray_symbols::StructFieldDefaultSurface,
    bray_symbols::UnionPayloadDefaultSurface,
);

fn export_type_facts(
    compilation: &Compilation,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
    facts: &mut ExportedDeclarationFacts,
) -> Result<(), PackageInterfaceExportError> {
    if let Some(template) = compilation
        .symbol_type_template(symbol)
        .map_err(|_| incomplete(symbol))?
    {
        if template.diagnostics().has_errors() {
            return Err(incomplete(symbol));
        }

        let ty = export.resolve_type_template(symbol, template.value())?;

        facts.declared_types.push(InterfaceDeclaredType::new(
            export.symbol_reference(symbol)?,
            export.type_id(ty)?,
        ));
    }

    let Some(named_type) = NamedTypeSymbolId::try_from_any(symbol) else {
        return Ok(());
    };

    let representation = compilation
        .declared_type_representation(named_type)
        .map_err(|_| incomplete(symbol))?;

    if representation.diagnostics().has_errors() || representation.value().is_recovered() {
        return Err(incomplete(symbol));
    }

    facts
        .type_representations
        .push(export.type_representation(symbol, representation.value())?);

    Ok(())
}

fn export_runtime_default(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    provider: AnySymbolId,
    contextual_inputs: &[RuntimeDefaultProviderInput],
    generic_context: &RuntimeDefaultGenericContext,
    template: &RuntimeDefaultTemplateReference,
    dependency: bray_symbols::DependencyContractTemplateId,
    checked_templates: &mut Vec<InterfaceCheckedTemplate>,
    declaration_templates: &mut Vec<InterfaceDeclarationTemplate>,
    support_entities: &mut Vec<InterfaceSupportEntity>,
) -> Result<(), PackageInterfaceExportError> {
    let RuntimeDefaultTemplateReference::Source(expression) = template else {
        return Err(incomplete(provider));
    };

    let key = compilation
        .source_runtime_default_key(provider, *expression)
        .map_err(|_| incomplete(provider))?;

    let mut inputs = runtime_default_inputs(compilation, graph, export, contextual_inputs)?;

    inputs.extend(generic_template_inputs(
        compilation,
        export,
        generic_context.parameters().iter().copied(),
    )?);

    let checked = export_checked_source_template(
        compilation,
        export,
        key,
        CheckedTemplateKind::RuntimeDefault,
        *expression,
        inputs,
        dependency,
    )?;

    push_declaration_template(
        export,
        provider,
        CheckedTemplateKind::RuntimeDefault,
        bray_symbols::SymbolOrdinal::new(0),
        checked,
        checked_templates,
        declaration_templates,
        support_entities,
    )
}

fn runtime_default_inputs(
    compilation: &Compilation,
    graph: &bray_symbols::SymbolGraph,
    export: &mut SemanticExporter<'_>,
    inputs: &[RuntimeDefaultProviderInput],
) -> Result<Vec<SourceTemplateInput>, PackageInterfaceExportError> {
    inputs
        .iter()
        .copied()
        .map(|input| {
            let (kind, symbol) = match input {
                RuntimeDefaultProviderInput::Receiver(receiver) => (
                    InterfaceCheckedTemplateInputKind::Receiver,
                    AnySymbolId::ReceiverParameter(receiver),
                ),
                RuntimeDefaultProviderInput::EarlierParameter(parameter) => {
                    let ordinal = graph
                        .callable_parameters()
                        .iter()
                        .find(|candidate| candidate.id() == parameter)
                        .map(|parameter| parameter.ordinal())
                        .ok_or_else(|| incomplete(parameter.into()))?;

                    (
                        InterfaceCheckedTemplateInputKind::Parameter(
                            bray_symbols::SymbolOrdinal::new(ordinal),
                        ),
                        AnySymbolId::CallableParameter(parameter),
                    )
                }
            };

            Ok(SourceTemplateInput::new(
                kind,
                Some(BoundReferenceTarget::Surface(symbol)),
                symbol_type(compilation, export, symbol)?,
            ))
        })
        .collect()
}

fn callable_template_inputs(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    callable: AnySymbolId,
    signature: &bray_symbols::CallableSignatureTemplate,
    generic_parameters: Vec<GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, PackageInterfaceExportError> {
    let mut inputs = Vec::new();

    if let Some(receiver) = signature.receiver() {
        inputs.push(SourceTemplateInput::new(
            InterfaceCheckedTemplateInputKind::Receiver,
            Some(BoundReferenceTarget::Surface(
                AnySymbolId::ReceiverParameter(receiver.parameter()),
            )),
            receiver.ty(),
        ));
    }

    let parameter_types = signature
        .parameter_type_templates(export.values)
        .map_err(|_| incomplete(callable))?;

    for (index, (parameter, ty)) in signature
        .parameters()
        .iter()
        .copied()
        .zip(parameter_types.iter())
        .enumerate()
    {
        inputs.push(SourceTemplateInput::new(
            InterfaceCheckedTemplateInputKind::Parameter(bray_symbols::SymbolOrdinal::new(
                u32::try_from(index).map_err(|_| incomplete(parameter.into()))?,
            )),
            Some(BoundReferenceTarget::Surface(
                AnySymbolId::CallableParameter(parameter),
            )),
            export.resolve_type_template(parameter.into(), ty)?,
        ));
    }

    inputs.extend(generic_template_inputs(
        compilation,
        export,
        generic_parameters,
    )?);

    Ok(inputs)
}

fn generic_template_inputs(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
) -> Result<Vec<SourceTemplateInput>, PackageInterfaceExportError> {
    parameters
        .into_iter()
        .map(|parameter| match parameter {
            GenericParameterSymbolId::Type(parameter) => {
                let symbol = AnySymbolId::GenericTypeParameter(parameter);

                let ty = export
                    .values
                    .intern_type(TypeData::TypeParameter(parameter))
                    .map_err(|_| incomplete(symbol))?;

                Ok(SourceTemplateInput::new(
                    InterfaceCheckedTemplateInputKind::GenericType(
                        export.symbol_reference(symbol)?,
                    ),
                    None,
                    ty,
                ))
            }
            GenericParameterSymbolId::Const(parameter) => {
                let symbol = AnySymbolId::GenericConstParameter(parameter);

                Ok(SourceTemplateInput::new(
                    InterfaceCheckedTemplateInputKind::GenericConstant(
                        export.symbol_reference(symbol)?,
                    ),
                    Some(BoundReferenceTarget::Surface(symbol)),
                    symbol_type(compilation, export, symbol)?,
                ))
            }
        })
        .collect()
}

fn symbol_type(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    symbol: AnySymbolId,
) -> Result<TypeId, PackageInterfaceExportError> {
    let ty = compilation
        .symbol_type_template(symbol)
        .map_err(|_| incomplete(symbol))?
        .ok_or_else(|| incomplete(symbol))?;

    if ty.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    export.resolve_type_template(symbol, ty.value())
}

fn generic_parameters(
    binder: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<Vec<GenericParameterSymbolId>, PackageInterfaceExportError> {
    let Some(owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(Vec::new());
    };

    let generic = binder
        .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
            owner,
        ))
        .map_err(|_| incomplete(symbol))?;

    if generic.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    Ok(generic.value().parameters().to_vec())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the helper commits one checked template and its correlated interface records"
)]
fn push_declaration_template(
    export: &mut SemanticExporter<'_>,
    owner: AnySymbolId,
    kind: CheckedTemplateKind,
    ordinal: bray_symbols::SymbolOrdinal,
    checked: InterfaceCheckedTemplate,
    checked_templates: &mut Vec<InterfaceCheckedTemplate>,
    declaration_templates: &mut Vec<InterfaceDeclarationTemplate>,
    support_entities: &mut Vec<InterfaceSupportEntity>,
) -> Result<(), PackageInterfaceExportError> {
    let checked_id = InterfaceCheckedTemplateId::new(index(checked_templates.len())?);
    let entity = InterfaceSupportEntityId::new(index(support_entities.len())?);

    checked_templates.push(checked);
    support_entities.push(InterfaceSupportEntity::CheckedTemplate(checked_id));

    declaration_templates.push(InterfaceDeclarationTemplate::new(
        export.symbol_reference(owner)?,
        kind,
        ordinal,
        entity,
    ));

    Ok(())
}

fn implementation_facts(
    export: &mut SemanticExporter<'_>,
    binder: &CompilationBinderFacts<'_>,
    selected: &BTreeSet<AnySymbolId>,
) -> Result<
    (
        Vec<InterfaceImplementationRecord>,
        Vec<InterfaceCoherenceRecord>,
    ),
    PackageInterfaceExportError,
> {
    let mut implementations = Vec::new();

    let mut coherence = BTreeMap::<
        (InterfaceTypeId, InterfaceTraitApplicationId),
        Vec<InterfaceSymbolReference>,
    >::new();

    for symbol in selected.iter().copied() {
        let Some(implementation) = ImplementationSymbolId::try_from_any(symbol) else {
            continue;
        };

        let checked = binder
            .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
                implementation,
            ))
            .map_err(|_| incomplete(symbol))?;

        if checked.diagnostics().has_errors() {
            return Err(incomplete(symbol));
        }

        let subject = export.type_id(checked.value().subject())?;

        let trait_application = checked
            .value()
            .trait_application()
            .map(|application| export.trait_application_id(application))
            .transpose()?;

        let reference = export.symbol_reference(symbol)?;

        implementations.push(InterfaceImplementationRecord::new(
            reference.clone(),
            subject,
            trait_application,
        ));

        if let Some(application) = trait_application {
            coherence
                .entry((subject, application))
                .or_default()
                .push(reference);
        }
    }

    let coherence = coherence
        .into_iter()
        .map(|((subject, application), implementations)| {
            InterfaceCoherenceRecord::new(subject, application, implementations)
        })
        .collect();

    Ok((implementations, coherence))
}

#[derive(Clone, Copy)]
struct CheckedConstantExpression {
    term: ConstantTermId,
    ty: TypeId,
}

pub(super) struct SemanticExporter<'a> {
    graph: &'a bray_symbols::SymbolGraph,
    surface: &'a PackageInterfaceSurface,
    keys: &'a BTreeMap<AnySymbolId, ExternalSymbolKey>,
    values: &'a SemanticValueStore,
    type_ids: BTreeMap<TypeId, InterfaceTypeId>,
    substitution_ids: BTreeMap<GenericSubstitutionId, InterfaceGenericSubstitutionId>,
    trait_application_ids: BTreeMap<TraitApplicationId, InterfaceTraitApplicationId>,
    callable_instance_ids: BTreeMap<CallableInstanceId, InterfaceCallableInstanceId>,
    implementation_instance_ids:
        BTreeMap<ImplementationInstanceId, InterfaceImplementationInstanceId>,
    dependency_contract_ids:
        BTreeMap<bray_symbols::DependencyContractTemplateId, InterfaceDependencyContractId>,
    constant_term_ids: BTreeMap<ConstantTermId, InterfaceConstantTermId>,
    constant_value_ids: BTreeMap<ConstantValueId, InterfaceConstantValueId>,
    types: Vec<InterfaceType>,
    substitutions: Vec<InterfaceGenericSubstitution>,
    trait_applications: Vec<InterfaceTraitApplication>,
    callable_instances: Vec<InterfaceCallableInstance>,
    implementation_instances: Vec<InterfaceImplementationInstance>,
    dependency_contracts: Vec<InterfaceDependencyContract>,
    constant_terms: Vec<InterfaceConstantTerm>,
    constant_values: Vec<InterfaceConstantValue>,
}

impl<'a> SemanticExporter<'a> {
    fn new(
        graph: &'a bray_symbols::SymbolGraph,
        surface: &'a PackageInterfaceSurface,
        keys: &'a BTreeMap<AnySymbolId, ExternalSymbolKey>,
        values: &'a SemanticValueStore,
    ) -> Self {
        Self {
            graph,
            surface,
            keys,
            values,
            type_ids: BTreeMap::new(),
            substitution_ids: BTreeMap::new(),
            trait_application_ids: BTreeMap::new(),
            callable_instance_ids: BTreeMap::new(),
            implementation_instance_ids: BTreeMap::new(),
            dependency_contract_ids: BTreeMap::new(),
            constant_term_ids: BTreeMap::new(),
            constant_value_ids: BTreeMap::new(),
            types: Vec::new(),
            substitutions: Vec::new(),
            trait_applications: Vec::new(),
            callable_instances: Vec::new(),
            implementation_instances: Vec::new(),
            dependency_contracts: Vec::new(),
            constant_terms: Vec::new(),
            constant_values: Vec::new(),
        }
    }

    fn callable_signature(
        &mut self,
        owner: AnySymbolId,
        template: &bray_symbols::CallableSignatureTemplate,
    ) -> Result<InterfaceCallableSignature, PackageInterfaceExportError> {
        let callable_type = self.resolve_type_template(owner, template.callable_type())?;
        let result = self.resolve_type_template(owner, template.result())?;

        let receiver = template
            .receiver()
            .map(|receiver| {
                Ok(InterfaceCallableReceiver::new(
                    self.symbol_reference(receiver.parameter().into())?,
                    self.type_id(receiver.ty())?,
                    receiver.mode(),
                ))
            })
            .transpose()?;

        let parameters = template
            .parameters()
            .iter()
            .copied()
            .map(|parameter| self.symbol_reference(parameter.into()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceCallableSignature::new(
            self.symbol_reference(owner)?,
            self.type_id(callable_type)?,
            receiver,
            parameters,
            self.type_id(result)?,
        )
        .with_body(template.has_body()))
    }

    fn generic_declaration(
        &self,
        owner: AnySymbolId,
        template: &bray_symbols::GenericDeclarationTemplate,
    ) -> Result<InterfaceGenericDeclaration, PackageInterfaceExportError> {
        let parameters = template
            .parameters()
            .iter()
            .copied()
            .map(|parameter| self.symbol_reference(parameter.into_any()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceGenericDeclaration::new(
            self.symbol_reference(owner)?,
            parameters,
        ))
    }

    pub(super) fn resolve_type_template(
        &self,
        owner: AnySymbolId,
        template: &TypeExpressionTemplate,
    ) -> Result<TypeId, PackageInterfaceExportError> {
        resolve_type_expression_template(self.values, template, &CheckedConstantTerms::new())
            .map_err(|_| incomplete(owner))?
            .ok_or_else(|| incomplete(owner))
    }

    fn generic_constraints(
        &mut self,
        owner: AnySymbolId,
        constraints: &bray_symbols::GenericConstraintSet,
    ) -> Result<Vec<InterfaceConstraint>, PackageInterfaceExportError> {
        let owner = self.symbol_reference(owner)?;

        constraints
            .constraints()
            .iter()
            .copied()
            .map(|constraint| match constraint.kind() {
                CheckedConstraintKind::Predicate(predicate) => Ok(InterfaceConstraint::new(
                    owner.clone(),
                    constraint.ordinal(),
                    self.predicate_summary(predicate)?,
                )),
                CheckedConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => Ok(InterfaceConstraint::trait_satisfaction(
                    owner.clone(),
                    constraint.ordinal(),
                    self.type_id(subject)?,
                    self.trait_application_id(application)?,
                )),
            })
            .collect()
    }

    fn callable_contract(
        &mut self,
        owner: AnySymbolId,
        contract: &bray_symbols::CallableContractSet,
    ) -> Result<InterfaceCallableContract, PackageInterfaceExportError> {
        let clauses = contract
            .invocation_preconditions()
            .iter()
            .chain(contract.static_constraints())
            .chain(contract.normal_completion_postconditions())
            .copied()
            .map(|clause| {
                Ok(InterfaceCallableContractClause::new(
                    clause.ordinal(),
                    clause.kind(),
                    self.predicate_summary(clause.predicate())?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let invocation = self.phase_behavior(contract.invocation_behavior())?;

        let deferred = contract
            .deferred_execution_behavior()
            .map(|behavior| self.phase_behavior(behavior))
            .transpose()?;

        Ok(InterfaceCallableContract::new(
            self.symbol_reference(owner)?,
            clauses,
            invocation,
            deferred,
        ))
    }

    fn predicate_summary(
        &mut self,
        predicate: bray_symbols::PredicateSemanticSummary,
    ) -> Result<InterfacePredicateSummary, PackageInterfaceExportError> {
        Ok(InterfacePredicateSummary::new(
            self.dependency_contract_id(predicate.dependency_contract())?,
        ))
    }

    fn checked_constant_template(
        &mut self,
        kind: CheckedTemplateKind,
        expression: CheckedConstantExpression,
        dependency_contract: bray_symbols::DependencyContractTemplateId,
    ) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
        let result = CheckedTemplateNodeId::new(0);

        let node = InterfaceCheckedTemplateNode::new(
            InterfaceCheckedTemplateOperation::Constant {
                term: self.constant_term_id(expression.term)?,
                usage: CheckedTemplateConstantUsage::default(),
            },
            self.type_id(expression.ty)?,
        );

        let behavior = InterfaceCheckedTemplateBehavior::new(
            [],
            [],
            [],
            InterfaceCheckedTemplateExecution::new([], CurrentRunCancellation::NotEntered),
            [],
            self.dependency_contract_id(dependency_contract)?,
            [],
        );

        Ok(InterfaceCheckedTemplate::new(
            kind,
            [],
            [node],
            [],
            result,
            behavior,
        ))
    }

    pub(super) fn type_id(
        &mut self,
        id: TypeId,
    ) -> Result<InterfaceTypeId, PackageInterfaceExportError> {
        if let Some(id) = self.type_ids.get(&id) {
            return Ok(*id);
        }

        let data = self.values.type_data(id).map_err(|_| incomplete_type())?;

        let ty = match data.as_ref() {
            TypeData::Error => return Err(incomplete_type()),
            TypeData::Named {
                definition,
                substitution,
            } => InterfaceType::Named {
                definition: self.symbol_reference(definition.into_any())?,
                substitution: self.substitution_id(*substitution)?,
            },
            TypeData::TypeParameter(parameter) => {
                InterfaceType::TypeParameter(self.symbol_reference((*parameter).into())?)
            }
            TypeData::ContextualSelf(context) => {
                InterfaceType::ContextualSelf(self.symbol_reference(context.symbol())?)
            }
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => InterfaceType::TypeValuedMemberProjection {
                subject: self.type_id(*subject)?,
                application: self.trait_application_id(*application)?,
                member: self.symbol_reference((*member).into())?,
            },
            TypeData::Tuple(elements) => InterfaceType::Tuple(
                elements
                    .iter()
                    .copied()
                    .map(|element| self.type_id(element))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            TypeData::Array { element, length } => InterfaceType::Array {
                element: self.type_id(*element)?,
                length: self.constant_term_id(*length)?,
            },
            TypeData::Slice(element) => InterfaceType::Slice(self.type_id(*element)?),
            TypeData::Generator(element) => InterfaceType::Generator(self.type_id(*element)?),
            TypeData::Nullable(target) => InterfaceType::Nullable(self.type_id(*target)?),
            TypeData::Borrow { kind, target } => InterfaceType::Borrow {
                kind: *kind,
                target: self.type_id(*target)?,
            },
            TypeData::TraitView(application) => {
                InterfaceType::TraitView(self.trait_application_id(*application)?)
            }
            TypeData::OwnedIndirection { storage, target } => InterfaceType::OwnedIndirection {
                storage: self.type_id(*storage)?,
                target: self.type_id(*target)?,
            },
            TypeData::Callable(callable) => self.callable_type(callable)?,
        };

        let exported = InterfaceTypeId::new(index(self.types.len())?);
        self.types.push(ty);
        self.type_ids.insert(id, exported);

        Ok(exported)
    }

    fn callable_type(
        &mut self,
        callable: &bray_symbols::CallableTypeData,
    ) -> Result<InterfaceType, PackageInterfaceExportError> {
        let parameters = callable
            .parameters()
            .iter()
            .map(|parameter| {
                Ok(InterfaceCallableParameter::new(
                    parameter.name().as_str(),
                    parameter.position(),
                    parameter.mode(),
                    self.type_id(parameter.ty())?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let invocation_behavior = self.phase_behavior(callable.phase_behaviors().invocation())?;

        let deferred_execution_behavior = callable
            .phase_behaviors()
            .deferred_execution()
            .map(|behavior| self.phase_behavior(behavior))
            .transpose()?;

        Ok(InterfaceType::Callable {
            parameters: parameters.into(),
            result: self.type_id(callable.result())?,
            constness: callable.constness(),
            trust: callable.trust(),
            abi: callable.abi(),
            invocation_behavior,
            deferred_execution_behavior,
        })
    }

    fn phase_behavior(
        &mut self,
        behavior: &CallablePhaseBehavior,
    ) -> Result<InterfaceCallablePhaseBehavior, PackageInterfaceExportError> {
        let effects = behavior
            .effects()
            .iter()
            .map(|requirement| self.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?;

        let capabilities = behavior
            .capabilities()
            .iter()
            .map(|requirement| self.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?;

        let trusted_capabilities = behavior
            .trusted_capabilities()
            .iter()
            .map(|requirement| {
                Ok(InterfaceTrustedCapabilityRequirement::new(
                    requirement.ordinal(),
                    self.symbol_reference(requirement.capability().into())?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let execution_requirements = behavior
            .execution_requirements()
            .iter()
            .map(|requirement| self.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceCallablePhaseBehavior::new(
            effects,
            capabilities,
            trusted_capabilities,
            execution_requirements,
            behavior.lifecycle_obligations().iter().copied(),
            self.dependency_contract_id(behavior.dependency_contract())?,
            behavior.current_run_cancellation(),
        ))
    }

    pub(super) fn dependency_contract_id(
        &mut self,
        id: bray_symbols::DependencyContractTemplateId,
    ) -> Result<InterfaceDependencyContractId, PackageInterfaceExportError> {
        if let Some(id) = self.dependency_contract_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .dependency_contract_template_data(id)
            .map_err(|_| incomplete_type())?;

        let requirements = data
            .requirements()
            .iter()
            .map(|requirement| self.dependency_requirement(requirement))
            .collect::<Result<Vec<_>, _>>()?;

        let exported = InterfaceDependencyContractId::new(index(self.dependency_contracts.len())?);

        self.dependency_contracts
            .push(InterfaceDependencyContract::new(requirements));

        self.dependency_contract_ids.insert(id, exported);

        Ok(exported)
    }

    fn dependency_requirement(
        &mut self,
        requirement: &DependencyRequirement,
    ) -> Result<InterfaceDependencyRequirement, PackageInterfaceExportError> {
        match requirement {
            DependencyRequirement::Direct { subject, kind } => {
                Ok(InterfaceDependencyRequirement::new(
                    self.dependency_subject(subject)?,
                    dependency_requirement_kind(*kind),
                ))
            }
            DependencyRequirement::Guarded(guarded) => {
                let guard = self.dependency_guard(guarded.guard())?;

                let requirements = guarded
                    .requirements()
                    .iter()
                    .map(|requirement| self.dependency_requirement(requirement))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(InterfaceDependencyRequirement::guarded(guard, requirements))
            }
        }
    }

    fn dependency_guard(
        &mut self,
        guard: &DependencyGuard,
    ) -> Result<InterfaceDependencyGuard, PackageInterfaceExportError> {
        match guard {
            DependencyGuard::NullablePresent(subject) => Ok(
                InterfaceDependencyGuard::NullablePresent(self.dependency_subject(subject)?),
            ),
            DependencyGuard::ActiveUnionVariant { subject, variant } => {
                Ok(InterfaceDependencyGuard::ActiveUnionVariant {
                    subject: self.dependency_subject(subject)?,
                    variant: self.symbol_reference((*variant).into())?,
                })
            }
        }
    }

    fn dependency_subject(
        &mut self,
        subject: &DependencySubject,
    ) -> Result<InterfaceDependencySubject, PackageInterfaceExportError> {
        let root = match subject.subject_root() {
            DependencySubjectRoot::Receiver => InterfaceDependencySubjectRoot::Receiver,
            DependencySubjectRoot::Parameter(ordinal) => {
                InterfaceDependencySubjectRoot::Parameter(ordinal)
            }
            DependencySubjectRoot::Result => InterfaceDependencySubjectRoot::Result,
            DependencySubjectRoot::ScopedCapability(ordinal) => {
                InterfaceDependencySubjectRoot::ScopedCapability(ordinal)
            }
            DependencySubjectRoot::ImplementationWitness(instance) => {
                InterfaceDependencySubjectRoot::ImplementationWitness(
                    self.implementation_instance_id(instance)?,
                )
            }
        };

        let projections = subject
            .projections()
            .iter()
            .map(|projection| match projection {
                DependencyProjection::ProductField(field) => self
                    .symbol_reference((*field).into())
                    .map(InterfaceDependencyProjection::ProductField),
                DependencyProjection::TupleElement(ordinal) => {
                    Ok(InterfaceDependencyProjection::TupleElement(*ordinal))
                }
                DependencyProjection::Element(term) => self
                    .constant_term_id(*term)
                    .map(InterfaceDependencyProjection::Element),
                DependencyProjection::NullableValue => {
                    Ok(InterfaceDependencyProjection::NullableValue)
                }
                DependencyProjection::UnionPayloadField(field) => self
                    .symbol_reference((*field).into())
                    .map(InterfaceDependencyProjection::UnionPayloadField),
                DependencyProjection::OwnedTarget => Ok(InterfaceDependencyProjection::OwnedTarget),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceDependencySubject::new(root, projections))
    }

    pub(super) fn substitution_id(
        &mut self,
        id: GenericSubstitutionId,
    ) -> Result<InterfaceGenericSubstitutionId, PackageInterfaceExportError> {
        if let Some(id) = self.substitution_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .generic_substitution_data(id)
            .map_err(|_| incomplete_type())?;

        let bindings = data
            .bindings()
            .iter()
            .map(|binding| {
                let argument = match binding.argument() {
                    GenericArgument::Type(ty) => InterfaceGenericArgument::Type(self.type_id(ty)?),
                    GenericArgument::Constant(term) => {
                        InterfaceGenericArgument::Constant(self.constant_term_id(term)?)
                    }
                };

                Ok(InterfaceGenericBinding::new(
                    self.symbol_reference(binding.parameter().into_any())?,
                    argument,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let exported = InterfaceGenericSubstitutionId::new(index(self.substitutions.len())?);

        self.substitutions.push(InterfaceGenericSubstitution::new(
            self.symbol_reference(data.owner().symbol())?,
            bindings,
        ));

        self.substitution_ids.insert(id, exported);

        Ok(exported)
    }

    fn trait_application_id(
        &mut self,
        id: TraitApplicationId,
    ) -> Result<InterfaceTraitApplicationId, PackageInterfaceExportError> {
        if let Some(id) = self.trait_application_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .trait_application_data(id)
            .map_err(|_| incomplete_type())?;

        let application = InterfaceTraitApplication::new(
            self.symbol_reference(data.definition().into())?,
            self.substitution_id(data.substitution())?,
        );

        let exported = InterfaceTraitApplicationId::new(index(self.trait_applications.len())?);
        self.trait_applications.push(application);
        self.trait_application_ids.insert(id, exported);

        Ok(exported)
    }

    fn callable_instance_id(
        &mut self,
        id: CallableInstanceId,
    ) -> Result<InterfaceCallableInstanceId, PackageInterfaceExportError> {
        if let Some(id) = self.callable_instance_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .callable_instance_data(id)
            .map_err(|_| incomplete_type())?;

        let instance = InterfaceCallableInstance::new(
            self.symbol_reference(data.definition().symbol())?,
            self.substitution_id(data.substitution())?,
        );

        let exported = InterfaceCallableInstanceId::new(index(self.callable_instances.len())?);
        self.callable_instances.push(instance);
        self.callable_instance_ids.insert(id, exported);

        Ok(exported)
    }

    fn implementation_instance_id(
        &mut self,
        id: ImplementationInstanceId,
    ) -> Result<InterfaceImplementationInstanceId, PackageInterfaceExportError> {
        if let Some(id) = self.implementation_instance_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .implementation_instance_data(id)
            .map_err(|_| incomplete_type())?;

        let instance = InterfaceImplementationInstance::new(
            self.symbol_reference(data.definition().into_any())?,
            self.substitution_id(data.substitution())?,
        );

        let exported =
            InterfaceImplementationInstanceId::new(index(self.implementation_instances.len())?);

        self.implementation_instances.push(instance);
        self.implementation_instance_ids.insert(id, exported);

        Ok(exported)
    }

    pub(super) fn constant_term_id(
        &mut self,
        id: ConstantTermId,
    ) -> Result<InterfaceConstantTermId, PackageInterfaceExportError> {
        if let Some(id) = self.constant_term_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .constant_term_data(id)
            .map_err(|_| incomplete_type())?;

        let term = match data.as_ref() {
            ConstantTermData::Value(value) => {
                InterfaceConstantTerm::Value(self.constant_value_id(*value)?)
            }
            ConstantTermData::IntegerLiteral { ty, value } => {
                InterfaceConstantTerm::IntegerLiteral {
                    ty: *ty,
                    value: value.clone(),
                }
            }
            ConstantTermData::Parameter(parameter) => {
                InterfaceConstantTerm::Parameter(self.symbol_reference((*parameter).into())?)
            }
            ConstantTermData::TargetFact(fact) => {
                InterfaceConstantTerm::TargetFact(self.symbol_reference((*fact).into())?)
            }
            ConstantTermData::Unary { operation, operand } => InterfaceConstantTerm::Unary {
                operation: *operation,
                operand: self.constant_term_id(*operand)?,
            },
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => InterfaceConstantTerm::Binary {
                operation: *operation,
                left: self.constant_term_id(*left)?,
                right: self.constant_term_id(*right)?,
            },
            ConstantTermData::Conversion { operand, target } => InterfaceConstantTerm::Conversion {
                operand: self.constant_term_id(*operand)?,
                target: self.type_id(*target)?,
            },
            ConstantTermData::NullablePresent(value) => {
                InterfaceConstantTerm::NullablePresent(self.constant_term_id(*value)?)
            }
            ConstantTermData::Tuple(values) => InterfaceConstantTerm::Tuple(
                values
                    .iter()
                    .copied()
                    .map(|value| self.constant_term_id(value))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantTermData::Array(values) => InterfaceConstantTerm::Array(
                values
                    .iter()
                    .copied()
                    .map(|value| self.constant_term_id(value))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantTermData::Product(fields) => InterfaceConstantTerm::Product(
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_term_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantTermData::Union { variant, fields } => InterfaceConstantTerm::Union {
                variant: self.symbol_reference((*variant).into())?,
                fields: fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_term_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
            ConstantTermData::DefinitionApplication {
                definition,
                substitution,
                selected_implementation,
            } => InterfaceConstantTerm::DefinitionApplication {
                definition: self.symbol_reference(definition.into_any())?,
                substitution: self.substitution_id(*substitution)?,
                selected_implementation: selected_implementation
                    .map(|instance| self.implementation_instance_id(instance))
                    .transpose()?,
            },
            ConstantTermData::Call {
                callable,
                selected_implementation,
                arguments,
            } => InterfaceConstantTerm::Call {
                callable: self.callable_instance_id(*callable)?,
                selected_implementation: selected_implementation
                    .map(|instance| self.implementation_instance_id(instance))
                    .transpose()?,
                arguments: arguments
                    .iter()
                    .copied()
                    .map(|argument| self.constant_term_id(argument))
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
            ConstantTermData::Projection(projection) => InterfaceConstantTerm::Projection {
                subject: self.constant_term_id(projection.subject())?,
                kind: match projection.kind() {
                    ConstantProjectionKind::TupleElement(ordinal) => {
                        InterfaceConstantProjection::TupleElement(ordinal)
                    }
                    ConstantProjectionKind::ArrayElement(index) => {
                        InterfaceConstantProjection::ArrayElement(self.constant_term_id(index)?)
                    }
                    ConstantProjectionKind::ProductField(field) => {
                        InterfaceConstantProjection::ProductField(
                            self.symbol_reference(field.into())?,
                        )
                    }
                    ConstantProjectionKind::UnionPayloadField(field) => {
                        InterfaceConstantProjection::UnionPayloadField(
                            self.symbol_reference(field.into())?,
                        )
                    }
                    ConstantProjectionKind::NullableValue => {
                        InterfaceConstantProjection::NullableValue
                    }
                },
            },
        };

        let exported = InterfaceConstantTermId::new(index(self.constant_terms.len())?);
        self.constant_terms.push(term);
        self.constant_term_ids.insert(id, exported);

        Ok(exported)
    }

    pub(super) fn constant_value_term_id(
        &mut self,
        value: ConstantValueId,
    ) -> Result<InterfaceConstantTermId, PackageInterfaceExportError> {
        let term = self
            .values
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|_| incomplete_type())?;

        self.constant_term_id(term)
    }

    pub(super) fn callable_template_reference(
        &mut self,
        data: bray_symbols::CallableInstanceData,
    ) -> Result<
        (
            bray_package_interface::InterfaceTemplateReference,
            InterfaceGenericSubstitutionId,
        ),
        PackageInterfaceExportError,
    > {
        Ok((
            bray_package_interface::InterfaceTemplateReference::Symbol(
                self.symbol_reference(data.definition().symbol())?,
            ),
            self.substitution_id(data.substitution())?,
        ))
    }

    pub(super) fn implementation_template_reference(
        &mut self,
        instance: ImplementationInstanceId,
    ) -> Result<
        (
            bray_package_interface::InterfaceImplementationReference,
            InterfaceGenericSubstitutionId,
        ),
        PackageInterfaceExportError,
    > {
        let data = self
            .values
            .implementation_instance_data(instance)
            .map_err(|_| incomplete_type())?;

        Ok((
            bray_package_interface::InterfaceImplementationReference::Symbol(
                self.symbol_reference(data.definition().into_any())?,
            ),
            self.substitution_id(data.substitution())?,
        ))
    }

    fn constant_value_id(
        &mut self,
        id: ConstantValueId,
    ) -> Result<InterfaceConstantValueId, PackageInterfaceExportError> {
        if let Some(id) = self.constant_value_ids.get(&id) {
            return Ok(*id);
        }

        let data = self
            .values
            .constant_value_data(id)
            .map_err(|_| incomplete_type())?;

        let kind = match data.kind() {
            ConstantValueKind::Error => return Err(incomplete_type()),
            ConstantValueKind::Boolean(value) => InterfaceConstantValueKind::Boolean(*value),
            ConstantValueKind::Character(value) => InterfaceConstantValueKind::Character(*value),
            ConstantValueKind::Integer(value) => InterfaceConstantValueKind::Integer(value.clone()),
            ConstantValueKind::Real(value) => InterfaceConstantValueKind::Real(*value),
            ConstantValueKind::Complex { real, imaginary } => InterfaceConstantValueKind::Complex {
                real: *real,
                imaginary: *imaginary,
            },
            ConstantValueKind::String(value) => InterfaceConstantValueKind::String(value.clone()),
            ConstantValueKind::Unit => InterfaceConstantValueKind::Unit,
            ConstantValueKind::NullableAbsent => InterfaceConstantValueKind::NullableAbsent,
            ConstantValueKind::NullablePresent(value) => {
                InterfaceConstantValueKind::NullablePresent(self.constant_value_id(*value)?)
            }
            ConstantValueKind::Tuple(values) => {
                InterfaceConstantValueKind::Tuple(self.constant_value_ids(values)?.into())
            }
            ConstantValueKind::Array(values) => {
                InterfaceConstantValueKind::Array(self.constant_value_ids(values)?.into())
            }
            ConstantValueKind::Product(fields) => InterfaceConstantValueKind::Product(
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_value_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            ),
            ConstantValueKind::Union { variant, fields } => InterfaceConstantValueKind::Union {
                variant: self.symbol_reference((*variant).into())?,
                fields: fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            self.symbol_reference((*field.field()).into())?,
                            self.constant_value_id(*field.value())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into(),
            },
        };

        let exported = InterfaceConstantValueId::new(index(self.constant_values.len())?);
        let ty = self.type_id(data.ty())?;

        self.constant_values
            .push(InterfaceConstantValue::new(ty, kind));

        self.constant_value_ids.insert(id, exported);

        Ok(exported)
    }

    fn constant_value_ids(
        &mut self,
        values: &[ConstantValueId],
    ) -> Result<Vec<InterfaceConstantValueId>, PackageInterfaceExportError> {
        values
            .iter()
            .copied()
            .map(|value| self.constant_value_id(value))
            .collect()
    }

    fn type_representation(
        &mut self,
        owner: AnySymbolId,
        representation: &bray_symbols::DeclaredTypeRepresentation,
    ) -> Result<InterfaceTypeRepresentation, PackageInterfaceExportError> {
        let union_tag_type = representation
            .union_tag_type()
            .map(|ty| self.type_id(ty))
            .transpose()?;

        let union_tags = representation
            .union_tags()
            .iter()
            .map(|tag| {
                Ok(InterfaceUnionTag::new(
                    self.symbol_reference(tag.variant().into())?,
                    tag.value().clone(),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let copy_dependencies = representation
            .copy_dependencies()
            .iter()
            .copied()
            .map(|parameter| self.symbol_reference(parameter.into()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(
            InterfaceTypeRepresentation::new(self.symbol_reference(owner)?)
                .with_layout(
                    representation.layout(),
                    representation.alignment(),
                    representation.packing(),
                    union_tag_type,
                )
                .with_union_tags(union_tags)
                .with_copy(representation.copy_contract(), copy_dependencies)
                .with_properties(
                    representation.is_plain_storage(),
                    representation.has_finite_size(),
                ),
        )
    }

    pub(super) fn symbol_reference(
        &self,
        symbol: AnySymbolId,
    ) -> Result<InterfaceSymbolReference, PackageInterfaceExportError> {
        if let Some(key) = self.keys.get(&symbol) {
            let id = self
                .surface
                .symbol_by_external_key(key)
                .ok_or_else(|| incomplete(symbol))?;

            return Ok(InterfaceSymbolReference::Local(id));
        }

        match self.graph.symbol_key(symbol).map(|key| key.data()) {
            Some(SymbolKeyData::CompilerKnownDeclaration { key, kind }) => {
                Ok(InterfaceSymbolReference::CompilerKnown {
                    key: key.clone(),
                    kind: *kind,
                })
            }
            Some(SymbolKeyData::External(key)) => {
                let dependency = self
                    .surface
                    .dependencies()
                    .iter()
                    .position(|dependency| dependency.package() == key.package_identity())
                    .and_then(|index| u32::try_from(index).ok())
                    .map(DependencyInterfaceId::new)
                    .ok_or_else(|| incomplete(symbol))?;

                Ok(InterfaceSymbolReference::Dependency {
                    dependency,
                    key: key.clone(),
                })
            }
            _ => Err(incomplete(symbol)),
        }
    }
}

fn index(length: usize) -> Result<u32, PackageInterfaceExportError> {
    u32::try_from(length).map_err(|_| incomplete_type())
}

fn checked_constraint_expression(
    compilation: &Compilation,
    generic: &bray_symbols::GenericDeclarationTemplate,
    unit: bray_declarations::SyntaxAnchor,
    expression: bray_symbols::DeclarationExpressionTemplate,
) -> Result<CheckedConstantExpression, PackageInterfaceExportError> {
    let cancellation = &compilation.state.cancellation;

    let key = compilation
        .constraint_unit_key(expression.owner(), unit)
        .map_err(|_| incomplete(expression.owner()))?;

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), cancellation)
        .map_err(|_| incomplete(expression.owner()))?;

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), cancellation)
        .map_err(|_| incomplete(expression.owner()))?;

    let diagnostics = DiagnosticBag::merged_all([
        bound.result().diagnostics(),
        semantics.result().diagnostics(),
    ]);

    if diagnostics.has_errors() {
        return Err(incomplete(expression.owner()));
    }

    let root = crate::compilation::generic_constraint::constraint_expression(
        bound.result().value(),
        expression.syntax(),
    )
    .ok_or_else(|| incomplete(expression.owner()))?;

    let values = compilation
        .semantic_value_store()
        .map_err(|_| incomplete(expression.owner()))?;

    let arguments = generic
        .parameters()
        .iter()
        .copied()
        .map(|parameter| {
            crate::compilation::substitution::generic_parameter_argument(values, parameter)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| incomplete(expression.owner()))?;

    let substitution = GenericSubstitutionData::try_new(
        generic.owner(),
        generic.parameters().iter().copied(),
        arguments,
    )
    .map_err(|_| incomplete(expression.owner()))?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(|_| incomplete(expression.owner()))?;

    let (references, reference_diagnostics) = compilation
        .concrete_call_references(
            bound.result().value(),
            &semantics.result().value().1,
            substitution,
            None,
            &BTreeMap::new(),
            ConstantEvaluationLimits::default(),
            cancellation,
        )
        .map_err(|_| incomplete(expression.owner()))?;

    if reference_diagnostics.has_errors() {
        return Err(incomplete(expression.owner()));
    }

    let context = CompilationCheckerContext::new(
        compilation
            .binder_facts_for(bound.result().value().key(), cancellation)
            .map_err(|_| incomplete(expression.owner()))?,
    );

    let semantic_context = semantic_unit_context_for(context.symbols(), bound.result().value())
        .map_err(|_| incomplete(expression.owner()))?;

    let request = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
        .map_err(|_| incomplete(expression.owner()))?;

    let resolver = crate::compilation::constant::CompilationConstantCallResolver::new(
        compilation,
        cancellation,
    );

    let input =
        ConstantEvaluationInput::new(&semantics.result().value().0, &semantics.result().value().1)
            .with_root(root)
            .with_references(references)
            .with_call_resolver(&resolver);

    let checked = checker_result(DefaultConstantChecker.check_constant_term(request, &input))
        .map_err(|_| incomplete(expression.owner()))?;

    if checked.diagnostics().has_errors() {
        return Err(incomplete(expression.owner()));
    }

    let ty = semantics
        .result()
        .value()
        .0
        .expression(root)
        .ok_or_else(|| incomplete(expression.owner()))?
        .ty();

    Ok(CheckedConstantExpression {
        term: *checked.value(),
        ty,
    })
}

const fn incomplete(symbol: AnySymbolId) -> PackageInterfaceExportError {
    PackageInterfaceExportError::IncompletePublicDeclarationFacts(symbol.kind())
}

const fn incomplete_type() -> PackageInterfaceExportError {
    PackageInterfaceExportError::IncompletePublicDeclarationFacts(SymbolKind::TypeCallableMember)
}

fn predicate_definition(
    binder: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> Result<Option<InterfacePredicateDefinitionState>, PackageInterfaceExportError> {
    let state = match symbol {
        AnySymbolId::Predicate(predicate) => {
            let fact = binder
                .symbol_fact(SymbolFactRequest::<PredicateDefinitionFact>::new(predicate))
                .map_err(|_| incomplete(symbol))?;

            if fact.diagnostics().has_errors() {
                return Err(incomplete(symbol));
            }

            predicate_definition_state(fact.value())
        }
        AnySymbolId::TraitPredicateMember(predicate) => {
            let fact = binder
                .symbol_fact(
                    SymbolFactRequest::<TraitPredicateMemberDefinitionFact>::new(predicate),
                )
                .map_err(|_| incomplete(symbol))?;

            if fact.diagnostics().has_errors() {
                return Err(incomplete(symbol));
            }

            predicate_definition_state(fact.value())
        }
        AnySymbolId::TraitPredicateFulfillment(predicate) => {
            let fact = binder
                .symbol_fact(
                    SymbolFactRequest::<TraitPredicateFulfillmentDefinitionFact>::new(predicate),
                )
                .map_err(|_| incomplete(symbol))?;

            if fact.diagnostics().has_errors() {
                return Err(incomplete(symbol));
            }

            predicate_definition_state(fact.value())
        }
        _ => return Ok(None),
    };

    state.map(Some).ok_or_else(|| incomplete(symbol))
}

const fn predicate_definition_state<T>(
    state: &PredicateDefinitionState<T>,
) -> Option<InterfacePredicateDefinitionState> {
    match state {
        PredicateDefinitionState::Defined(_) => Some(InterfacePredicateDefinitionState::Defined),
        PredicateDefinitionState::Required => Some(InterfacePredicateDefinitionState::Required),
        PredicateDefinitionState::OpaqueTrusted => {
            Some(InterfacePredicateDefinitionState::OpaqueTrusted)
        }
        PredicateDefinitionState::Error(_) => None,
    }
}

const fn dependency_requirement_kind(
    kind: DependencyRequirementKind,
) -> InterfaceDependencyRequirementKind {
    match kind {
        DependencyRequirementKind::StorageAlive => InterfaceDependencyRequirementKind::StorageAlive,
        DependencyRequirementKind::StorageInitialized => {
            InterfaceDependencyRequirementKind::StorageInitialized
        }
        DependencyRequirementKind::BorrowCapabilityActive(kind) => {
            InterfaceDependencyRequirementKind::BorrowCapabilityActive(kind)
        }
        DependencyRequirementKind::ExclusiveMutationAuthority => {
            InterfaceDependencyRequirementKind::ExclusiveMutationAuthority
        }
        DependencyRequirementKind::ScopedCapabilityLive => {
            InterfaceDependencyRequirementKind::ScopedCapabilityLive
        }
        DependencyRequirementKind::LifecycleObligation(kind) => {
            InterfaceDependencyRequirementKind::LifecycleObligation(kind)
        }
    }
}
