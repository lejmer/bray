use std::sync::Arc;

use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundIterationSource, BoundStructuredExpressionKind,
    BoundUnit, BoundUnitKey, IterationSourceMode, SelectedIterationProtocolOperation,
    SelectedIterationSource, SelectedIterationTypes,
};
use bray_checker::{
    CandidateSelection, CheckedConstantTerms, DefaultSemanticSelector, IterationSourceCandidate,
    IterationSourceSelectionRequest, SemanticSelector, resolve_type_expression_template,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, BorrowKind, CallableDefinitionId, CallableInstanceData,
    ExternalDeclarationIdentity, ExternalSymbolKeyData, ImplementationCandidate,
    ImplementationInstanceData, ImplementationRequirementKey, ImplementationSymbolId,
    SymbolFactRequest, SymbolKeyData, TraitApplicationData, TraitCallableFulfillmentSymbolId,
    TraitCallableMemberSymbolId, TraitSymbolId, TraitTypeFulfillmentSymbolId,
    TraitTypeFulfillmentValueFact, TraitTypeMemberSymbolId, TypeData, TypeId,
};

use super::Compilation;
use super::binder::{CompilationBinderFacts, binder_fact_error};
use super::checker::{CompilationCheckerContext, checker_result};
use super::constant::empty_concrete_substitution;
use super::substitution::empty_substitution;
use super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, IterationSourceFactKey};

#[derive(Clone, Copy)]
struct IterationInput {
    expression: BoundExpressionId,
    source: BoundExpressionId,
    mode: IterationSourceMode,
    source_type: TypeId,
    subject_type: TypeId,
    protocol: bray_symbols::CompilerKnownIterationProtocol,
}

#[derive(Clone, Copy)]
struct ProtocolCandidate<'candidate> {
    requirement: ImplementationRequirementKey,
    candidate: &'candidate ImplementationCandidate,
    member: TraitCallableMemberSymbolId,
    fulfillment: TraitCallableFulfillmentSymbolId,
}

#[derive(Clone, Copy)]
struct ImplementationFulfillments<'symbols> {
    callables: &'symbols [TraitCallableFulfillmentSymbolId],
    types: &'symbols [TraitTypeFulfillmentSymbolId],
}

enum TypeValuedMemberResolution {
    Resolved(TypeId),
    Invalid,
    Deferred,
}

impl Compilation {
    /// Returns the exact selected protocol operations for one iteration source occurrence.
    pub fn iteration_source(
        &self,
        unit: BoundUnitKey,
        expression: BoundExpressionId,
    ) -> Result<Arc<DiagnosticResult<Option<SelectedIterationSource>>>, FactQueryError> {
        self.iteration_source_with_cancellation(unit, expression, &self.state.cancellation)
    }

