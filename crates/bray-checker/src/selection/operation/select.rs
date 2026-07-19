use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, CheckedExpressionTypes,
    ConstructionTarget, ExpressionTypeResult, IndexTarget, SelectedConstruction, SelectedOperation,
    SelectionKind,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::super::{
    CandidateSelection, ImplementationSelectionEvidence, OperationCandidate,
    OperationCandidatePlan, OperationCandidateState, OperationSelectionRequest, SelectionFailure,
};
use super::construction::map_construction_inputs;
use super::conversion::{is_builtin_conversion, validate_conversion};
use super::validation::{implementation_selections_match, validate_operation_instances};

pub(in crate::selection) fn select<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    input: OperationSelectionRequest,
) -> Result<Option<CandidateSelection<SelectedOperation>>, CheckerInfrastructureError>
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
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let actual_types = expression_types(types, &operands)?;

    if actual_types.iter().any(|result| result.is_recovered()) {
        return Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Recovered,
        )));
    }

    let mut applicable = Vec::new();

    let mut has_inaccessible = false;
    let mut has_incompatible = false;
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

        let (key, plan, implementation_selections, trait_operations, _) = candidate.into_parts();

        let context = CandidateContext {
            request,
            types,
            expression,
            kind,
            actual_types: &actual_types,
        };

        match check_candidate(context, plan, &implementation_selections, &trait_operations)? {
            CandidateCheck::Applicable(_operation)
                if state == OperationCandidateState::Inaccessible =>
            {
                has_inaccessible = true;
            }
            CandidateCheck::Applicable(operation) => applicable.push((key, operation)),
            CandidateCheck::Incompatible => has_incompatible = true,
            CandidateCheck::Recovered => has_recovered = true,
        }
    }

    if kind == SelectionKind::Conversion
        && applicable
            .iter()
            .any(|(_, operation)| is_builtin_conversion(operation))
    {
        applicable.retain(|(_, operation)| is_builtin_conversion(operation));
    }

    let selection = match applicable.len() {
        1 => CandidateSelection::Selected(applicable.remove(0).1),
        count if count > 1 => CandidateSelection::Failed(SelectionFailure::Ambiguous(
            applicable.into_iter().map(|(key, _)| key).collect(),
        )),
        _ if has_inaccessible => CandidateSelection::Failed(SelectionFailure::Inaccessible),
        _ if has_recovered => CandidateSelection::Failed(SelectionFailure::Recovered),
        _ if has_incompatible => CandidateSelection::Failed(SelectionFailure::Incompatible),
        _ => CandidateSelection::Failed(SelectionFailure::Unavailable),
    };

    Ok(Some(selection))
}

enum CandidateCheck {
    Applicable(SelectedOperation),
    Incompatible,
    Recovered,
}

#[derive(Clone, Copy)]
struct CandidateContext<'facts, 'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: UnitCheckRequest<'request, C>,
    types: &'facts CheckedExpressionTypes,
    expression: BoundExpressionId,
    kind: SelectionKind,
    actual_types: &'facts [ExpressionTypeResult],
}

fn check_candidate<C>(
    context: CandidateContext<'_, '_, C>,
    plan: OperationCandidatePlan,
    evidence: &[ImplementationSelectionEvidence],
    trait_operations: &[super::super::TraitOperationEvidence],
) -> Result<CandidateCheck, CheckerInfrastructureError>
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
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }

            if !operand_types
                .iter()
                .zip(actual_types)
                .all(|(expected, actual)| *expected == actual.ty())
            {
                return Ok(CandidateCheck::Incompatible);
            }

            operation
        }
        OperationCandidatePlan::Construction {
            target,
            result_type,
            inputs,
        } => {
            if kind != SelectionKind::Construction {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }

            let Some(inputs) =
                map_construction_inputs(request, types, expression, target, &inputs)?
            else {
                return Ok(CandidateCheck::Incompatible);
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

    if !operation_matches_expression(request, expression, &operation)?
        || !operation_is_valid(request, types, expression, &operation)?
    {
        return Ok(CandidateCheck::Incompatible);
    }

    validate_operation_instances(request, &operation)?;

    if !implementation_selections_match(request, &operation, evidence)? {
        return Ok(CandidateCheck::Incompatible);
    }

    if !super::validation::trait_operations_match(
        request,
        &operation,
        actual_types,
        trait_operations,
    )? {
        return Ok(CandidateCheck::Incompatible);
    }

    Ok(CandidateCheck::Applicable(operation))
}

fn validate_request<C>(
    request: UnitCheckRequest<'_, C>,
    types: &CheckedExpressionTypes,
    input: &OperationSelectionRequest,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if input.kind() == SelectionKind::Callable
        || types.unit() != request.view().unit()
        || types.kind() != request.view().kind()
        || !expression_matches_request(request, input)
    {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(())
}

fn expression_matches_request<C>(
    request: UnitCheckRequest<'_, C>,
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
        | (SelectionKind::Operator, BoundExpression::Unary(_) | BoundExpression::Binary(_)) => true,
        (SelectionKind::Index, BoundExpression::Structured(expression)) => matches!(
            expression.kind(),
            BoundStructuredExpressionKind::ElementIndex | BoundStructuredExpressionKind::SliceIndex
        ),
        (SelectionKind::Construction, BoundExpression::StructConstruction(_)) => true,
        (
            SelectionKind::Construction,
            BoundExpression::LeadingDotVariant(_) | BoundExpression::MemberAccess(_),
        ) => true,
        (SelectionKind::Construction, BoundExpression::Call(call)) => {
            union_variant_reference(request, call.callee())
        }
        (SelectionKind::Construction, BoundExpression::Structured(expression)) => {
            expression.kind() == BoundStructuredExpressionKind::TypeFormConstruction
        }
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
        BoundExpression::LeadingDotVariant(_) | BoundExpression::MemberAccess(_)
            if kind == SelectionKind::Construction =>
        {
            Some(Vec::new())
        }
        _ => Some(expression.child_expressions().collect()),
    }
}

fn union_variant_reference<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    match request.view().expression(expression) {
        Some(BoundExpression::LeadingDotVariant(_) | BoundExpression::MemberAccess(_)) => true,
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
    request: UnitCheckRequest<'_, C>,
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
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;
    }

    match operation {
        SelectedOperation::Conversion(conversion) => validate_conversion(request, conversion),
        SelectedOperation::Implementation(witness) => {
            let subject = types
                .expression(expression)
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            Ok(!subject.is_recovered() && witness.requirement().subject() == subject.ty())
        }
        _ => Ok(true),
    }
}

