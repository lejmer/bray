use std::sync::Arc;

use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundIterationSource, BoundOperator,
    BoundStructuredExpressionKind, BoundUnit, BoundUnitKey, CheckedLiteralValues,
    IterationSourceMode, SelectedIterationProtocolOperation, SelectedIterationSource,
    SelectedIterationTypes,
};
use bray_checker::{
    CandidateSelection, DefaultSemanticSelector, IterationSourceCandidate,
    IterationSourceSelectionRequest, SemanticSelector,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    BorrowKind, ConstantTermData, ConstantValueKind, ImplementationCandidate,
    ImplementationInstanceData, ImplementationRequirementKey, IntegerConstant, IntegerSign,
    TargetSizedIntegerType, TraitCallableMemberSymbolId, TypeData, TypeId,
};

use super::Compilation;
use super::binder::CompilationBindingContext;
use super::checker::{CompilationCheckerContext, checker_result};
use super::implementation::{
    TypeValuedMemberResolution, callable_instance, implementation_callable_instance,
    implementation_fulfillments, implementation_requirement, selected_type_valued_member,
};
use super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, IterationSourceQueryKey};

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

    pub(in crate::compilation) fn iteration_source_with_cancellation(
        &self,
        unit: BoundUnitKey,
        expression: BoundExpressionId,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<SelectedIterationSource>>>, FactQueryError> {
        let key = IterationSourceQueryKey::new(unit, expression);
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
        key: &IterationSourceQueryKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<SelectedIterationSource>>, FactQueryError> {
        let bound = self.bound_unit_with_cancellation(key.unit().clone(), cancellation)?;

        let semantics = self
            .provisional_expression_semantics_with_cancellation(key.unit().clone(), cancellation)?;

        let types = semantics.result().value().types();

        let mut diagnostics = bound
            .result()
            .diagnostics()
            .merged(semantics.result().diagnostics());

        let (source, mode) = iteration_source(bound.result().value(), key.expression())?;

        let source_type = types
            .expression(source)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if source_type.is_recovered() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let binding_context = self.binding_context_for(key.unit(), cancellation)?;

        let protocol = self
            .available_compiler_known_symbols()
            .iteration_protocol()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let source_type = source_type.ty();

        let subject_type =
            iteration_subject_type(binding_context.semantic_values(), source_type, mode)?;

        let input = IterationInput {
            expression: key.expression(),
            source,
            mode,
            source_type,
            subject_type,
            protocol,
        };

        let (candidates, is_deferred) =
            self.iteration_candidates(&binding_context, input, cancellation, &mut diagnostics)?;

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
            CandidateSelection::Selected(selection) => {
                let exact_count = iteration_exact_count(
                    bound.result().value(),
                    semantics.result().value().literals(),
                    binding_context.semantic_values(),
                    selection.source(),
                    selection.source_type(),
                    self.selected_target().target().integer_width_bits().get(),
                )?;

                Some(match exact_count {
                    Some(exact_count) => selection.with_exact_count(exact_count),
                    None => selection,
                })
            }
            CandidateSelection::Failed(_) => None,
        };

        Ok(DiagnosticResult::new(selected, diagnostics))
    }

    fn iteration_candidates(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        input: IterationInput,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(Vec<IterationSourceCandidate>, bool), FactQueryError> {
        let iterable_requirement = implementation_requirement(
            binding_context.semantic_values(),
            input.subject_type,
            input.protocol.iterable_trait(),
            [],
            [],
        )?;

        let iterable_candidates = self.implementation_candidate_set_result_with_cancellation(
            iterable_requirement,
            cancellation,
        )?;

        diagnostics.add_range(iterable_candidates.diagnostics().clone());

        let mut candidates = Vec::new();
        let mut is_deferred = false;

        for iterable in iterable_candidates.value().candidates() {
            match self.implementation_candidate_constraint_outcome(
                iterable,
                cancellation,
                diagnostics,
            )? {
                bray_symbols::ProofOutcome::Proven => {}
                bray_symbols::ProofOutcome::Disproven | bray_symbols::ProofOutcome::Recovered => {
                    continue;
                }
                bray_symbols::ProofOutcome::Unknown => {
                    is_deferred = true;

                    continue;
                }
            }

            let iterable_fulfillments =
                implementation_fulfillments(binding_context, iterable.implementation())?;

            let cursor_type = match selected_type_valued_member(
                binding_context,
                iterable.substitution(),
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
                binding_context,
                iterable.substitution(),
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

            let iterator_requirement = implementation_requirement(
                binding_context.semantic_values(),
                cursor_type,
                input.protocol.iterator_trait(),
                [],
                [],
            )?;

            let iterator_candidates = self.implementation_candidate_set_result_with_cancellation(
                iterator_requirement,
                cancellation,
            )?;

            diagnostics.add_range(iterator_candidates.diagnostics().clone());

            for iterator in iterator_candidates.value().candidates() {
                match self.implementation_candidate_constraint_outcome(
                    iterator,
                    cancellation,
                    diagnostics,
                )? {
                    bray_symbols::ProofOutcome::Proven => {}
                    bray_symbols::ProofOutcome::Disproven
                    | bray_symbols::ProofOutcome::Recovered => continue,
                    bray_symbols::ProofOutcome::Unknown => {
                        is_deferred = true;

                        continue;
                    }
                }

                let iterator_fulfillments =
                    implementation_fulfillments(binding_context, iterator.implementation())?;

                let iterator_element = match selected_type_valued_member(
                    binding_context,
                    iterator.substitution(),
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

                let candidate = iteration_candidate(
                    binding_context,
                    input,
                    cursor_type,
                    element_type,
                    ProtocolCandidate {
                        requirement: iterable_requirement,
                        candidate: iterable,
                        member: input.protocol.iterable_iterate(),
                    },
                    ProtocolCandidate {
                        requirement: iterator_requirement,
                        candidate: iterator,
                        member: input.protocol.iterator_next(),
                    },
                )?;

                if let Some(candidate) = candidate {
                    candidates.push(candidate);
                }
            }
        }

        Ok((candidates, is_deferred))
    }
}

fn iteration_exact_count(
    unit: &BoundUnit,
    literals: &CheckedLiteralValues,
    values: &bray_symbols::SemanticValueStore,
    source_expression: BoundExpressionId,
    source_type: TypeId,
    usize_width_bits: u16,
) -> Result<Option<bray_symbols::ConstantTermId>, FactQueryError> {
    let source = values
        .type_data(source_type)
        .map_err(FactQueryError::SemanticValueStore)?;

    match source.as_ref() {
        TypeData::Array { length, .. } => Ok(Some(*length)),
        TypeData::Named { .. } => {
            range_literal_count(unit, literals, values, source_expression, usize_width_bits)
        }
        _ => Ok(None),
    }
}

fn range_literal_count(
    unit: &BoundUnit,
    literals: &CheckedLiteralValues,
    values: &bray_symbols::SemanticValueStore,
    source: BoundExpressionId,
    usize_width_bits: u16,
) -> Result<Option<bray_symbols::ConstantTermId>, FactQueryError> {
    let Some(BoundExpression::Structured(range)) = unit.view().expression(source) else {
        return Ok(None);
    };

    if range.kind() != BoundStructuredExpressionKind::Range {
        return Ok(None);
    }

    let [start, end] = range.operands() else {
        return Ok(None);
    };

    let Some(start) = range_literal_integer(unit, literals, values, *start)? else {
        return Ok(None);
    };

    let Some(end) = range_literal_integer(unit, literals, values, *end)? else {
        return Ok(None);
    };

    let Some(count) = half_open_integer_count(&start, &end) else {
        return Ok(None);
    };

    if !count_fits_target_usize(count, usize_width_bits) {
        return Ok(None);
    }

    values
        .intern_constant_term(ConstantTermData::IntegerLiteral {
            ty: TargetSizedIntegerType::Usize,
            value: IntegerConstant::new(IntegerSign::NonNegative, count.to_be_bytes()),
        })
        .map(Some)
        .map_err(FactQueryError::SemanticValueStore)
}

fn range_literal_integer(
    unit: &BoundUnit,
    literals: &CheckedLiteralValues,
    values: &bray_symbols::SemanticValueStore,
    expression: BoundExpressionId,
) -> Result<Option<IntegerConstant>, FactQueryError> {
    if let Some(value) = literals.expression(expression) {
        let value = values
            .constant_value_data(value)
            .map_err(FactQueryError::SemanticValueStore)?;

        return Ok(match value.kind() {
            ConstantValueKind::Integer(integer) => Some(integer.clone()),
            _ => None,
        });
    }

    let Some(BoundExpression::Unary(unary)) = unit.view().expression(expression) else {
        return Ok(None);
    };

    let [operand] = unary.operands() else {
        return Ok(None);
    };

    let Some(integer) = range_literal_integer(unit, literals, values, *operand)? else {
        return Ok(None);
    };

    match unary.operator() {
        BoundOperator::Add => Ok(Some(integer)),
        BoundOperator::Subtract => {
            let sign = match integer.sign() {
                IntegerSign::NonNegative => IntegerSign::Negative,
                IntegerSign::Negative => IntegerSign::NonNegative,
            };

            Ok(Some(IntegerConstant::new(
                sign,
                integer.magnitude().iter().copied(),
            )))
        }
        _ => Ok(None),
    }
}

fn half_open_integer_count(start: &IntegerConstant, end: &IntegerConstant) -> Option<u128> {
    let magnitude = |integer: &IntegerConstant| {
        integer.magnitude().iter().try_fold(0_u128, |value, byte| {
            value.checked_mul(256)?.checked_add(u128::from(*byte))
        })
    };

    let start_magnitude = magnitude(start)?;
    let end_magnitude = magnitude(end)?;

    let count = match (start.sign(), end.sign()) {
        (IntegerSign::NonNegative, IntegerSign::NonNegative) => {
            end_magnitude.saturating_sub(start_magnitude)
        }
        (IntegerSign::Negative, IntegerSign::Negative) => {
            start_magnitude.saturating_sub(end_magnitude)
        }
        (IntegerSign::Negative, IntegerSign::NonNegative) => {
            start_magnitude.checked_add(end_magnitude)?
        }
        (IntegerSign::NonNegative, IntegerSign::Negative) => 0,
    };

    Some(count)
}

fn count_fits_target_usize(count: u128, usize_width_bits: u16) -> bool {
    usize_width_bits >= u128::BITS as u16 || count < (1_u128 << usize_width_bits)
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
        .map_err(FactQueryError::SemanticValueStore)
}

fn iteration_source(
    unit: &BoundUnit,
    expression: BoundExpressionId,
) -> Result<(BoundExpressionId, IterationSourceMode), FactQueryError> {
    unit.view()
        .expression(expression)
        .and_then(BoundExpression::iteration_source)
        .ok_or(FactQueryError::InfrastructureFailure)
}

fn iteration_candidate(
    binding_context: &CompilationBindingContext<'_>,
    input: IterationInput,
    cursor_type: TypeId,
    element_type: TypeId,
    iterable: ProtocolCandidate<'_>,
    iterator: ProtocolCandidate<'_>,
) -> Result<Option<IterationSourceCandidate>, FactQueryError> {
    let values = binding_context.semantic_values();

    let iterable_witness = values
        .intern_implementation_instance(ImplementationInstanceData::new(
            iterable.candidate.implementation(),
            iterable.candidate.substitution(),
        ))
        .map_err(FactQueryError::SemanticValueStore)?;

    let iterator_witness = values
        .intern_implementation_instance(ImplementationInstanceData::new(
            iterator.candidate.implementation(),
            iterator.candidate.substitution(),
        ))
        .map_err(FactQueryError::SemanticValueStore)?;

    let iterable_application = values
        .trait_application_data(iterable.requirement.trait_application())
        .map_err(FactQueryError::SemanticValueStore)?;

    let iterator_application = values
        .trait_application_data(iterator.requirement.trait_application())
        .map_err(FactQueryError::SemanticValueStore)?;

    let iterate_member = callable_instance(
        values,
        iterable.member.into(),
        [iterable_application.substitution()],
    )?;

    let iterable_fulfillments =
        implementation_fulfillments(binding_context, iterable.candidate.implementation())?;

    let Some(iterate_fulfillment) = implementation_callable_instance(
        binding_context,
        iterable_fulfillments.callables,
        iterable.member,
        iterable_application.substitution(),
        iterable.candidate.substitution(),
    )?
    else {
        return Ok(None);
    };

    let iterate_fulfillment = iterate_fulfillment.instance();

    let next_member = callable_instance(
        values,
        iterator.member.into(),
        [iterator_application.substitution()],
    )?;

    let iterator_fulfillments =
        implementation_fulfillments(binding_context, iterator.candidate.implementation())?;

    let Some(next_fulfillment) = implementation_callable_instance(
        binding_context,
        iterator_fulfillments.callables,
        iterator.member,
        iterator_application.substitution(),
        iterator.candidate.substitution(),
    )?
    else {
        return Ok(None);
    };

    let next_fulfillment = next_fulfillment.instance();

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

    Ok(Some(IterationSourceCandidate::new(
        iterable.candidate.key().clone(),
        iterator.candidate.key().clone(),
        selection,
    )))
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

    let unit = super::unit::checker_unit_view(bound, &semantic_context, context)?;

    let request = IterationSourceSelectionRequest::new(expression, source, mode, candidates);

    checker_result(DefaultSemanticSelector.select_iteration_source(unit, &request))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{BindingQueryContext, SymbolQueryProvider};
    use bray_bound_tree::{
        AnyBoundNodeId, BoundExpression, BoundStructuredExpressionKind, BoundWalkControl,
        BoundWalkEvent, BoundWalkOutcome, IterationSourceMode, SelectedIterationSource,
        SemanticSelection, walk_bound_unit_view,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticSelectionKind};
    use bray_symbols::{
        BorrowKind, ConstantTermData, ImplementationCoherenceQuery, ImplementationSymbolId,
        NamedTypeSymbolId, SemanticValueStore, StructSymbolId, SymbolKind, SymbolOrigin,
        SymbolQueryRequest, TypeData,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use crate::CancellationToken;
    use crate::test_support::{compilation, source_callable_body_key};

    use super::{IterationInput, iteration_source, iteration_subject_type, select_iteration};
    use crate::compilation::substitution::empty_substitution;

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

        assert_goal_state_diagnostic_kind(
            first.diagnostics(),
            DiagnosticKind::CheckingNoApplicableCandidate,
        );
    }

    #[test]
    fn range_iteration_selects_i32_protocols_and_exact_literal_cardinality() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    for value in 0..4\n",
            "    {\n",
            "    }\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let key = source_callable_body_key(&compilation);

        let bound = compilation
            .bound_unit(key.clone())
            .unwrap_or_else(|error| panic!("range unit must bind: {error:?}"));

        let range = find_expression(bound.value(), |expression| {
            matches!(
                expression,
                BoundExpression::Structured(expression)
                    if expression.kind() == BoundStructuredExpressionKind::Range
            )
        });

        let types = compilation
            .expression_types(key.clone())
            .unwrap_or_else(|error| panic!("range types must publish: {error:?}"));

        let range_type = types
            .value()
            .expression(range)
            .map(|result| result.ty())
            .unwrap_or_else(|| panic!("range expression must have a type"));

        let element = compilation
            .available_compiler_known_symbols()
            .unary_representation_argument(
                compilation
                    .semantic_value_store()
                    .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}")),
                RepresentationRole::Range,
                range_type,
            )
            .unwrap_or_else(|| panic!("range type must retain its element"));

        assert_eq!(
            compilation
                .available_compiler_known_symbols()
                .symbol_representation(
                    match compilation
                        .semantic_value_store()
                        .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"))
                        .type_data(element)
                        .unwrap_or_else(|error| panic!("element type must publish: {error:?}"))
                        .as_ref()
                    {
                        TypeData::Named {
                            definition: NamedTypeSymbolId::Struct(definition),
                            ..
                        } => *definition,
                        _ => panic!("range element must be a named integer"),
                    },
                ),
            Some(RepresentationRole::ScalarI32)
        );

        let iteration = iteration_expression(bound.value());

        let selection = compilation
            .iteration_source(key, iteration)
            .unwrap_or_else(|error| panic!("range iteration must select: {error:?}"));

        let count = selection
            .value()
            .as_ref()
            .and_then(SelectedIterationSource::exact_count)
            .unwrap_or_else(|| panic!("literal range must have an exact cardinality"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"));

        let count = values
            .constant_term_data(count)
            .unwrap_or_else(|error| panic!("range cardinality must publish: {error:?}"));

        assert!(matches!(
            count.as_ref(),
            ConstantTermData::IntegerLiteral { value, .. } if value.to_u64() == Some(4)
        ));
    }

    #[test]
    fn range_bounds_require_one_integer_type() {
        let non_integer = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let range: Range<bool> = false..true;\n",
            "}\n",
        ));

        assert_goal_state_diagnostic_kind(
            non_integer.check_diagnostics(),
            DiagnosticKind::CheckingRangeElementTypeMustBeInteger,
        );

        let mismatched = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let start: i32 = 0;\n",
            "    let end: u32 = 4;\n",
            "    let range = start..end;\n",
            "}\n",
        ));

        assert_goal_state_diagnostic_kind(
            mismatched.check_diagnostics(),
            DiagnosticKind::CheckingIncompatibleExpressionType,
        );

        let invalid_type = compilation(concat!(
            "module app;\n",
            "struct InvalidRange\n",
            "{\n",
            "    value: Range<bool>;\n",
            "}\n",
        ));

        assert_goal_state_diagnostic_kind(
            invalid_type.check_diagnostics(),
            DiagnosticKind::CheckingRangeElementTypeMustBeInteger,
        );
    }

    #[test]
    fn empty_literal_ranges_publish_zero_exact_cardinality() {
        assert!(super::count_fits_target_usize(u64::MAX.into(), 64));

        assert!(!super::count_fits_target_usize(
            u128::from(u64::MAX) + 1,
            64,
        ));

        for (bounds, expected) in [("4..4", 0), ("4..0", 0), ("-2..2", 4), ("2..(-2)", 0)] {
            let source = format!(
                "module app;\nfunc main()\n{{\n    for value in {bounds}\n    {{\n    }}\n}}\n"
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{bounds}: {:#?}",
                compilation.check_diagnostics()
            );

            let key = source_callable_body_key(&compilation);

            let bound = compilation
                .bound_unit(key.clone())
                .unwrap_or_else(|error| panic!("{bounds} must bind: {error:?}"));

            let selection = compilation
                .iteration_source(key, iteration_expression(bound.value()))
                .unwrap_or_else(|error| panic!("{bounds} must select: {error:?}"));

            let count = selection
                .value()
                .as_ref()
                .and_then(SelectedIterationSource::exact_count)
                .unwrap_or_else(|| panic!("{bounds} must have an exact cardinality"));

            let values = compilation
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("{bounds} values must publish: {error:?}"));

            let count = values
                .constant_term_data(count)
                .unwrap_or_else(|error| panic!("{bounds} cardinality must publish: {error:?}"));

            assert!(matches!(
                count.as_ref(),
                ConstantTermData::IntegerLiteral { value, .. }
                    if value.to_u64() == Some(expected)
            ));
        }
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
            "    type Element = bool;\n",
            "    type Cursor = ItemsCursor;\n",
            "\n",
            "    consume func iterate() -> ItemsCursor\n",
            "    {\n",
            "    }\n",
            "}\n",
            "\n",
            "impl ItemsCursor(Iterator)\n",
            "{\n",
            "    type Element = bool;\n",
            "\n",
            "    mut func next() -> bool?\n",
            "    {\n",
            "    }\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let items: Items = Items {};\n",
            "    let folded_all: bool = all(items);\n",
            "    let folded_any: bool = any(items);\n",
            "\n",
            "    for item in items\n",
            "    {\n",
            "        yield item;\n",
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

        let binding_context = match compilation.binding_context_for(&key, &cancellation) {
            Ok(binding_context) => binding_context,
            Err(error) => panic!("binder context must be available: {error:?}"),
        };

        let structures = binding_context
            .symbols()
            .structures()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [items, cursor] = structures.as_slice() else {
            panic!("test source must declare Items and ItemsCursor");
        };

        let source_type = named_type(&binding_context, items.id());
        let cursor_type = named_type(&binding_context, cursor.id());

        let boolean = binding_context
            .symbols()
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(RepresentationRole::ScalarBool)
            .unwrap_or_else(|| panic!("compiler-known bool must be available"));

        let element_type = named_type(&binding_context, boolean);

        let subject_type =
            iteration_subject_type(binding_context.semantic_values(), source_type, mode)
                .unwrap_or_else(|error| panic!("iteration subject must be available: {error:?}"));

        let protocol = compilation
            .available_compiler_known_symbols()
            .iteration_protocol()
            .unwrap_or_else(|| panic!("iteration protocol must be available"));

        let implementation = binding_context
            .symbols()
            .unnamed_trait_implementations()
            .iter()
            .find(|implementation| implementation.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must declare an Iterable implementation"));

        let coherence = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
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

        let application = binding_context
            .semantic_values()
            .trait_application_data(application)
            .unwrap_or_else(|error| panic!("Iterable application must be available: {error:?}"));

        assert_eq!(application.definition(), protocol.iterable_trait());

        let mut diagnostics = DiagnosticBag::new();

        let candidates = compilation
            .iteration_candidates(
                &binding_context,
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

        let iterable_witness = binding_context
            .semantic_values()
            .implementation_instance_data(selected.iterable_witness())
            .unwrap_or_else(|error| panic!("Iterable witness must be available: {error:?}"));

        let iterator_witness = binding_context
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

        let types = compilation
            .expression_types(key.clone())
            .unwrap_or_else(|error| panic!("final expression types must publish: {error:?}"));

        let selections = compilation
            .semantic_selections(key)
            .unwrap_or_else(|error| panic!("final semantic selections must publish: {error:?}"));

        assert!(matches!(
            selections.value().expression(iteration),
            Some(SemanticSelection::Iteration(selection))
                if selection.iterate() == selected.iterate()
                    && selection.next() == selected.next()
        ));

        let boolean_all =
            boolean_fold_expression(bound.value(), BoundStructuredExpressionKind::BooleanAllFold);

        assert!(matches!(
            selections.value().expression(boolean_all),
            Some(SemanticSelection::Iteration(selection))
                if selection.iterate() == selected.iterate()
                    && selection.next() == selected.next()
        ));

        let boolean_any =
            boolean_fold_expression(bound.value(), BoundStructuredExpressionKind::BooleanAnyFold);

        assert!(matches!(
            selections.value().expression(boolean_any),
            Some(SemanticSelection::Iteration(selection))
                if selection.iterate() == selected.iterate()
                    && selection.next() == selected.next()
        ));

        let reference = iteration_binding_reference_expression(bound.value());

        let actual = types
            .value()
            .expression(reference)
            .map(|result| result.ty())
            .unwrap_or_else(|| panic!("iteration binding reference must have a final type"));

        assert_eq!(
            binding_context
                .semantic_values()
                .type_data(actual)
                .unwrap_or_else(|error| panic!("actual type must be available: {error:?}")),
            binding_context
                .semantic_values()
                .type_data(element_type)
                .unwrap_or_else(|error| panic!("element type must be available: {error:?}")),
            "{:?}",
            types.diagnostics()
        );
    }

    fn iteration_expression(
        unit: &bray_bound_tree::BoundUnit,
    ) -> bray_bound_tree::BoundExpressionId {
        find_expression(unit, |expression| {
            matches!(expression, BoundExpression::For(_))
        })
    }

    fn boolean_fold_expression(
        unit: &bray_bound_tree::BoundUnit,
        kind: BoundStructuredExpressionKind,
    ) -> bray_bound_tree::BoundExpressionId {
        find_expression(unit, |expression| {
            matches!(
                expression,
                BoundExpression::Structured(expression)
                    if expression.kind() == kind
            )
        })
    }

    fn find_expression(
        unit: &bray_bound_tree::BoundUnit,
        predicate: impl Fn(&BoundExpression) -> bool,
    ) -> bray_bound_tree::BoundExpressionId {
        unit.tree()
            .expressions()
            .find_map(|(expression, node)| predicate(node).then_some(expression))
            .unwrap_or_else(|| panic!("test source must bind the expected expression"))
    }

    fn iteration_binding_reference_expression(
        unit: &bray_bound_tree::BoundUnit,
    ) -> bray_bound_tree::BoundExpressionId {
        let mut reference = None;

        let outcome = walk_bound_unit_view(unit.view(), unit.root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            let is_binding_reference = match unit.view().expression(expression) {
                Some(BoundExpression::PatternReference(reference)) => {
                    reference.name().as_str() == "item"
                }
                Some(BoundExpression::Name(name)) => {
                    let bray_bound_tree::BoundReferenceTarget::Local(
                        bray_symbols::AnyLocalSymbolId::Binding(binding),
                    ) = name.target()
                    else {
                        return BoundWalkControl::Continue;
                    };

                    unit.local_symbols()
                        .binding(binding)
                        .is_some_and(|binding| binding.name().as_str() == "item")
                }
                _ => false,
            };

            if is_binding_reference {
                reference = Some(expression);

                return BoundWalkControl::Stop;
            }

            BoundWalkControl::Continue
        });

        assert_eq!(outcome, BoundWalkOutcome::Stopped);

        match reference {
            Some(reference) => reference,
            None => panic!("test source must bind one iteration pattern reference"),
        }
    }

    fn named_type(
        binding_context: &super::CompilationBindingContext<'_>,
        definition: StructSymbolId,
    ) -> bray_symbols::TypeId {
        let substitution = empty_substitution(binding_context.semantic_values(), definition.into())
            .unwrap_or_else(|error| panic!("type substitution must be interned: {error:?}"));

        binding_context
            .semantic_values()
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap_or_else(|error| panic!("named type must be interned: {error:?}"))
    }
}
