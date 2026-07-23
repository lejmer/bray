use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockItem, BoundCallableBodyKind, BoundControlTransferKind,
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit, BoundUnitKey,
    BoundUnitKind, BoundUnitRoot, BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes,
    CheckedTemplateKind, CheckedTemplateOperation, ExpressionTypeEntry, ExpressionTypeResult,
    walk_bound_unit_view,
};
use bray_checker::{
    CheckerUnitView, ConstantChecker, ConstantEvaluationInput, ConstantEvaluator,
    ConstantReferenceResolution, DefaultConstantChecker, DefaultConstantEvaluator,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, CallableDefinitionId, ConstantDefinition,
    ConstantDefinitionFact, ConstantDefinitionState, ConstantInstanceKey,
    ConstantInstanceValueFact, ConstantTermData, ConstantTermId, ConstantValueData,
    ConstantValueId, ConstantValueKind, ErrorConstantDefinition, GenericOwnerId,
    GenericSubstitutionData, GenericSubstitutionId, SemanticFactResult, SymbolFactRequest,
    TraitConstantFulfillmentDefinitionFact, TraitConstantMemberDefinitionFact,
};

use super::super::Compilation;
use super::super::binder::{binder_fact_error, imported_declaration_template};
use super::super::checker::checker_result;
use super::super::unit::semantic_unit_context_for;
use super::call::CompilationConstantCallResolver;
use crate::fact::{
    CancellationToken, CompilationFactKey, ConstantInstanceFactKey, FactQueryError,
    PublishedUnitFact,
};