    fn iteration_source_with_cancellation(
        &self,
        unit: BoundUnitKey,
        expression: BoundExpressionId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<SelectedIterationSource>>>, FactQueryError> {
        let key = IterationSourceFactKey::new(unit, expression);
        let cell = self.state.iteration_sources.cell(key.clone())?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::IterationSource(key.clone()),
            cancellation,
            || {
                self.compute_iteration_source(&key, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_iteration_source(
        &self,
        key: &IterationSourceFactKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<SelectedIterationSource>>, FactQueryError> {
        let bound = self.bound_unit_with_cancellation(key.unit().clone(), cancellation)?;
        let types = self.expression_types_with_cancellation(key.unit().clone(), cancellation)?;

        let mut diagnostics = bound
            .result()
            .diagnostics()
            .merged(types.result().diagnostics());

        let (source, mode) = iteration_source(bound.result().value(), key.expression())?;

        let source_type = types
            .result()
            .value()
            .expression(source)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if source_type.is_recovered() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let facts = self.binder_facts_for(key.unit(), cancellation)?;

        let protocol = self
            .available_compiler_known_symbols()
            .iteration_protocol()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let source_type = source_type.ty();
        let subject_type = iteration_subject_type(facts.semantic_values(), source_type, mode)?;

        let input = IterationInput {
            expression: key.expression(),
            source,
            mode,
            source_type,
            subject_type,
            protocol,
        };

        let (candidates, is_deferred) =
            self.iteration_candidates(&facts, input, cancellation, &mut diagnostics)?;

        if is_deferred {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let context = self.checker_context_for(key.unit(), cancellation)?;

        let selection = select_iteration(
            bound.result().value(),
            key.expression(),
            source,
            mode,
            candidates,
            &context,
        )?;

        let (selection, selection_diagnostics) = selection.into_parts();

        diagnostics.add_range(selection_diagnostics);

        let selected = match selection {
            CandidateSelection::Selected(selection) => Some(selection),
            CandidateSelection::Failed(_) => None,
        };

        Ok(DiagnosticResult::new(selected, diagnostics))
    }

    fn iteration_candidates(
        &self,
        facts: &CompilationBinderFacts<'_>,
        input: IterationInput,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(Vec<IterationSourceCandidate>, bool), FactQueryError> {
        let iterable_requirement = implementation_requirement(
            facts.semantic_values(),
            input.subject_type,
            input.protocol.iterable_trait(),
        )?;

        let iterable_candidates = self.implementation_candidate_set_result_with_cancellation(
            iterable_requirement,
            cancellation,
        )?;

        diagnostics.add_range(iterable_candidates.diagnostics().clone());

        let mut candidates = Vec::new();
        let mut is_deferred = false;

        for iterable in iterable_candidates.value().candidates() {
            if !iterable.constraints().is_empty() {
                // TODO(BRA-233): Select constrained implementations from checked predicate facts.
                is_deferred = true;

                continue;
            }

            if !self.target_dependencies_hold(iterable, cancellation, diagnostics)? {
                continue;
            }

            let iterable_fulfillments =
                implementation_fulfillments(facts, iterable.implementation())?;

            let cursor_type = match selected_type_valued_member(
                facts,
                iterable,
                iterable_fulfillments.types,
                input.protocol.iterable_cursor(),
                diagnostics,
            )? {
                TypeValuedMemberResolution::Resolved(ty) => ty,
                TypeValuedMemberResolution::Invalid => continue,
                TypeValuedMemberResolution::Deferred => {
                    is_deferred = true;

                    continue;
                }
            };

            let element_type = match selected_type_valued_member(
                facts,
                iterable,
                iterable_fulfillments.types,
                input.protocol.iterable_element(),
                diagnostics,
            )? {
                TypeValuedMemberResolution::Resolved(ty) => ty,
                TypeValuedMemberResolution::Invalid => continue,
                TypeValuedMemberResolution::Deferred => {
                    is_deferred = true;

                    continue;
                }
            };

            let Some(iterate) = selected_callable(
                facts,
                iterable_fulfillments.callables,
                input.protocol.iterable_iterate(),
            ) else {
                continue;
            };

            let iterator_requirement = implementation_requirement(
                facts.semantic_values(),
                cursor_type,
                input.protocol.iterator_trait(),
            )?;

            let iterator_candidates = self.implementation_candidate_set_result_with_cancellation(
                iterator_requirement,
                cancellation,
            )?;

            diagnostics.add_range(iterator_candidates.diagnostics().clone());

            for iterator in iterator_candidates.value().candidates() {
                if !iterator.constraints().is_empty() {
                    // TODO(BRA-233): Select constrained implementations from checked predicate facts.
                    is_deferred = true;

                    continue;
                }

                if !self.target_dependencies_hold(iterator, cancellation, diagnostics)? {
                    continue;
                }

                let iterator_fulfillments =
                    implementation_fulfillments(facts, iterator.implementation())?;

                let iterator_element = match selected_type_valued_member(
                    facts,
                    iterator,
                    iterator_fulfillments.types,
                    input.protocol.iterator_element(),
                    diagnostics,
                )? {
                    TypeValuedMemberResolution::Resolved(ty) => ty,
                    TypeValuedMemberResolution::Invalid => continue,
                    TypeValuedMemberResolution::Deferred => {
                        is_deferred = true;

                        continue;
                    }
                };

                if iterator_element != element_type {
                    continue;
                }

                let Some(next) = selected_callable(
                    facts,
                    iterator_fulfillments.callables,
                    input.protocol.iterator_next(),
                ) else {
                    continue;
                };

                candidates.push(iteration_candidate(
                    facts,
                    input,
                    cursor_type,
                    element_type,
                    ProtocolCandidate {
                        requirement: iterable_requirement,
                        candidate: iterable,
                        member: input.protocol.iterable_iterate(),
                        fulfillment: iterate,
                    },
                    ProtocolCandidate {
                        requirement: iterator_requirement,
                        candidate: iterator,
                        member: input.protocol.iterator_next(),
                        fulfillment: next,
                    },
                )?);
            }
        }

        Ok((candidates, is_deferred))
    }

    fn target_dependencies_hold(
        &self,
        candidate: &ImplementationCandidate,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        for dependency in candidate.target_dependencies() {
            let definition = AnyConstantDefinitionId::Constant(dependency.fact());

            let substitution =
                empty_concrete_substitution(self.semantic_value_store()?, definition)?;

            let instance = bray_symbols::ConstantInstanceKey::new(definition, substitution, None);
            let result = self.constant_instance_with_cancellation(instance, cancellation)?;

            diagnostics.add_range(result.diagnostics().clone());

            if result.value() != &dependency.value() {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

fn iteration_subject_type(
    values: &bray_symbols::SemanticValueStore,
    source_type: TypeId,
    mode: IterationSourceMode,
) -> Result<TypeId, FactQueryError> {
    let borrow_kind = match mode {
        IterationSourceMode::Shared => Some(BorrowKind::Shared),
        IterationSourceMode::Mutable => Some(BorrowKind::Mutable),
        IterationSourceMode::Move => None,
    };

    let Some(kind) = borrow_kind else {
        return Ok(source_type);
    };

    values
        .intern_type(TypeData::Borrow {
            kind,
            target: source_type,
        })
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

fn iteration_source(
    unit: &BoundUnit,
    expression: BoundExpressionId,
) -> Result<(BoundExpressionId, IterationSourceMode), FactQueryError> {
    let expression = unit
        .view()
        .expression(expression)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    match expression {
        BoundExpression::For(expression) => Ok((expression.source(), expression.source_mode())),
        BoundExpression::Generator(expression) => {
            Ok((expression.source(), expression.source_mode()))
        }
        BoundExpression::Structured(expression)
            if expression.kind() == BoundStructuredExpressionKind::BooleanFold =>
        {
            let [source] = expression.operands() else {
                return Err(FactQueryError::InfrastructureFailure);
            };

            Ok((*source, IterationSourceMode::Shared))
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

fn implementation_requirement(
    values: &bray_symbols::SemanticValueStore,
    subject: TypeId,
    definition: TraitSymbolId,
) -> Result<ImplementationRequirementKey, FactQueryError> {
    let substitution = empty_substitution(values, definition.into())?;

    let application = values
        .intern_trait_application(TraitApplicationData::new(definition, substitution))
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    Ok(ImplementationRequirementKey::new(subject, application))
}

fn selected_type_valued_member(
    facts: &CompilationBinderFacts<'_>,
    candidate: &ImplementationCandidate,
    fulfillments: &[TraitTypeFulfillmentSymbolId],
    member: TraitTypeMemberSymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<TypeValuedMemberResolution, FactQueryError> {
    let expected_name = facts
        .symbols()
        .member_name(member.into())
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let mut matching = fulfillments.iter().copied().filter(|fulfillment| {
        fulfillment_name(facts, (*fulfillment).into())
            .is_some_and(|name| name == expected_name.as_str())
    });

    let Some(fulfillment) = matching.next() else {
        return Ok(TypeValuedMemberResolution::Invalid);
    };

    if matching.next().is_some() {
        return Ok(TypeValuedMemberResolution::Invalid);
    }

    let result = facts
        .symbol_fact(SymbolFactRequest::<TraitTypeFulfillmentValueFact>::new(
            fulfillment,
        ))
        .map_err(binder_fact_error)?;

    diagnostics.add_range(result.diagnostics().clone());

    if result.diagnostics().has_errors() {
        return Ok(TypeValuedMemberResolution::Invalid);
    }

    // TODO(BRA-246): Supply checked embedded constant terms for source type templates.
    let Some(ty) = resolve_type_expression_template(
        facts.semantic_values(),
        result.value(),
        &CheckedConstantTerms::new(),
    )
    .map_err(FactQueryError::CheckerInfrastructure)?
    else {
        return Ok(TypeValuedMemberResolution::Deferred);
    };

    facts
        .semantic_values()
        .substitute_type(ty, candidate.substitution())
        .map(TypeValuedMemberResolution::Resolved)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

fn implementation_fulfillments<'facts>(
    facts: &'facts CompilationBinderFacts<'_>,
    implementation: ImplementationSymbolId,
) -> Result<ImplementationFulfillments<'facts>, FactQueryError> {
    let source =
        match implementation {
            ImplementationSymbolId::Inherent(id) => facts
                .symbols()
                .inherent_implementation(id)
                .map(|symbol| ImplementationFulfillments {
                    callables: symbol.callable_fulfillments(),
                    types: symbol.type_fulfillments(),
                }),
            ImplementationSymbolId::UnnamedTrait(id) => facts
                .symbols()
                .unnamed_trait_implementation(id)
                .map(|symbol| ImplementationFulfillments {
                    callables: symbol.callable_fulfillments(),
                    types: symbol.type_fulfillments(),
                }),
            ImplementationSymbolId::NamedTrait(id) => facts
                .symbols()
                .named_trait_implementation(id)
                .map(|symbol| ImplementationFulfillments {
                    callables: symbol.callable_fulfillments(),
                    types: symbol.type_fulfillments(),
                }),
        };

    if let Some(fulfillments) = source {
        return Ok(fulfillments);
    }

    let imported = facts.imported_symbols().map_err(binder_fact_error)?;

    let imported = match implementation {
        ImplementationSymbolId::Inherent(id) => imported
            .and_then(|symbols| symbols.inherent_implementation(id))
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
        ImplementationSymbolId::UnnamedTrait(id) => imported
            .and_then(|symbols| symbols.unnamed_trait_implementation(id))
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
        ImplementationSymbolId::NamedTrait(id) => imported
            .and_then(|symbols| symbols.named_trait_implementation(id))
            .map(|symbol| ImplementationFulfillments {
                callables: symbol.callable_fulfillments(),
                types: symbol.type_fulfillments(),
            }),
    };

    imported.ok_or(FactQueryError::InfrastructureFailure)
}

fn fulfillment_name<'facts>(
    facts: &'facts CompilationBinderFacts<'_>,
    fulfillment: AnySymbolId,
) -> Option<&'facts str> {
    if let Some(name) = facts.symbols().member_name(fulfillment) {
        return Some(name.as_str());
    }

    let key = facts.symbol_key(fulfillment).ok()??;

    let SymbolKeyData::External(key) = key.data() else {
        return None;
    };

    let ExternalSymbolKeyData::Declaration {
        identity: ExternalDeclarationIdentity::Name(name),
        ..
    } = key.data()
    else {
        return None;
    };

    Some(name.as_str())
}

fn selected_callable(
    facts: &CompilationBinderFacts<'_>,
    fulfillments: &[TraitCallableFulfillmentSymbolId],
    member: TraitCallableMemberSymbolId,
) -> Option<TraitCallableFulfillmentSymbolId> {
    let expected_name = facts.symbols().member_name(member.into())?;

    let mut matching = fulfillments.iter().copied().filter(|fulfillment| {
        fulfillment_name(facts, (*fulfillment).into())
            .is_some_and(|name| name == expected_name.as_str())
    });

    let fulfillment = matching.next()?;

    if matching.next().is_some() {
        return None;
    }

    Some(fulfillment)
}

fn callable_instance(
    values: &bray_symbols::SemanticValueStore,
    callable: AnySymbolId,
) -> Result<CallableInstanceData, FactQueryError> {
    let definition =
        CallableDefinitionId::try_new(callable).ok_or(FactQueryError::InfrastructureFailure)?;

    let substitution = empty_substitution(values, callable)?;

    Ok(CallableInstanceData::new(definition, substitution))
}

fn iteration_candidate(
    facts: &CompilationBinderFacts<'_>,
    input: IterationInput,
    cursor_type: TypeId,
    element_type: TypeId,
    iterable: ProtocolCandidate<'_>,
    iterator: ProtocolCandidate<'_>,
) -> Result<IterationSourceCandidate, FactQueryError> {
    let values = facts.semantic_values();

    let iterable_witness = values
        .intern_implementation_instance(ImplementationInstanceData::new(
            iterable.candidate.implementation(),
            iterable.candidate.substitution(),
        ))
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let iterator_witness = values
        .intern_implementation_instance(ImplementationInstanceData::new(
            iterator.candidate.implementation(),
            iterator.candidate.substitution(),
        ))
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let iterate_member = callable_instance(values, iterable.member.into())?;
    let iterate_fulfillment = callable_instance(values, iterable.fulfillment.into())?;
    let next_member = callable_instance(values, iterator.member.into())?;
    let next_fulfillment = callable_instance(values, iterator.fulfillment.into())?;

    let selection = SelectedIterationSource::new(
        input.expression,
        BoundIterationSource::new(input.source, input.mode),
        SelectedIterationTypes::new(input.source_type, cursor_type, element_type),
        SelectedIterationProtocolOperation::new(
            iterable.requirement,
            iterable_witness,
            iterate_member,
            iterate_fulfillment,
        ),
        SelectedIterationProtocolOperation::new(
            iterator.requirement,
            iterator_witness,
            next_member,
            next_fulfillment,
        ),
    );

    Ok(IterationSourceCandidate::new(
        iterable.candidate.key().clone(),
        iterator.candidate.key().clone(),
        selection,
    ))
}

fn select_iteration(
    bound: &BoundUnit,
    expression: BoundExpressionId,
    source: BoundExpressionId,
    mode: IterationSourceMode,
    candidates: Vec<IterationSourceCandidate>,
    context: &CompilationCheckerContext<'_>,
) -> Result<DiagnosticResult<CandidateSelection<SelectedIterationSource>>, FactQueryError> {
    let semantic_context = semantic_unit_context_for(context.symbols(), bound)?;

    let unit =
        bray_checker::CheckerUnitView::new(bound, &semantic_context, context).map_err(|error| {
            FactQueryError::CheckerInfrastructure(
                bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
            )
        })?;

    let request = IterationSourceSelectionRequest::new(expression, source, mode, candidates);

    checker_result(DefaultSemanticSelector.select_iteration_source(unit, &request))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{BinderFactContext, SymbolFactProvider};
    use bray_bound_tree::{
        AnyBoundNodeId, BoundExpression, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome,
        IterationSourceMode, walk_bound_unit_view,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticSelectionKind};
    use bray_symbols::{
        BorrowKind, ImplementationCoherenceFact, ImplementationSymbolId, NamedTypeSymbolId,
        SemanticValueStore, StructSymbolId, SymbolFactRequest, SymbolKind, SymbolOrigin, TypeData,
    };

    use crate::CancellationToken;
    use crate::test_support::{compilation, source_callable_body_key};

    use super::{
        IterationInput, empty_substitution, iteration_source, iteration_subject_type,
        select_iteration,
    };

    #[test]
    fn iteration_subject_types_follow_the_selected_access_mode() {
        let values = match SemanticValueStore::try_new() {
            Ok(values) => values,
            Err(error) => panic!("semantic values must be available: {error:?}"),
        };

        let source = match values.intern_type(TypeData::Error) {
            Ok(source) => source,
            Err(error) => panic!("source type must be internable: {error:?}"),
        };

        let shared = match iteration_subject_type(&values, source, IterationSourceMode::Shared) {
            Ok(subject) => subject,
            Err(error) => panic!("shared subject must be available: {error:?}"),
        };

        let mutable = match iteration_subject_type(&values, source, IterationSourceMode::Mutable) {
            Ok(subject) => subject,
            Err(error) => panic!("mutable subject must be available: {error:?}"),
        };

        assert_eq!(
            values.type_data(shared).as_deref(),
            Ok(&TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: source,
            })
        );

        assert_eq!(
            values.type_data(mutable).as_deref(),
            Ok(&TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: source,
            })
        );

        assert_eq!(
            iteration_subject_type(&values, source, IterationSourceMode::Move),
            Ok(source)
        );
    }

    #[test]
    fn iteration_source_queries_are_narrow_cached_and_diagnostic_backed() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let items: i32 = 1;\n",
            "    for item in items\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);
        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let iteration = iteration_expression(bound.value());

        let first = match compilation.iteration_source(key.clone(), iteration) {
            Ok(result) => result,
            Err(error) => panic!("iteration selection must publish: {error:?}"),
        };

        let second = match compilation.iteration_source(key, iteration) {
            Ok(result) => result,
            Err(error) => panic!("iteration selection must be reusable: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(*first.value(), None);

        let diagnostic = first
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingNoApplicableCandidate);

        let Some(diagnostic) = diagnostic else {
            panic!(
                "iteration diagnostics must report unavailable protocols: {:?}",
                first.diagnostics()
            );
        };

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::selection_kind(
                DiagnosticSelectionKind::IterationSource
            )]
        );
    }

    #[test]
    fn iteration_candidates_select_exact_protocol_fulfillments() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "struct Items\n",
            "{\n",
            "}\n",
            "\n",
            "struct ItemsCursor\n",
            "{\n",
            "}\n",
            "\n",
            "impl &Items(Iterable)\n",
            "{\n",
            "    type Element = i32;\n",
            "    type Cursor = ItemsCursor;\n",
            "\n",
            "    consume func iterate() -> ItemsCursor\n",
            "    {\n",
            "    }\n",
            "}\n",
            "\n",
            "impl ItemsCursor(Iterator)\n",
            "{\n",
            "    type Element = i32;\n",
            "\n",
            "    mut func next() -> i32?\n",
            "    {\n",
            "    }\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    for item in 1\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);
        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let iteration = iteration_expression(bound.value());

        let (source, mode) = match iteration_source(bound.value(), iteration) {
            Ok(source) => source,
            Err(error) => panic!("iteration source must be available: {error:?}"),
        };

        let cancellation = CancellationToken::new();
        let facts = match compilation.binder_facts_for(&key, &cancellation) {
            Ok(facts) => facts,
            Err(error) => panic!("binder facts must be available: {error:?}"),
        };

        let structures = facts
            .symbols()
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [items, cursor] = structures.as_slice() else {
            panic!("test source must declare Items and ItemsCursor");
        };

        let source_type = named_type(&facts, items.id());
        let cursor_type = named_type(&facts, cursor.id());

        let integer = facts
            .symbols()
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(RepresentationRole::ScalarI32)
            .unwrap_or_else(|| panic!("compiler-known i32 must be available"));

        let element_type = named_type(&facts, integer);

        let subject_type = iteration_subject_type(facts.semantic_values(), source_type, mode)
            .unwrap_or_else(|error| panic!("iteration subject must be available: {error:?}"));

        let protocol = compilation
            .available_compiler_known_symbols()
            .iteration_protocol()
            .unwrap_or_else(|| panic!("iteration protocol must be available"));

        let implementation = facts
            .symbols()
            .unnamed_trait_implementations()
            .iter()
            .find(|implementation| implementation.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must declare an Iterable implementation"));

        let coherence = facts
            .symbol_fact(SymbolFactRequest::<ImplementationCoherenceFact>::new(
                ImplementationSymbolId::UnnamedTrait(implementation.id()),
            ))
            .unwrap_or_else(|error| panic!("implementation coherence must publish: {error:?}"));

        assert!(
            coherence.diagnostics().is_empty(),
            "{:?}",
            coherence.diagnostics()
        );
        assert_eq!(coherence.value().subject(), subject_type);

        let application = coherence
            .value()
            .trait_application()
            .unwrap_or_else(|| panic!("Iterable implementation must retain its trait"));

        let application = facts
            .semantic_values()
            .trait_application_data(application)
            .unwrap_or_else(|error| panic!("Iterable application must be available: {error:?}"));

        assert_eq!(application.definition(), protocol.iterable_trait());

        let mut diagnostics = DiagnosticBag::new();

        let candidates = compilation
            .iteration_candidates(
                &facts,
                IterationInput {
                    expression: iteration,
                    source,
                    mode,
                    source_type,
                    subject_type,
                    protocol,
                },
                &cancellation,
                &mut diagnostics,
            )
            .unwrap_or_else(|error| panic!("iteration candidates must be available: {error:?}"));

        assert!(!candidates.1);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        let context = compilation
            .checker_context_for(&key, &cancellation)
            .unwrap_or_else(|error| panic!("checker context must be available: {error:?}"));

        let selected = select_iteration(
            bound.value(),
            iteration,
            source,
            mode,
            candidates.0,
            &context,
        )
        .unwrap_or_else(|error| panic!("iteration selection must complete: {error:?}"));

        assert!(
            selected.diagnostics().is_empty(),
            "{:?}",
            selected.diagnostics()
        );

        let bray_checker::CandidateSelection::Selected(selected) = selected.value() else {
            panic!("valid iteration protocols must be selected");
        };

        assert_eq!(selected.mode(), IterationSourceMode::Shared);
        assert_eq!(selected.source_type(), source_type);
        assert_eq!(selected.cursor_type(), cursor_type);
        assert_eq!(selected.element_type(), element_type);
        assert_eq!(selected.iterable_requirement().subject(), subject_type);
        assert_eq!(selected.iterator_requirement().subject(), cursor_type);

        let iterable_witness = facts
            .semantic_values()
            .implementation_instance_data(selected.iterable_witness())
            .unwrap_or_else(|error| panic!("Iterable witness must be available: {error:?}"));

        let iterator_witness = facts
            .semantic_values()
            .implementation_instance_data(selected.iterator_witness())
            .unwrap_or_else(|error| panic!("Iterator witness must be available: {error:?}"));

        assert_ne!(iterable_witness.definition(), iterator_witness.definition());
        assert_eq!(
            selected.iterate_member().definition().symbol().kind(),
            SymbolKind::TraitCallableMember
        );
        assert_eq!(
            selected.iterate().definition().symbol().kind(),
            SymbolKind::TraitCallableFulfillment
        );
        assert_eq!(
            selected.next_member().definition().symbol().kind(),
            SymbolKind::TraitCallableMember
        );
        assert_eq!(
            selected.next().definition().symbol().kind(),
            SymbolKind::TraitCallableFulfillment
        );
    }

    fn iteration_expression(
        unit: &bray_bound_tree::BoundUnit,
    ) -> bray_bound_tree::BoundExpressionId {
        let mut iteration = None;

        let outcome = walk_bound_unit_view(unit.view(), unit.root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            if matches!(
                unit.view().expression(expression),
                Some(BoundExpression::For(_))
            ) {
                iteration = Some(expression);

                return BoundWalkControl::Stop;
            }

            BoundWalkControl::Continue
        });

        assert_eq!(outcome, BoundWalkOutcome::Stopped);

        match iteration {
            Some(iteration) => iteration,
            None => panic!("test source must bind one for expression"),
        }
    }

    fn named_type(
        facts: &super::CompilationBinderFacts<'_>,
        definition: StructSymbolId,
    ) -> bray_symbols::TypeId {
        let substitution = empty_substitution(facts.semantic_values(), definition.into())
            .unwrap_or_else(|error| panic!("type substitution must be interned: {error:?}"));

        facts
            .semantic_values()
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap_or_else(|error| panic!("named type must be interned: {error:?}"))
    }
}
