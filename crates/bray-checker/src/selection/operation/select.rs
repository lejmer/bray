use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, CheckedExpressionTypes,
    ConstructionTarget, ExpressionTypeResult, SelectedConstruction, SelectedOperation,
    SelectionKind,
};

use crate::unit::semantic_input_failure;
use crate::{
    CheckerInfrastructureError, CheckerInputKind, CheckerQueryError, CheckerRequestContext,
    CheckerUnitView,
};

use super::super::{
    CandidateSelection, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidatePlan, OperationCandidateState, OperationSelectionRequest,
    SelectionCandidateRejectionReason, SelectionCandidateSignature, SelectionFailure,
    SelectionFailureCandidate, SelectionInaccessibility, SelectionRejectedCandidate,
};
use super::construction::{ConstructionInputMapping, map_construction_inputs};
use super::conversion::{is_builtin_conversion, validate_conversion};
use super::validation::{implementation_selections_match, validate_operation_instances};

pub(in crate::selection) fn select<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: OperationSelectionRequest,
) -> Result<Option<CandidateSelection<SelectedOperation>>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return Ok(None);
    }

    validate_request(request, types, &input)?;

    let OperationSelectionRequest {
        expression,
        kind,
        operands,
        mut candidates,
    } = input;

    if !super::super::order::canonicalize_by_key(&mut candidates, OperationCandidate::key) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
    }

    let actual_types = expression_types(types, &operands)?;

    if kind != SelectionKind::Construction
        && actual_types.iter().any(|result| result.is_recovered())
    {
        return Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Recovered,
        )));
    }

    let mut applicable = Vec::new();

    let mut inaccessible = Vec::new();
    let mut rejected = Vec::new();
    let mut has_recovered = false;

    for candidate in candidates {
        if request.is_cancelled() {
            return Ok(None);
        }

        let state = candidate.state();

        match state {
            OperationCandidateState::Unavailable => continue,
            OperationCandidateState::Inaccessible | OperationCandidateState::Available => {}
            OperationCandidateState::Recovered => {
                has_recovered = true;
                continue;
            }
        }

        let (key, plan, implementation_selections, compiler_known_operations, _) =
            candidate.into_parts();

        let diagnostic_candidate = operation_failure_candidate(key.clone(), &plan);

        let context = CandidateContext {
            request,
            types,
            expression,
            kind,
            actual_types: &actual_types,
        };

        match check_candidate(
            context,
            plan,
            &implementation_selections,
            &compiler_known_operations,
        )? {
            CandidateCheck::Applicable(_operation)
                if state == OperationCandidateState::Inaccessible =>
            {
                inaccessible.push(diagnostic_candidate);
            }
            CandidateCheck::Applicable(operation) => {
                applicable.push((diagnostic_candidate, key, operation));
            }
            CandidateCheck::Incompatible(reason) => rejected.push(SelectionRejectedCandidate::new(
                diagnostic_candidate,
                reason,
            )),
            CandidateCheck::Recovered => has_recovered = true,
        }
    }

    if kind == SelectionKind::Conversion
        && applicable
            .iter()
            .any(|(_, _, operation)| is_builtin_conversion(operation))
    {
        applicable.retain(|(_, _, operation)| is_builtin_conversion(operation));
    }

    let selection = match applicable.len() {
        1 => CandidateSelection::Selected(applicable.remove(0).2),
        count if count > 1 => CandidateSelection::Failed(SelectionFailure::Ambiguous(
            applicable
                .into_iter()
                .map(|(candidate, _, _)| candidate)
                .collect(),
        )),
        _ if !inaccessible.is_empty() => {
            CandidateSelection::Failed(SelectionFailure::Inaccessible {
                candidates: inaccessible.into(),
                reason: SelectionInaccessibility::NotVisibleFromRequestingContext,
            })
        }
        _ if has_recovered => CandidateSelection::Failed(SelectionFailure::Recovered),
        _ if !rejected.is_empty() => {
            CandidateSelection::Failed(SelectionFailure::Incompatible(rejected.into()))
        }
        _ => CandidateSelection::Failed(SelectionFailure::Unavailable),
    };

    Ok(Some(selection))
}