impl Compilation {
    /// Returns the semantic definition state of one constant declaration.
    pub fn constant_definition(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Arc<DiagnosticResult<ConstantDefinitionState>>, FactQueryError> {
        let facts = self.binder_facts(&self.state.cancellation)?;

        match definition {
            AnyConstantDefinitionId::Constant(owner) => facts
                .symbol_fact(SymbolFactRequest::<ConstantDefinitionFact>::new(owner))
                .map_err(binder_fact_error),
            AnyConstantDefinitionId::TraitMember(owner) => facts
                .symbol_fact(SymbolFactRequest::<TraitConstantMemberDefinitionFact>::new(
                    owner,
                ))
                .map_err(binder_fact_error),
            AnyConstantDefinitionId::TraitFulfillment(owner) => facts
                .symbol_fact(
                    SymbolFactRequest::<TraitConstantFulfillmentDefinitionFact>::new(owner),
                )
                .map_err(binder_fact_error),
        }
    }

    /// Returns the checked symbolic term for one constant definition initializer.
    pub fn symbolic_constant_term(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Arc<DiagnosticResult<ConstantTermId>>, FactQueryError> {
        let key = self
            .constant_template_key(definition)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let published =
            self.symbolic_constant_term_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns the closed value of one exact constant instance.
    pub fn constant_instance(
        &self,
        instance: ConstantInstanceKey,
    ) -> Result<Arc<SemanticFactResult<ConstantInstanceValueFact>>, FactQueryError> {
        self.constant_instance_with_cancellation(instance, &self.state.cancellation)
    }

    pub(in crate::compilation) fn compute_constant_definition(
        &self,
        definition: AnyConstantDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ConstantDefinitionState>, FactQueryError> {
        let Some(key) = self.constant_template_key(definition)? else {
            let facts = self.binder_facts(cancellation)?;

            let imported = facts
                .imported_fact_address(definition.into_any())
                .map_err(binder_fact_error)?;

            if let Some(address) = imported {
                let result = imported_declaration_template(
                    &facts,
                    address,
                    CheckedTemplateKind::ConstantDefinition,
                )
                .map_err(binder_fact_error)?;

                let diagnostics = result.diagnostics().clone();
                let state = imported_constant_definition(definition, result.value().as_ref())?;

                return Ok(DiagnosticResult::new(state, diagnostics));
            }

            return match definition {
                AnyConstantDefinitionId::TraitMember(_) => Ok(
                    DiagnosticResult::without_diagnostics(ConstantDefinitionState::Required),
                ),
                AnyConstantDefinitionId::Constant(_)
                | AnyConstantDefinitionId::TraitFulfillment(_) => {
                    Err(FactQueryError::InfrastructureFailure)
                }
            };
        };

        let term = self.symbolic_constant_term_with_cancellation(key.clone(), cancellation)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let root = expression_root(bound.result().value())?;
        let types = self.expression_types_with_cancellation(key, cancellation)?;

        let Some(root_type) = types.result().value().expression(root) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let diagnostics = term.result().diagnostics().clone();

        let state = if diagnostics.has_errors() {
            ConstantDefinitionState::Error(ErrorConstantDefinition)
        } else {
            ConstantDefinitionState::Defined(ConstantDefinition::new(
                root_type.ty(),
                *term.result().value(),
            ))
        };

        Ok(DiagnosticResult::new(state, diagnostics))
    }

    pub(in crate::compilation) fn symbolic_constant_term_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<ConstantTermId>>, FactQueryError> {
        if key.kind() != BoundUnitKind::ConstantTemplate {
            return Err(FactQueryError::InfrastructureFailure);
        }

        self.unit_fact(
            &self.state.symbolic_constant_terms,
            CompilationFactKey::SymbolicConstantTerm(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let semantics =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(context.symbols(), bound.result().value())?;

                let references = self.symbolic_references(bound.result().value())?;

                let resolver = CompilationConstantCallResolver::new(self, cancellation);

                let input = ConstantEvaluationInput::new(
                    &semantics.result().value().0,
                    &semantics.result().value().1,
                )
                .with_references(references)
                .with_call_resolver(&resolver);

                let unit =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                        .map_err(|error| {
                            FactQueryError::CheckerInfrastructure(
                                bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                            )
                        })?;

                let checked =
                    checker_result(DefaultConstantChecker.check_constant_term(unit, &input))?;

                let diagnostics = DiagnosticBag::merged_all([
                    semantics.result().diagnostics(),
                    checked.diagnostics(),
                ]);

                Ok((
                    DiagnosticResult::new(*checked.value(), diagnostics),
                    Box::new([]),
                ))
            },
        )
    }

    pub(in crate::compilation) fn constant_instance_with_cancellation(
        &self,
        instance: ConstantInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<SemanticFactResult<ConstantInstanceValueFact>>, FactQueryError> {
        let target = self.options().selected_target().profile().clone();
        let key = ConstantInstanceFactKey::new(instance, target);
        let cell = self.state.constant_instances.cell(key.clone())?;

        let published = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ConstantInstance(key),
            cancellation,
            || {
                self.compute_constant_instance(instance, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(published))
    }

    fn compute_constant_instance(
        &self,
        instance: ConstantInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<SemanticFactResult<ConstantInstanceValueFact>, FactQueryError> {
        let key = self
            .constant_template_key(instance.definition())?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let term = self.symbolic_constant_term_with_cancellation(key.clone(), cancellation)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(&key, cancellation)?;

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let types = substitute_expression_types(
            self.semantic_value_store()?,
            &semantics.result().value().0,
            instance.substitution().substitution(),
        )?;

        if term.result().diagnostics().has_errors() {
            let root = expression_root(bound.result().value())?;

            let ty = types
                .expression(root)
                .ok_or(FactQueryError::InfrastructureFailure)?
                .ty();

            let value = self
                .semantic_value_store()?
                .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Error))
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            return Ok(DiagnosticResult::without_diagnostics(value));
        }

        let (references, dependency_diagnostics) =
            self.concrete_references(bound.result().value(), instance, cancellation)?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(&types, &semantics.result().value().1)
            .with_references(references)
            .with_call_resolver(&resolver);

        let unit = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(
                    bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                )
            })?;

        let evaluated = checker_result(DefaultConstantEvaluator.evaluate_constant(unit, &input))?;
        let diagnostics = dependency_diagnostics.merged(evaluated.diagnostics());

        Ok(DiagnosticResult::new(*evaluated.value(), diagnostics))
    }

    fn symbolic_references(
        &self,
        bound: &BoundUnit,
    ) -> Result<Vec<(BoundExpressionId, ConstantReferenceResolution)>, FactQueryError> {
        let values = self.semantic_value_store()?;

        collect_references(bound, |_, target| match target {
            BoundReferenceTarget::Surface(AnySymbolId::GenericConstParameter(parameter)) => values
                .intern_constant_term(ConstantTermData::Parameter(parameter))
                .map(ConstantReferenceResolution::Term)
                .map_err(|_| FactQueryError::InfrastructureFailure),
            BoundReferenceTarget::Surface(symbol) => {
                let Some(definition) = constant_definition_id(symbol) else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let substitution = empty_substitution(values, definition)?;

                let term = values
                    .intern_constant_term(ConstantTermData::DefinitionApplication {
                        definition,
                        substitution,
                        selected_implementation: None,
                    })
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                Ok(ConstantReferenceResolution::Term(term))
            }
            BoundReferenceTarget::Local(_) => Err(FactQueryError::InfrastructureFailure),
        })
    }

    fn concrete_references(
        &self,
        bound: &BoundUnit,
        instance: ConstantInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(BoundExpressionId, ConstantReferenceResolution)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        self.concrete_references_for(
            bound,
            instance.substitution().substitution(),
            instance.selected_implementation(),
            &BTreeMap::new(),
            Some(instance),
            cancellation,
        )
    }

    pub(super) fn concrete_references_for(
        &self,
        bound: &BoundUnit,
        substitution: GenericSubstitutionId,
        selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
        parameters: &BTreeMap<AnySymbolId, ConstantValueId>,
        current_constant: Option<ConstantInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(BoundExpressionId, ConstantReferenceResolution)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mut dependency_diagnostics = DiagnosticBag::new();

        let references = collect_references(bound, |_, target| match target {
            BoundReferenceTarget::Surface(symbol) if parameters.contains_key(&symbol) => parameters
                .get(&symbol)
                .copied()
                .map(ConstantReferenceResolution::Value)
                .ok_or(FactQueryError::InfrastructureFailure),
            BoundReferenceTarget::Surface(AnySymbolId::GenericConstParameter(parameter)) => {
                let Some(bray_symbols::GenericArgument::Constant(term)) = substitution
                    .argument_for(bray_symbols::GenericParameterSymbolId::Const(parameter))
                else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let data = values
                    .constant_term_data(term)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                match data.as_ref() {
                    ConstantTermData::Value(value) => {
                        Ok(ConstantReferenceResolution::Value(*value))
                    }
                    _ => Err(FactQueryError::InfrastructureFailure),
                }
            }
            BoundReferenceTarget::Surface(symbol) => {
                let Some(definition) = constant_definition_id(symbol) else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let dependency = if current_constant
                    .is_some_and(|instance| definition == instance.definition())
                {
                    current_constant.ok_or(FactQueryError::InfrastructureFailure)?
                } else {
                    ConstantInstanceKey::new(
                        definition,
                        empty_concrete_substitution(values, definition)?,
                        selected_implementation_for_reference(definition, selected_implementation),
                    )
                };

                match self.constant_instance_with_cancellation(dependency, cancellation) {
                    Ok(result) => {
                        dependency_diagnostics =
                            dependency_diagnostics.merged(result.diagnostics());

                        Ok(ConstantReferenceResolution::Value(*result.value()))
                    }
                    Err(FactQueryError::Cycle(_)) => Ok(ConstantReferenceResolution::Cycle),
                    Err(error) => Err(error),
                }
            }
            BoundReferenceTarget::Local(_) => {
                // TODO(BRA-246): Evaluate local constants and immutable local bindings.
                Err(FactQueryError::InfrastructureFailure)
            }
        })?;

        Ok((references, dependency_diagnostics))
    }

    fn constant_template_key(
        &self,
        definition: AnyConstantDefinitionId,
    ) -> Result<Option<BoundUnitKey>, FactQueryError> {
        Ok(self.constant_template_keys()?.get(&definition).cloned())
    }

    pub(super) fn callable_body_key(
        &self,
        definition: CallableDefinitionId,
    ) -> Result<Option<BoundUnitKey>, FactQueryError> {
        let result = self.fact(
            CompilationFactKey::CallableBodyKeys,
            &self.state.callable_body_keys,
            || {
                let symbols = self.symbol_graph()?;
                let mut bodies = BTreeMap::new();

                for key in self.declared_unit_keys()? {
                    if key.kind() != BoundUnitKind::CallableBody {
                        continue;
                    }

                    let symbol = symbols
                        .symbol_for_key(key.declared_owner())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let definition = CallableDefinitionId::try_new(symbol)
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    if bodies.insert(definition, key).is_some() {
                        return Err(FactQueryError::InfrastructureFailure);
                    }
                }

                Ok(bodies)
            },
        );

        match result {
            Ok(bodies) => Ok(bodies.get(&definition).cloned()),
            Err(error) => Err(error.clone()),
        }
    }

    fn constant_template_keys(
        &self,
    ) -> Result<&BTreeMap<AnyConstantDefinitionId, BoundUnitKey>, FactQueryError> {
        let result = self.fact(
            CompilationFactKey::ConstantTemplateKeys,
            &self.state.constant_template_keys,
            || {
                let symbols = self.symbol_graph()?;
                let mut templates = BTreeMap::new();

                for key in self.declared_unit_keys()? {
                    if key.kind() != BoundUnitKind::ConstantTemplate {
                        continue;
                    }

                    let symbol = symbols
                        .symbol_for_key(key.declared_owner())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let definition = constant_definition_id(symbol)
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    if templates.insert(definition, key).is_some() {
                        return Err(FactQueryError::InfrastructureFailure);
                    }
                }

                Ok(templates)
            },
        );

        match result {
            Ok(templates) => Ok(templates),
            Err(error) => Err(error.clone()),
        }
    }
}

fn imported_constant_definition(
    definition: AnyConstantDefinitionId,
    fact: Option<&bray_package_interface::ImportedDeclarationTemplateFact>,
) -> Result<ConstantDefinitionState, FactQueryError> {
    let Some(fact) = fact else {
        return match definition {
            AnyConstantDefinitionId::TraitMember(_) => Ok(ConstantDefinitionState::Required),
            AnyConstantDefinitionId::Constant(_) | AnyConstantDefinitionId::TraitFulfillment(_) => {
                Err(FactQueryError::InfrastructureFailure)
            }
        };
    };

    let template = fact.template();

    let index = usize::try_from(template.result().raw())
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let result = template
        .nodes()
        .get(index)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let CheckedTemplateOperation::Constant(term) = result.operation() else {
        return Err(FactQueryError::InfrastructureFailure);
    };

    Ok(ConstantDefinitionState::Defined(ConstantDefinition::new(
        result.ty(),
        *term,
    )))
}

pub(super) fn call_parameter_values(
    values: &bray_symbols::SemanticValueStore,
    signature: &bray_symbols::CallableSignature,
    arguments: &[ConstantValueId],
) -> Result<BTreeMap<AnySymbolId, ConstantValueId>, FactQueryError> {
    let parameters = signature
        .receiver()
        .map(|receiver| (AnySymbolId::from(receiver.parameter()), receiver.ty()))
        .into_iter()
        .chain(
            signature
                .parameters()
                .iter()
                .map(|parameter| (AnySymbolId::from(parameter.parameter()), parameter.ty())),
        )
        .collect::<Vec<_>>();

    if parameters.len() != arguments.len() {
        return Err(FactQueryError::InfrastructureFailure);
    }

    let mut resolved = BTreeMap::new();

    for ((parameter, expected), value) in parameters.into_iter().zip(arguments.iter().copied()) {
        let actual = values
            .constant_value_data(value)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if actual.ty() != expected {
            return Err(FactQueryError::InfrastructureFailure);
        }

        resolved.insert(parameter, value);
    }

    Ok(resolved)
}

pub(super) fn constant_callable_root(bound: &BoundUnit) -> Option<BoundExpressionId> {
    let BoundUnitRoot::CallableBody(body) = bound.root() else {
        return None;
    };

    let body = bound.view().callable_body(body)?;

    let BoundCallableBodyKind::Block(block) = body.kind() else {
        return None;
    };

    let block = bound.view().block(block)?;

    let [BoundBlockItem::Expression(expression)] = block.items() else {
        return None;
    };

    match bound.view().expression(*expression)? {
        BoundExpression::ControlTransfer(transfer)
            if transfer.kind() == BoundControlTransferKind::Return =>
        {
            transfer.operand()
        }
        _ => Some(*expression),
    }
}

fn collect_references(
    bound: &BoundUnit,
    mut resolve: impl FnMut(
        BoundExpressionId,
        BoundReferenceTarget,
    ) -> Result<ConstantReferenceResolution, FactQueryError>,
) -> Result<Vec<(BoundExpressionId, ConstantReferenceResolution)>, FactQueryError> {
    let mut references = Vec::new();
    let mut failure = None;

    let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        let Some(BoundExpression::Name(name)) = bound.view().expression(expression) else {
            return BoundWalkControl::Continue;
        };

        match resolve(expression, name.target()) {
            Ok(resolution) => references.push((expression, resolution)),
            Err(error) => {
                failure = Some(error);

                return BoundWalkControl::Stop;
            }
        }

        BoundWalkControl::Continue
    });

    if let Some(error) = failure {
        return Err(error);
    }

    if !matches!(outcome, bray_bound_tree::BoundWalkOutcome::Completed) {
        return Err(FactQueryError::InfrastructureFailure);
    }

    Ok(references)
}

pub(super) fn substitute_expression_types(
    values: &bray_symbols::SemanticValueStore,
    types: &CheckedExpressionTypes,
    substitution: GenericSubstitutionId,
) -> Result<CheckedExpressionTypes, FactQueryError> {
    let entries = types
        .entries()
        .iter()
        .map(|entry| {
            let result = entry.result();

            let ty = values
                .substitute_type(result.ty(), substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            Ok(ExpressionTypeEntry::new(
                entry.expression(),
                ExpressionTypeResult::new(ty, result.status()),
            ))
        })
        .collect::<Result<Vec<_>, FactQueryError>>()?;

    Ok(CheckedExpressionTypes::new(
        types.unit(),
        types.kind(),
        entries,
    ))
}

fn expression_root(bound: &BoundUnit) -> Result<BoundExpressionId, FactQueryError> {
    match bound.root() {
        BoundUnitRoot::Expression(root) => Ok(root),
        BoundUnitRoot::CallableBody(_)
        | BoundUnitRoot::AnonymousCallable { .. }
        | BoundUnitRoot::ExpressionSequence(_) => Err(FactQueryError::InfrastructureFailure),
    }
}

pub(in crate::compilation) fn constant_definition_id(
    symbol: AnySymbolId,
) -> Option<AnyConstantDefinitionId> {
    match symbol {
        AnySymbolId::Constant(id) => Some(AnyConstantDefinitionId::Constant(id)),
        AnySymbolId::TraitConstantMember(id) => Some(AnyConstantDefinitionId::TraitMember(id)),
        AnySymbolId::TraitConstantFulfillment(id) => {
            Some(AnyConstantDefinitionId::TraitFulfillment(id))
        }
        _ => None,
    }
}

fn empty_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnyConstantDefinitionId,
) -> Result<bray_symbols::GenericSubstitutionId, FactQueryError> {
    let owner = GenericOwnerId::try_new(definition.into_any())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let data = GenericSubstitutionData::try_new(owner, [], [])
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    values
        .intern_generic_substitution(data)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

pub(in crate::compilation) fn empty_concrete_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnyConstantDefinitionId,
) -> Result<bray_symbols::ConcreteGenericSubstitutionId, FactQueryError> {
    let substitution = empty_substitution(values, definition)?;

    values
        .require_concrete_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

const fn selected_implementation_for_reference(
    definition: AnyConstantDefinitionId,
    selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
) -> Option<bray_symbols::ImplementationInstanceId> {
    match definition {
        AnyConstantDefinitionId::TraitMember(_) => selected_implementation,
        AnyConstantDefinitionId::Constant(_) | AnyConstantDefinitionId::TraitFulfillment(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{
        AnyConstantDefinitionId, CallableDefinitionId, CallableInstanceData,
        CallableParameterSignature, CallableParameterSymbolId, CallableSignature,
        ConstantDefinitionState, ConstantInstanceKey, ConstantTermData, ConstantValueData,
        ConstantValueId, ConstantValueKind, GenericArgument, GenericOwnerId,
        GenericParameterSymbolId, GenericSubstitutionData, IntegerConstant, IntegerSign,
        ReceiverMode, ReceiverParameterSignature, ReceiverParameterSymbolId, SymbolId,
        SymbolOrigin,
    };
    use bray_target::{TargetIdentity, TargetProfile};

    use super::{call_parameter_values, constant_callable_root, empty_concrete_substitution};
    use crate::SelectedTarget;
    use crate::fact::{ConstantCallFactKey, ConstantInstanceFactKey, FactCellTestEvent};
    use crate::test_support::{FactTestGate, compilation};
    use bray_checker::{ConstantCallRequest, ConstantEvaluationLimits};

    #[test]
    fn constant_instances_demand_only_referenced_definitions_and_reuse_publications() {
        let compilation = Arc::new(compilation(concat!(
            "module app;\n",
            "const first: i32 = 1;\n",
            "const second: i32 = first;\n",
            "const unrelated: i32 = 99;\n",
        )));

        let definitions = source_constant_definitions(&compilation);

        let [first, second, unrelated] = definitions.as_slice() else {
            panic!("test source must produce three constant definitions");
        };

        let first = instance_key(&compilation, *first);
        let second = instance_key(&compilation, *second);
        let unrelated = instance_key(&compilation, *unrelated);

        let second_unit = compilation
            .constant_template_key(second.definition())
            .unwrap_or_else(|error| panic!("second constant key must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("second constant must have a body"));

        assert!(!instance_is_published(&compilation, first));
        assert!(!instance_is_published(&compilation, second));
        assert!(!instance_is_published(&compilation, unrelated));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);
        let cache_key = instance_fact_key(&compilation, second);

        compilation
            .state
            .constant_instances
            .cell(cache_key)
            .unwrap_or_else(|error| panic!("constant instance cell must exist: {error:?}"))
            .set_test_observer(gate.observer())
            .unwrap_or_else(|error| panic!("constant instance observer must attach: {error:?}"));

        let (left, right) = std::thread::scope(|scope| {
            let left_compilation = Arc::clone(&compilation);
            let left = scope.spawn(move || left_compilation.constant_instance(second));

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let right_compilation = Arc::clone(&compilation);
            let right = scope.spawn(move || right_compilation.constant_instance(second));

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            (
                left.join()
                    .unwrap_or_else(|_| panic!("first constant worker must not panic")),
                right
                    .join()
                    .unwrap_or_else(|_| panic!("second constant worker must not panic")),
            )
        });

        let left = left.unwrap_or_else(|error| panic!("constant instance must publish: {error:?}"));

        let right =
            right.unwrap_or_else(|error| panic!("constant instance must publish: {error:?}"));

        assert!(Arc::ptr_eq(&left, &right));
        assert_eq!(integer_value(&compilation, *left.value()), 1);
        assert!(left.diagnostics().is_empty());

        assert!(instance_is_published(&compilation, first));
        assert!(instance_is_published(&compilation, second));
        assert!(!instance_is_published(&compilation, unrelated));

        assert_eq!(
            compilation
                .state
                .checked_control_flow
                .is_published(&second_unit),
            Ok(false)
        );
    }

    #[test]
    fn selected_constant_calls_evaluate_source_bodies_once_per_exact_request() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const func answer() -> i32\n",
            "{\n",
            "    return 42;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must publish: {error:?}"));

        let function = symbols
            .functions()
            .iter()
            .find(|function| function.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must produce one function"));

        let definition = CallableDefinitionId::try_new(function.id().into())
            .unwrap_or_else(|| panic!("source function must be callable"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let substitution = GenericSubstitutionData::try_new(
            GenericOwnerId::try_new(function.id().into())
                .unwrap_or_else(|| panic!("source function must own a substitution")),
            [],
            [],
        )
        .unwrap_or_else(|error| panic!("empty function substitution must validate: {error:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("empty function substitution must intern: {error:?}"));

        let ty = i32_type(&compilation);

        let request = ConstantCallRequest::new(
            CallableInstanceData::new(definition, substitution),
            None,
            [],
            ty,
            ConstantEvaluationLimits::default(),
        );

        let callable = values
            .intern_callable_instance(request.callable())
            .unwrap_or_else(|error| panic!("constant callable instance must intern: {error:?}"));

        let target = compilation.options().selected_target().profile().clone();

        let shallow = ConstantCallFactKey::new(
            callable,
            None,
            Arc::from(request.arguments()),
            ty,
            target.clone(),
            ConstantEvaluationLimits::default().with_call_depth(1),
        );

        let deep = ConstantCallFactKey::new(
            callable,
            None,
            Arc::from(request.arguments()),
            ty,
            target,
            ConstantEvaluationLimits::default().with_call_depth(8),
        );

        assert_ne!(shallow, deep);
        assert_eq!(shallow.dependency_key(), deep.dependency_key());

        let body_key = compilation
            .callable_body_key(definition)
            .unwrap_or_else(|error| panic!("constant callable body key must resolve: {error:?}"))
            .unwrap_or_else(|| panic!("source constant callable must have a body"));

        let body = compilation
            .bound_unit(body_key)
            .unwrap_or_else(|error| panic!("constant callable body must bind: {error:?}"));

        assert!(
            constant_callable_root(body.value()).is_some(),
            "unexpected constant callable body: {:#?}",
            body.value()
        );

        let first = compilation
            .constant_call_with_cancellation(&request, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("selected constant call must publish: {error:?}"));

        let second = compilation
            .constant_call_with_cancellation(&request, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("selected constant call must be cached: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());

        let Some(value) = *first.value() else {
            panic!("constant callable must be eligible for evaluation");
        };

        assert_eq!(integer_value(&compilation, value), 42);
    }

    #[test]
    fn constant_call_arguments_map_the_receiver_before_ordinary_parameters() {
        let compilation = compilation("module app;");

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let ty = i32_type(&compilation);

        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(70));
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(71));

        let signature = CallableSignature::new(
            ty,
            Some(ReceiverParameterSignature::new(
                receiver,
                ty,
                ReceiverMode::Shared,
            )),
            [CallableParameterSignature::new(parameter, ty)],
            ty,
        );

        let first = integer_constant(&compilation, ty, 1);
        let second = integer_constant(&compilation, ty, 2);

        let parameters = call_parameter_values(values, &signature, &[first, second])
            .unwrap_or_else(|error| panic!("constant call arguments must map: {error:?}"));

        assert_eq!(parameters.get(&receiver.into()), Some(&first));
        assert_eq!(parameters.get(&parameter.into()), Some(&second));
    }

    #[test]
    fn constant_definitions_are_cached_without_closing_instances() {
        let compilation = compilation(concat!("module app;\n", "const value: i32 = 1;\n",));
        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let first = compilation
            .constant_definition(*definition)
            .unwrap_or_else(|error| panic!("constant template must publish: {error:?}"));

        let second = compilation
            .constant_definition(*definition)
            .unwrap_or_else(|error| panic!("constant template must be cached: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());
        assert!(matches!(first.value(), ConstantDefinitionState::Defined(_)));

        assert!(!instance_is_published(
            &compilation,
            instance_key(&compilation, *definition)
        ));
    }

    #[test]
    fn symbolic_terms_do_not_evaluate_constant_instances() {
        let compilation = compilation(concat!(
            "module app;\n",
            "const first: i32 = 1;\n",
            "const second: i32 = first;\n",
        ));

        let definitions = source_constant_definitions(&compilation);

        let [first, second] = definitions.as_slice() else {
            panic!("test source must produce two constant definitions");
        };

        let term = compilation
            .symbolic_constant_term(*second)
            .unwrap_or_else(|error| panic!("symbolic constant term must publish: {error:?}"));

        assert!(
            term.diagnostics().is_empty(),
            "unexpected diagnostics: {:?}",
            term.diagnostics()
        );

        let data = compilation
            .semantic_value_store()
            .and_then(|values| {
                values
                    .constant_term_data(*term.value())
                    .map_err(|_| crate::FactQueryError::InfrastructureFailure)
            })
            .unwrap_or_else(|error| panic!("symbolic constant term must be interned: {error:?}"));

        assert!(matches!(
            data.as_ref(),
            ConstantTermData::DefinitionApplication { .. }
        ));

        assert!(!instance_is_published(
            &compilation,
            instance_key(&compilation, *first)
        ));

        assert!(!instance_is_published(
            &compilation,
            instance_key(&compilation, *second)
        ));
    }

    #[test]
    fn constant_instance_cycles_publish_one_deterministic_diagnostic() {
        let compilation = compilation(concat!("module app;\n", "const value: i32 = value;\n",));
        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let result = compilation
            .constant_instance(instance_key(&compilation, *definition))
            .unwrap_or_else(|error| panic!("cyclic constant must recover: {error:?}"));

        let repeated = compilation
            .constant_instance(instance_key(&compilation, *definition))
            .unwrap_or_else(|error| panic!("cyclic constant must remain cached: {error:?}"));

        assert!(Arc::ptr_eq(&result, &repeated));
        assert_eq!(result.diagnostics(), repeated.diagnostics());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingCyclicConstantDefinition)
                .count(),
            1
        );

        assert!(matches!(
            constant_value(&compilation, *result.value()).kind(),
            ConstantValueKind::Error
        ));
    }

    #[test]
    fn generic_constant_instances_are_separated_by_concrete_substitution() {
        let compilation = Arc::new(compilation(concat!(
            "module app;\n",
            "struct Buffer<const count: i32>\n",
            "{\n",
            "    const size: i32 = count;\n",
            "}\n",
        )));

        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must publish: {error:?}"));

        let structure = symbols
            .structures()
            .iter()
            .find(|structure| structure.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must produce one structure"));

        let [parameter] = structure.generic_const_parameters() else {
            panic!("test structure must produce one const parameter");
        };

        let ty = i32_type(&compilation);

        let first = concrete_substitution(&compilation, structure.id().into(), *parameter, ty, 3);
        let second = concrete_substitution(&compilation, structure.id().into(), *parameter, ty, 7);

        let first_key = ConstantInstanceKey::new(*definition, first, None);
        let second_key = ConstantInstanceKey::new(*definition, second, None);

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        for instance in [first_key, second_key] {
            compilation
                .state
                .constant_instances
                .cell(instance_fact_key(&compilation, instance))
                .unwrap_or_else(|error| panic!("constant instance cell must exist: {error:?}"))
                .set_test_observer(gate.observer())
                .unwrap_or_else(|error| {
                    panic!("constant instance observer must attach: {error:?}")
                });
        }

        let (first, second) = std::thread::scope(|scope| {
            let first_compilation = Arc::clone(&compilation);
            let first = scope.spawn(move || first_compilation.constant_instance(first_key));

            let second_compilation = Arc::clone(&compilation);
            let second = scope.spawn(move || second_compilation.constant_instance(second_key));

            gate.wait_until_observed(FactCellTestEvent::Computing, 2);
            gate.release();

            (
                first
                    .join()
                    .unwrap_or_else(|_| panic!("first constant worker must not panic")),
                second
                    .join()
                    .unwrap_or_else(|_| panic!("second constant worker must not panic")),
            )
        });

        let first =
            first.unwrap_or_else(|error| panic!("first generic instance must publish: {error:?}"));

        let second = second
            .unwrap_or_else(|error| panic!("second generic instance must publish: {error:?}"));

        assert!(
            first.diagnostics().is_empty(),
            "unexpected first diagnostics: {:?}",
            first.diagnostics()
        );
        assert!(
            second.diagnostics().is_empty(),
            "unexpected second diagnostics: {:?}",
            second.diagnostics()
        );

        assert_eq!(integer_value(&compilation, *first.value()), 3);
        assert_eq!(integer_value(&compilation, *second.value()), 7);
        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn constant_instance_fact_keys_include_the_complete_target_profile() {
        let compilation = compilation(concat!("module app;\n", "const value: i32 = 1;\n",));
        let definitions = source_constant_definitions(&compilation);

        let [definition] = definitions.as_slice() else {
            panic!("test source must produce one constant definition");
        };

        let instance = instance_key(&compilation, *definition);
        let baseline = SelectedTarget::baseline();

        let alternate_identity = TargetIdentity::try_new("alternate-test-target")
            .unwrap_or_else(|| panic!("alternate target identity must be valid"));

        let alternate = TargetProfile::try_new(
            alternate_identity,
            baseline.profile().machine().clone(),
            baseline.profile().facts().clone(),
        )
        .unwrap_or_else(|error| panic!("alternate target profile must be valid: {error:?}"));

        assert_ne!(
            ConstantInstanceFactKey::new(instance, baseline.profile().clone()),
            ConstantInstanceFactKey::new(instance, alternate)
        );
    }

    fn source_constant_definitions(
        compilation: &crate::Compilation,
    ) -> Vec<AnyConstantDefinitionId> {
        compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must publish: {error:?}"))
            .constants()
            .iter()
            .filter(|constant| constant.origin() == SymbolOrigin::Source)
            .map(|constant| AnyConstantDefinitionId::Constant(constant.id()))
            .collect()
    }

    fn instance_key(
        compilation: &crate::Compilation,
        definition: AnyConstantDefinitionId,
    ) -> ConstantInstanceKey {
        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let substitution = empty_concrete_substitution(values, definition)
            .unwrap_or_else(|error| panic!("empty substitution must be concrete: {error:?}"));

        ConstantInstanceKey::new(definition, substitution, None)
    }

    fn instance_fact_key(
        compilation: &crate::Compilation,
        instance: ConstantInstanceKey,
    ) -> ConstantInstanceFactKey {
        ConstantInstanceFactKey::new(
            instance,
            compilation.options().selected_target().profile().clone(),
        )
    }

    fn instance_is_published(
        compilation: &crate::Compilation,
        instance: ConstantInstanceKey,
    ) -> bool {
        compilation
            .state
            .constant_instances
            .is_published(&instance_fact_key(compilation, instance))
            .unwrap_or_else(|error| panic!("constant cache must be readable: {error:?}"))
    }

    fn concrete_substitution(
        compilation: &crate::Compilation,
        owner: bray_symbols::AnySymbolId,
        parameter: bray_symbols::GenericConstParameterSymbolId,
        ty: bray_symbols::TypeId,
        value: u8,
    ) -> bray_symbols::ConcreteGenericSubstitutionId {
        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let value = integer_constant(compilation, ty, value);

        let term = values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("constant argument term must be interned: {error:?}"));

        let owner = GenericOwnerId::try_new(owner)
            .unwrap_or_else(|| panic!("test generic owner must accept parameters"));

        let data = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::Const(parameter)],
            [GenericArgument::Constant(term)],
        )
        .unwrap_or_else(|error| panic!("generic substitution must be valid: {error:?}"));

        let substitution = values
            .intern_generic_substitution(data)
            .unwrap_or_else(|error| panic!("generic substitution must be interned: {error:?}"));

        values
            .require_concrete_substitution(substitution)
            .unwrap_or_else(|error| panic!("generic substitution must be concrete: {error:?}"))
    }

    fn i32_type(compilation: &crate::Compilation) -> bray_symbols::TypeId {
        let definition = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(
                bray_compiler_known::RepresentationRole::ScalarI32,
            )
            .unwrap_or_else(|| panic!("i32 representation must be available"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let owner = GenericOwnerId::try_new(definition.into())
            .unwrap_or_else(|| panic!("i32 must be a generic owner"));

        let substitution = GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        )
        .unwrap_or_else(|error| panic!("i32 substitution must be valid: {error:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("i32 substitution must be interned: {error:?}"));

        values
            .intern_type(bray_symbols::TypeData::Named {
                definition: bray_symbols::NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap_or_else(|error| panic!("i32 type must be interned: {error:?}"))
    }

    fn integer_constant(
        compilation: &crate::Compilation,
        ty: bray_symbols::TypeId,
        value: u8,
    ) -> ConstantValueId {
        compilation
            .semantic_value_store()
            .and_then(|values| {
                values
                    .intern_constant_value(ConstantValueData::new(
                        ty,
                        ConstantValueKind::Integer(IntegerConstant::new(
                            IntegerSign::NonNegative,
                            [value],
                        )),
                    ))
                    .map_err(|_| crate::FactQueryError::InfrastructureFailure)
            })
            .unwrap_or_else(|error| panic!("constant value must be interned: {error:?}"))
    }

    fn integer_value(compilation: &crate::Compilation, value: ConstantValueId) -> u8 {
        let value = constant_value(compilation, value);

        let ConstantValueKind::Integer(integer) = value.kind() else {
            panic!("constant value must be an integer: {value:?}");
        };

        let [value] = integer.magnitude() else {
            panic!("test integer must fit in one byte");
        };

        *value
    }

    fn constant_value(
        compilation: &crate::Compilation,
        value: ConstantValueId,
    ) -> Arc<ConstantValueData> {
        compilation
            .semantic_value_store()
            .and_then(|values| {
                values
                    .constant_value_data(value)
                    .map_err(|_| crate::FactQueryError::InfrastructureFailure)
            })
            .unwrap_or_else(|error| panic!("constant value must be interned: {error:?}"))
    }
}