fn operation_matches_expression<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
    operation: &SelectedOperation,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let matches = match (operation, expression) {
        (SelectedOperation::Operator { target, .. }, BoundExpression::Unary(source)) => {
            target.operator() == source.operator()
        }
        (SelectedOperation::Operator { target, .. }, BoundExpression::Binary(source)) => {
            target.operator() == source.operator()
        }
        (SelectedOperation::Index { target, .. }, BoundExpression::Structured(source)) => {
            match source.kind() {
                BoundStructuredExpressionKind::ElementIndex => matches!(
                    target,
                    IndexTarget::ArrayElement
                        | IndexTarget::SliceElement
                        | IndexTarget::Custom { .. }
                ),
                BoundStructuredExpressionKind::SliceIndex => matches!(
                    target,
                    IndexTarget::ArraySlice | IndexTarget::Slice | IndexTarget::Custom { .. }
                ),
                _ => false,
            }
        }
        (SelectedOperation::Construction(construction), BoundExpression::StructConstruction(_)) => {
            matches!(construction.target(), ConstructionTarget::Struct(_))
        }
        (SelectedOperation::Construction(construction), BoundExpression::Structured(source)) => {
            matches!(construction.target(), ConstructionTarget::TypeForm(_))
                && source.kind() == BoundStructuredExpressionKind::TypeFormConstruction
        }
        (
            SelectedOperation::Construction(construction),
            BoundExpression::LeadingDotVariant(_)
            | BoundExpression::MemberAccess(_)
            | BoundExpression::Call(_),
        ) => matches!(construction.target(), ConstructionTarget::UnionVariant(_)),
        (SelectedOperation::Conversion(conversion), BoundExpression::Conversion(source)) => {
            source.target_type() == Some(conversion.target_type())
        }
        (SelectedOperation::Member(_), _) | (SelectedOperation::Implementation(_), _) => true,
        _ => false,
    };

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBinaryExpression, BoundExpression, BoundExpressionId, BoundOperator,
        BoundStructConstructionExpression, BoundStructFieldInitializer, BoundUnit, BoundUnitId,
        CheckedExpressionTypes, ConstructionDefaultProvider, ConstructionInputId,
        ConstructionTarget, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
        MemberTarget, OperatorTarget, SelectedConstructionInput, SelectedOperation, SelectionKind,
    };
    use bray_symbols::{
        CallablePosition, FunctionSymbolId, StructFieldDefaultProviderSymbolId,
        StructFieldSymbolId, StructSymbolId, SymbolId, SymbolKind, TypeId,
    };

    use crate::test_support::{
        TestCheckerContext, callable_entry, declaration_key, expression_unit, push_expression,
        symbol_name, tuple_type,
    };
    use crate::{
        CandidateSelection, ConstructionInputSurface, DefaultSemanticSelector, OperationCandidate,
        OperationCandidateState, OperationSelectionRequest, SelectionCandidateKey,
        SelectionFailure, SemanticSelector, UnitCheckRequest,
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
            CandidateSelection::Failed(SelectionFailure::Incompatible)
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
                },
                SelectedConstructionInput::Default {
                    input: ConstructionInputId::StructField(first),
                    provider: ConstructionDefaultProvider::StructField(default),
                },
            ]
        );
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
            keys.as_ref(),
            [
                SelectionCandidateKey::Symbol(declaration_key(SymbolKind::Function, 1)),
                SelectionCandidateKey::Symbol(declaration_key(SymbolKind::Function, 2)),
            ]
        );
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

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .iter()
                .copied()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        );

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

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .iter()
                .copied()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        );

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

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .iter()
                .copied()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        );

        MemberFixture {
            member: expressions[1],
            receiver: expressions[0],
            unit,
            types,
            value_type,
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
        let input = OperationSelectionRequest::new(
            fixture.operation,
            SelectionKind::Operator,
            fixture.operands,
            candidates,
        );

        select_request(&fixture.unit, &fixture.types, input)
    }

    fn select_request(
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        input: OperationSelectionRequest,
    ) -> bray_diagnostics::DiagnosticResult<CandidateSelection<SelectedOperation>> {
        let entry = callable_entry(unit.key());
        let context = TestCheckerContext::new(false);

        let request = match UnitCheckRequest::new(unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("operation selection request must be valid: {error:?}"),
        };

        match DefaultSemanticSelector.select_operation(request, types, input) {
            crate::CheckerOutcome::Complete(result) => result,
            other => panic!("operation selection must complete: {other:?}"),
        }
    }
}