fn operation_failure_candidate(
    key: super::super::SelectionCandidateKey,
    plan: &OperationCandidatePlan,
) -> SelectionFailureCandidate {
    let (operand_types, result_type) = match plan {
        OperationCandidatePlan::Exact {
            operation,
            operand_types,
        } => (operand_types.clone(), operation.result_type()),
        OperationCandidatePlan::Construction {
            result_type,
            inputs,
            ..
        } => (
            inputs.iter().map(|input| input.ty()).collect(),
            Some(*result_type),
        ),
    };

    SelectionFailureCandidate::new(
        key,
        SelectionCandidateSignature::Operation {
            operand_types,
            result_type,
        },
    )
}

enum CandidateCheck {
    Applicable(SelectedOperation),
    Incompatible(SelectionCandidateRejectionReason),
    Recovered,
}

#[derive(Clone, Copy)]
struct CandidateContext<'input, 'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'request, C>,
    types: &'input CheckedExpressionTypes,
    expression: BoundExpressionId,
    kind: SelectionKind,
    actual_types: &'input [ExpressionTypeResult],
}

fn check_candidate<C>(
    context: CandidateContext<'_, '_, C>,
    plan: OperationCandidatePlan,
    evidence: &[ImplementationSelectionEvidence],
    compiler_known_operations: &[super::super::CompilerKnownOperationEvidence],
) -> Result<CandidateCheck, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let CandidateContext {
        request,
        types,
        expression,
        kind,
        actual_types,
    } = context;

    let operation = match plan {
        OperationCandidatePlan::Exact {
            operation,
            operand_types,
        } => {
            if operation.kind() != kind || operand_types.len() != actual_types.len() {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
            }

            if !operand_types
                .iter()
                .zip(actual_types)
                .all(|(expected, actual)| *expected == actual.ty())
            {
                return Ok(CandidateCheck::Incompatible(
                    SelectionCandidateRejectionReason::OperandTypes {
                        provided: actual_types.iter().map(|actual| actual.ty()).collect(),
                    },
                ));
            }

            operation
        }
        OperationCandidatePlan::Construction {
            target,
            result_type,
            inputs,
        } => {
            if kind != SelectionKind::Construction {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
            }

            if let ConstructionTarget::Struct(structure) = target {
                let representation = match request.declared_type_representation(structure.into()) {
                    Ok(representation) => representation,
                    Err(crate::CheckerQueryError::Cancelled) => {
                        return Err(CheckerQueryError::Cancelled);
                    }
                    Err(CheckerQueryError::Infrastructure(error)) => return Err(error.into()),
                    Err(CheckerQueryError::Upstream(error)) => {
                        return Err(CheckerQueryError::Upstream(error));
                    }
                };

                if representation.value().has_flexible_trailing_member() {
                    return Ok(CandidateCheck::Incompatible(
                        SelectionCandidateRejectionReason::ExpressionForm,
                    ));
                }
            }

            let inputs = match map_construction_inputs(request, types, expression, target, &inputs)?
            {
                ConstructionInputMapping::Mapped(inputs) => inputs,
                ConstructionInputMapping::Rejected(reason) => {
                    return Ok(CandidateCheck::Incompatible(
                        SelectionCandidateRejectionReason::ConstructionInput(reason),
                    ));
                }
            };

            if inputs.recovered {
                return Ok(CandidateCheck::Recovered);
            }

            SelectedOperation::Construction(SelectedConstruction::new(
                target,
                result_type,
                inputs.values,
            ))
        }
    };

    let Some(source_expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
    };

    if !operation.matches_expression(source_expression)
        || !operation_is_valid(request, types, expression, &operation)?
    {
        return Ok(CandidateCheck::Incompatible(
            SelectionCandidateRejectionReason::ExpressionForm,
        ));
    }

    validate_operation_instances(request, &operation)?;

    if !implementation_selections_match(request, &operation, evidence)? {
        return Ok(CandidateCheck::Incompatible(
            SelectionCandidateRejectionReason::RequiredImplementation,
        ));
    }

    if !super::validation::compiler_known_operations_match(
        request,
        &operation,
        source_expression,
        actual_types,
        compiler_known_operations,
    )? {
        return Ok(CandidateCheck::Incompatible(
            SelectionCandidateRejectionReason::RequiredLanguageOperation,
        ));
    }

    Ok(CandidateCheck::Applicable(operation))
}

fn validate_request<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    input: &OperationSelectionRequest,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(error) = semantic_input_failure(
        request,
        [(
            CheckerInputKind::ExpressionTypes,
            (types.unit(), types.kind()),
        )],
    ) {
        return Err(error);
    }

    if input.kind() == SelectionKind::Callable || !expression_matches_request(request, input) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(())
}

fn expression_matches_request<C>(
    request: CheckerUnitView<'_, C>,
    input: &OperationSelectionRequest,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(input.expression()) else {
        return false;
    };

    let category_matches = match (input.kind(), expression) {
        (
            SelectionKind::Member,
            BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_),
        )
        | (
            SelectionKind::Operator,
            BoundExpression::Unary(_) | BoundExpression::Binary(_) | BoundExpression::Assignment(_),
        ) => true,
        (SelectionKind::Index, BoundExpression::Structured(expression)) => matches!(
            expression.kind(),
            BoundStructuredExpressionKind::ElementIndex | BoundStructuredExpressionKind::SliceIndex
        ),
        (SelectionKind::Construction, BoundExpression::StructConstruction(_)) => true,
        (
            SelectionKind::Construction,
            BoundExpression::LeadingDotVariant(_)
            | BoundExpression::UnqualifiedVariant(_)
            | BoundExpression::MemberAccess(_),
        ) => true,
        (SelectionKind::Construction, BoundExpression::Call(call)) => {
            union_variant_reference(request, call.callee())
        }
        (SelectionKind::Construction, BoundExpression::BoxConstruction(_)) => true,
        (SelectionKind::Conversion, BoundExpression::Conversion(_)) => true,
        (SelectionKind::Implementation, _) => true,
        _ => false,
    };

    if !category_matches {
        return false;
    }

    if input.kind() == SelectionKind::Implementation {
        return input.operands().is_empty();
    }

    source_operands(expression, input.kind())
        .is_some_and(|operands| operands.into_iter().eq(input.operands().iter().copied()))
}

fn source_operands(
    expression: &BoundExpression,
    kind: SelectionKind,
) -> Option<Vec<BoundExpressionId>> {
    match expression {
        BoundExpression::StructConstruction(construction) => Some(
            construction
                .fields()
                .iter()
                .map(bray_bound_tree::BoundStructFieldInitializer::expression)
                .collect(),
        ),
        BoundExpression::Call(call) if kind == SelectionKind::Construction => Some(
            call.arguments()
                .iter()
                .map(bray_bound_tree::BoundArgument::expression)
                .collect(),
        ),
        BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::MemberAccess(_)
            if kind == SelectionKind::Construction =>
        {
            Some(Vec::new())
        }
        _ => Some(expression.child_expressions().collect()),
    }
}

fn union_variant_reference<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    match request.view().expression(expression) {
        Some(
            BoundExpression::LeadingDotVariant(_)
            | BoundExpression::UnqualifiedVariant(_)
            | BoundExpression::MemberAccess(_),
        ) => true,
        Some(BoundExpression::Name(name)) => matches!(
            name.target(),
            bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::AnySymbolId::UnionVariant(_)
            )
        ),
        _ => false,
    }
}

fn expression_types(
    types: &CheckedExpressionTypes,
    expressions: &[BoundExpressionId],
) -> Result<Vec<ExpressionTypeResult>, CheckerInfrastructureError> {
    expressions
        .iter()
        .map(|expression| {
            types
                .expression(*expression)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)
        })
        .collect()
}

fn operation_is_valid<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
    operation: &SelectedOperation,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(result_type) = operation.result_type() {
        request
            .semantic_values()
            .type_data(result_type)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;
    }

    match operation {
        SelectedOperation::Conversion(conversion) => {
            let Some(BoundExpression::Conversion(source)) = request.view().expression(expression)
            else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            let source_type = types
                .expression(source.operand())
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            if source_type.is_recovered() || conversion.source_type() != source_type.ty() {
                return Ok(false);
            }

            validate_conversion(request, conversion)
        }
        SelectedOperation::Implementation(witness) => {
            let subject = types
                .expression(expression)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            Ok(!subject.is_recovered() && witness.requirement().subject() == subject.ty())
        }
        _ => Ok(true),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundAssignmentExpression, BoundAssignmentOperator, BoundBinaryExpression,
        BoundConversionExpression, BoundExpression, BoundExpressionId, BoundOperator,
        BoundStructConstructionExpression, BoundStructFieldInitializer, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundUnit, BoundUnitId, CheckedExpressionTypes,
        ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget, ConversionTarget,
        ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus, IndexTarget, MemberTarget,
        OperatorTarget, SelectedConstructionInput, SelectedConversion,
        SelectedImplementationWitness, SelectedOperation, SelectionKind,
    };
    use bray_symbols::{
        CallablePosition, FunctionSymbolId, ImplementationSelection,
        StructFieldDefaultProviderSymbolId, StructFieldSymbolId, StructSymbolId, SymbolId,
        SymbolKind, TraitSymbolId, TypeId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, checked_expression_types,
        checked_expression_types_from_entries, compiler_known_symbol, declaration_key,
        expression_unit, push_expression, semantic_values, symbol_name, tuple_type,
    };
    use crate::{
        CandidateSelection, CheckerUnitView, ConstructionInputSurface, DefaultSemanticSelector,
        ImplementationSelectionEvidence, OperationCandidate, OperationCandidateState,
        OperationSelectionRequest, SelectionCandidateKey, SelectionFailure, SemanticSelector,
    };

    #[test]
    fn exact_operator_candidates_publish_their_selected_target() {
        let fixture = operator_fixture(BoundUnitId::new(80));

        let operation = SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(BoundOperator::Add),
            result_type: fixture.value_type,
        };

        let candidate = OperationCandidate::built_in(
            operation.clone(),
            [fixture.value_type, fixture.value_type],
            OperationCandidateState::Available,
        );

        let result = select(&fixture, [candidate]);

        assert_eq!(result.value(), &CandidateSelection::Selected(operation));
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn compound_assignments_accept_their_binary_operator_candidate() {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(114), |tree, origin| {
            let destination = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let value = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let assignment = push_expression(
                tree,
                BoundExpression::Assignment(BoundAssignmentExpression::new(
                    origin,
                    BoundAssignmentOperator::Add,
                    [destination, value],
                    None,
                    false,
                )),
            );

            vec![destination, value, assignment]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        let operation = SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(BoundOperator::Add),
            result_type: value_type,
        };

        let input = OperationSelectionRequest::new(
            expressions[2],
            SelectionKind::Operator,
            [expressions[0], expressions[1]],
            [OperationCandidate::built_in(
                operation.clone(),
                [value_type, value_type],
                OperationCandidateState::Available,
            )],
        );

        let selection = select_request(&unit, &types, input);

        assert_eq!(selection.value(), &CandidateSelection::Selected(operation));
        assert!(selection.diagnostics().is_empty());
    }

    #[test]
    fn operator_selection_ignores_candidate_result_types() {
        let fixture = operator_fixture(BoundUnitId::new(81));
        let other_result = tuple_type([fixture.value_type]);

        let candidates = [
            operator_candidate(1, fixture.value_type, fixture.value_type),
            operator_candidate(2, fixture.value_type, other_result),
        ];

        let result = select(&fixture, candidates);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Ambiguous(_))
        ));
    }

    #[test]
    fn operator_targets_must_match_the_source_operator_contract() {
        let fixture = operator_fixture(BoundUnitId::new(82));

        let candidate = OperationCandidate::built_in(
            SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Multiply),
                result_type: fixture.value_type,
            },
            [fixture.value_type, fixture.value_type],
            OperationCandidateState::Available,
        );

        let result = select(&fixture, [candidate]);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible(_))
        ));
    }

    #[test]
    fn conversion_candidates_must_start_from_the_bound_operand_type() {
        let source_type = tuple_type([]);
        let target_type = tuple_type([source_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(87), |tree, origin| {
            let operand = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(source_type)),
            );

            let conversion = push_expression(
                tree,
                BoundExpression::Conversion(BoundConversionExpression::new(
                    origin,
                    operand,
                    origin.source_anchor().syntax(),
                    Some(target_type),
                    Some(target_type),
                    false,
                )),
            );

            vec![operand, conversion]
        });

        let types = checked_expression_types_from_entries(
            &unit,
            [
                (
                    expressions[0],
                    ExpressionTypeResult::new(source_type, ExpressionTypeStatus::Valid),
                ),
                (
                    expressions[1],
                    ExpressionTypeResult::new(target_type, ExpressionTypeStatus::Valid),
                ),
            ],
        );

        let candidate = OperationCandidate::built_in(
            SelectedOperation::Conversion(SelectedConversion::new(
                target_type,
                target_type,
                ConversionTarget::Identity,
            )),
            [source_type],
            OperationCandidateState::Available,
        );

        let input = OperationSelectionRequest::new(
            expressions[1],
            SelectionKind::Conversion,
            [expressions[0]],
            [candidate],
        );

        let result = select_request(&unit, &types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible(_))
        ));
    }

    #[test]
    fn construction_maps_named_inputs_then_declaration_order_defaults() {
        let fixture = construction_fixture(BoundUnitId::new(83));

        let first = StructFieldSymbolId::from_symbol_id(SymbolId::new(10));
        let second = StructFieldSymbolId::from_symbol_id(SymbolId::new(11));
        let default = StructFieldDefaultProviderSymbolId::from_symbol_id(SymbolId::new(12));
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(13));

        let candidate = OperationCandidate::symbol_construction(
            declaration_key(SymbolKind::Struct, 13),
            ConstructionTarget::Struct(structure),
            fixture.value_type,
            [
                ConstructionInputSurface::new(
                    ConstructionInputId::StructField(first),
                    symbol_name("first"),
                    CallablePosition::NamedOnly,
                    fixture.value_type,
                    Some(ConstructionDefaultProvider::StructField(default)),
                    0,
                ),
                ConstructionInputSurface::new(
                    ConstructionInputId::StructField(second),
                    symbol_name("second"),
                    CallablePosition::NamedOnly,
                    fixture.value_type,
                    None,
                    1,
                ),
            ],
            OperationCandidateState::Available,
        );

        let input = OperationSelectionRequest::new(
            fixture.construction,
            SelectionKind::Construction,
            [fixture.value],
            [candidate],
        );

        let result = select_request(&fixture.unit, &fixture.types, input);

        let CandidateSelection::Selected(SelectedOperation::Construction(construction)) =
            result.value()
        else {
            panic!("construction candidate must be selected");
        };

        assert_eq!(
            construction.inputs(),
            [
                SelectedConstructionInput::Explicit {
                    expression: fixture.value,
                    input: ConstructionInputId::StructField(second),
                    ty: fixture.value_type,
                    ordinal: 1,
                },
                SelectedConstructionInput::Default {
                    input: ConstructionInputId::StructField(first),
                    provider: ConstructionDefaultProvider::StructField(default),
                    ty: fixture.value_type,
                    ordinal: 0,
                },
            ]
        );
    }

    #[test]
    fn construction_selection_supplies_expected_types_to_provisional_inputs() {
        let fixture = construction_fixture(BoundUnitId::new(90));

        let types = CheckedExpressionTypes::new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [
                ExpressionTypeEntry::new(
                    fixture.value,
                    ExpressionTypeResult::new(fixture.value_type, ExpressionTypeStatus::Recovered),
                ),
                ExpressionTypeEntry::new(
                    fixture.construction,
                    ExpressionTypeResult::new(fixture.value_type, ExpressionTypeStatus::Valid),
                ),
            ],
        );

        let field = StructFieldSymbolId::from_symbol_id(SymbolId::new(10));
        let structure = StructSymbolId::from_symbol_id(SymbolId::new(13));

        let candidate = OperationCandidate::symbol_construction(
            declaration_key(SymbolKind::Struct, 13),
            ConstructionTarget::Struct(structure),
            fixture.value_type,
            [ConstructionInputSurface::new(
                ConstructionInputId::StructField(field),
                symbol_name("second"),
                CallablePosition::NamedOnly,
                fixture.value_type,
                None,
                0,
            )],
            OperationCandidateState::Available,
        );

        let input = OperationSelectionRequest::new(
            fixture.construction,
            SelectionKind::Construction,
            [fixture.value],
            [candidate],
        );

        let result = select_request(&fixture.unit, &types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Selected(SelectedOperation::Construction(_))
        ));
    }

    #[test]
    fn member_candidates_are_canonicalized_without_ranking() {
        let fixture = member_fixture(BoundUnitId::new(84));

        let input = OperationSelectionRequest::new(
            fixture.member,
            SelectionKind::Member,
            [fixture.receiver],
            [
                member_candidate(2, fixture.value_type),
                member_candidate(1, fixture.value_type),
            ],
        );

        let result = select_request(&fixture.unit, &fixture.types, input);

        let CandidateSelection::Failed(SelectionFailure::Ambiguous(keys)) = result.value() else {
            panic!("two applicable member candidates must remain ambiguous");
        };

        assert_eq!(
            keys.iter()
                .map(|candidate| candidate.key())
                .collect::<Vec<_>>(),
            [
                &SelectionCandidateKey::Symbol(declaration_key(SymbolKind::Function, 1)),
                &SelectionCandidateKey::Symbol(declaration_key(SymbolKind::Function, 2)),
            ]
        );
    }

    #[test]
    fn candidate_states_preserve_accessibility_availability_and_recovery() {
        let fixture = operator_fixture(BoundUnitId::new(85));

        let inaccessible = select(
            &fixture,
            [operator_candidate_with_state(
                fixture.value_type,
                OperationCandidateState::Inaccessible,
            )],
        );

        assert!(matches!(
            inaccessible.value(),
            CandidateSelection::Failed(SelectionFailure::Inaccessible { .. })
        ));

        assert!(inaccessible.diagnostics().is_empty());

        let unavailable = select(
            &fixture,
            [operator_candidate_with_state(
                fixture.value_type,
                OperationCandidateState::Unavailable,
            )],
        );

        assert!(matches!(
            unavailable.value(),
            CandidateSelection::Failed(SelectionFailure::Unavailable)
        ));

        assert!(!unavailable.diagnostics().is_empty());

        let recovered = select(
            &fixture,
            [operator_candidate_with_state(
                fixture.value_type,
                OperationCandidateState::Recovered,
            )],
        );

        assert!(matches!(
            recovered.value(),
            CandidateSelection::Failed(SelectionFailure::Recovered)
        ));

        assert!(recovered.diagnostics().is_empty());
    }

    #[test]
    fn cancellation_publishes_no_operation_selection() {
        let fixture = operator_fixture(BoundUnitId::new(86));
        let candidate = operator_candidate(1, fixture.value_type, fixture.value_type);
        let input = operator_request(&fixture, [candidate]);
        let context = TestCheckerContext::new(true);

        let outcome = select_outcome(&fixture.unit, &fixture.types, input, &context);

        assert_eq!(outcome, crate::CheckerOutcome::Cancelled);
    }

    #[test]
    fn built_in_index_candidates_publish_their_selected_target() {
        let fixture = index_fixture(BoundUnitId::new(88));

        let operation = SelectedOperation::Index {
            target: IndexTarget::ArrayElement,
            result_type: fixture.value_type,
        };

        let candidate = OperationCandidate::built_in(
            operation.clone(),
            [fixture.value_type, fixture.value_type],
            OperationCandidateState::Available,
        );

        let input = OperationSelectionRequest::new(
            fixture.index,
            SelectionKind::Index,
            fixture.operands,
            [candidate],
        );

        let result = select_request(&fixture.unit, &fixture.types, input);

        assert_eq!(result.value(), &CandidateSelection::Selected(operation));
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn implementation_candidates_publish_their_exact_witness() {
        let fixture = implementation_fixture(BoundUnitId::new(89));
        let operation = SelectedOperation::Implementation(fixture.selection);

        let candidate =
            OperationCandidate::built_in(operation.clone(), [], OperationCandidateState::Available)
                .with_implementation_selections([ImplementationSelectionEvidence::new(
                    fixture.selection.requirement(),
                    ImplementationSelection::Selected(fixture.selection.witness()),
                )]);

        let input = OperationSelectionRequest::new(
            fixture.expression,
            SelectionKind::Implementation,
            [],
            [candidate],
        );

        let result = select_request(&fixture.unit, &fixture.types, input);

        assert_eq!(result.value(), &CandidateSelection::Selected(operation));
        assert!(result.diagnostics().is_empty());
    }

    struct OperatorFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        operation: BoundExpressionId,
        operands: [BoundExpressionId; 2],
        value_type: TypeId,
    }

    struct ConstructionFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        construction: BoundExpressionId,
        value: BoundExpressionId,
        value_type: TypeId,
    }

    struct MemberFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        member: BoundExpressionId,
        receiver: BoundExpressionId,
        value_type: TypeId,
    }

    struct IndexFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        index: BoundExpressionId,
        operands: [BoundExpressionId; 2],
        value_type: TypeId,
    }

    struct ImplementationFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        expression: BoundExpressionId,
        selection: SelectedImplementationWitness,
    }

    fn operator_fixture(unit: BoundUnitId) -> OperatorFixture {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let left = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let right = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let operation = push_expression(
                tree,
                BoundExpression::Binary(BoundBinaryExpression::new(
                    origin,
                    BoundOperator::Add,
                    [left, right],
                    None,
                    false,
                )),
            );

            vec![left, right, operation]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        OperatorFixture {
            operation: expressions[2],
            operands: [expressions[0], expressions[1]],
            unit,
            types,
            value_type,
        }
    }

    fn construction_fixture(unit: BoundUnitId) -> ConstructionFixture {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let value = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let construction = push_expression(
                tree,
                BoundExpression::StructConstruction(BoundStructConstructionExpression::new(
                    origin,
                    None,
                    [BoundStructFieldInitializer::new(
                        Some(symbol_name("second")),
                        value,
                        false,
                    )],
                    None,
                    false,
                )),
            );

            vec![value, construction]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        ConstructionFixture {
            construction: expressions[1],
            value: expressions[0],
            unit,
            types,
            value_type,
        }
    }

    fn member_fixture(unit: BoundUnitId) -> MemberFixture {
        use bray_bound_tree::{BoundMemberAccessExpression, BoundMemberSelector};

        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let receiver = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let member = push_expression(
                tree,
                BoundExpression::MemberAccess(BoundMemberAccessExpression::new(
                    origin,
                    receiver,
                    Some(BoundMemberSelector::Name(symbol_name("value"))),
                    None,
                    false,
                )),
            );

            vec![receiver, member]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        MemberFixture {
            member: expressions[1],
            receiver: expressions[0],
            unit,
            types,
            value_type,
        }
    }

    fn index_fixture(unit: BoundUnitId) -> IndexFixture {
        let value_type = tuple_type([]);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let receiver = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let selector = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(value_type)),
            );

            let index = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::ElementIndex,
                    [receiver, selector],
                    [],
                    [],
                    Some(value_type),
                    false,
                )),
            );

            vec![receiver, selector, index]
        });

        let result = ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        IndexFixture {
            index: expressions[2],
            operands: [expressions[0], expressions[1]],
            unit,
            types,
            value_type,
        }
    }

    fn implementation_fixture(unit: BoundUnitId) -> ImplementationFixture {
        let subject = tuple_type([]);
        let argument = tuple_type([subject]);

        let trait_definition = compiler_known_symbol::<TraitSymbolId>("Storage");

        let requirement = bray_symbols::testing::implementation_requirement(
            semantic_values(),
            trait_definition,
            subject,
            argument,
        );

        let witness = bray_symbols::testing::implementation_instance(semantic_values(), 32);

        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let expression = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(subject)),
            );

            vec![expression]
        });

        let result = ExpressionTypeResult::new(subject, ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        ImplementationFixture {
            unit,
            types,
            expression: expressions[0],
            selection: SelectedImplementationWitness::new(requirement, witness),
        }
    }

    fn operator_candidate(
        declaration: u32,
        operand_type: TypeId,
        result_type: TypeId,
    ) -> OperationCandidate {
        OperationCandidate::symbol(
            declaration_key(SymbolKind::Function, declaration),
            SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Add),
                result_type,
            },
            [operand_type, operand_type],
            OperationCandidateState::Available,
        )
    }

    fn operator_candidate_with_state(
        operand_type: TypeId,
        state: OperationCandidateState,
    ) -> OperationCandidate {
        OperationCandidate::built_in(
            SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Add),
                result_type: operand_type,
            },
            [operand_type, operand_type],
            state,
        )
    }

    fn member_candidate(declaration: u32, operand_type: TypeId) -> OperationCandidate {
        let member = FunctionSymbolId::from_symbol_id(SymbolId::new(declaration));

        OperationCandidate::symbol(
            declaration_key(SymbolKind::Function, declaration),
            SelectedOperation::Member(MemberTarget::new(member.into(), operand_type, [])),
            [operand_type],
            OperationCandidateState::Available,
        )
    }

    fn select(
        fixture: &OperatorFixture,
        candidates: impl IntoIterator<Item = OperationCandidate>,
    ) -> bray_diagnostics::DiagnosticResult<CandidateSelection<SelectedOperation>> {
        let input = operator_request(fixture, candidates);

        select_request(&fixture.unit, &fixture.types, input)
    }

    fn operator_request(
        fixture: &OperatorFixture,
        candidates: impl IntoIterator<Item = OperationCandidate>,
    ) -> OperationSelectionRequest {
        OperationSelectionRequest::new(
            fixture.operation,
            SelectionKind::Operator,
            fixture.operands,
            candidates,
        )
    }

    fn select_request(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        input: OperationSelectionRequest,
    ) -> bray_diagnostics::DiagnosticResult<CandidateSelection<SelectedOperation>> {
        let context = TestCheckerContext::new(false);

        match select_outcome(unit, types, input, &context) {
            crate::CheckerOutcome::Complete(result) => result,
            other => panic!("operation selection must complete: {other:?}"),
        }
    }

    fn select_outcome<'unit>(
        unit: &'unit BoundUnit,
        types: &CheckedExpressionTypes,
        input: OperationSelectionRequest,
        context: &'unit TestCheckerContext,
    ) -> crate::CheckerOutcome<CandidateSelection<SelectedOperation>> {
        let entry = callable_entry(unit.key());

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("operation selection request must be valid: {error:?}"),
        };

        DefaultSemanticSelector.select_operation(request, types, input)
    }
}
