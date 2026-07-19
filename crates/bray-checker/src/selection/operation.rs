use bray_bound_tree::{CheckedExpressionTypes, ExpressionTypeResult};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{TypeData, TypeId};

use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::{
    CandidateSelection, ConversionTarget, OperationCandidate, OperationCandidateState,
    OperationSelectionRequest, SelectedOperation, SelectionFailure, SelectionKind,
};

pub(super) fn select<C>(
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

    if !super::order::canonicalize_by_key(&mut candidates, OperationCandidate::key) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let expression_type = expression_type(types, expression)?;
    let actual_types = expression_types(types, &operands)?;

    let mut applicable = Vec::new();

    let mut has_inaccessible = false;
    let mut has_incompatible = false;
    let mut has_recovered =
        expression_type.is_recovered() || actual_types.iter().any(|result| result.is_recovered());

    if has_recovered {
        return Ok(Some(CandidateSelection::Failed(
            SelectionFailure::Recovered,
        )));
    }

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

        if candidate.operation().kind() != kind {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        }

        if candidate.operand_types().len() != actual_types.len()
            || !candidate
                .operand_types()
                .iter()
                .zip(&actual_types)
                .all(|(expected, actual)| *expected == actual.ty())
        {
            has_incompatible = true;
            continue;
        }

        if !operation_is_valid(request, expression, expression_type, &candidate)? {
            has_incompatible = true;
            continue;
        }

        let (key, operation, _, _) = candidate.into_parts();

        if state == OperationCandidateState::Inaccessible {
            has_inaccessible = true;
        } else {
            applicable.push((key, operation));
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
        _ if has_recovered => CandidateSelection::Failed(SelectionFailure::Recovered),
        _ if has_incompatible => CandidateSelection::Failed(SelectionFailure::Incompatible),
        _ if has_inaccessible => CandidateSelection::Failed(SelectionFailure::Inaccessible),
        _ => CandidateSelection::Failed(SelectionFailure::Unavailable),
    };

    Ok(Some(selection))
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
            bray_bound_tree::BoundExpression::MemberAccess(_)
            | bray_bound_tree::BoundExpression::TraitQualifiedMember(_),
        )
        | (
            SelectionKind::Operator,
            bray_bound_tree::BoundExpression::Unary(_)
            | bray_bound_tree::BoundExpression::Binary(_),
        ) => true,
        (SelectionKind::Index, bray_bound_tree::BoundExpression::Structured(expression)) => {
            matches!(
                expression.kind(),
                bray_bound_tree::BoundStructuredExpressionKind::ElementIndex
                    | bray_bound_tree::BoundStructuredExpressionKind::SliceIndex
            )
        }
        (SelectionKind::Construction, bray_bound_tree::BoundExpression::StructConstruction(_)) => {
            true
        }
        (
            SelectionKind::Construction,
            bray_bound_tree::BoundExpression::LeadingDotVariant(_)
            | bray_bound_tree::BoundExpression::MemberAccess(_),
        ) => true,
        (SelectionKind::Construction, bray_bound_tree::BoundExpression::Call(call)) => {
            union_variant_reference(request, call.callee())
        }
        (SelectionKind::Construction, bray_bound_tree::BoundExpression::Structured(expression))
            if expression.kind()
                == bray_bound_tree::BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            true
        }
        (SelectionKind::Conversion, bray_bound_tree::BoundExpression::Conversion(_)) => true,
        (SelectionKind::Implementation, _) => true,
        _ => false,
    };

    if !category_matches {
        return false;
    }

    if input.kind() == SelectionKind::Implementation {
        return input.operands().is_empty();
    }

    match expression {
        bray_bound_tree::BoundExpression::StructConstruction(construction) => construction
            .fields()
            .iter()
            .map(bray_bound_tree::BoundStructFieldInitializer::expression)
            .eq(input.operands().iter().copied()),
        bray_bound_tree::BoundExpression::Call(call)
            if input.kind() == SelectionKind::Construction =>
        {
            call.arguments()
                .iter()
                .map(bray_bound_tree::BoundArgument::expression)
                .eq(input.operands().iter().copied())
        }
        bray_bound_tree::BoundExpression::LeadingDotVariant(_)
        | bray_bound_tree::BoundExpression::MemberAccess(_)
            if input.kind() == SelectionKind::Construction =>
        {
            input.operands().is_empty()
        }
        _ => expression
            .child_expressions()
            .eq(input.operands().iter().copied()),
    }
}

fn union_variant_reference<C>(
    request: UnitCheckRequest<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    match request.view().expression(expression) {
        Some(
            bray_bound_tree::BoundExpression::LeadingDotVariant(_)
            | bray_bound_tree::BoundExpression::MemberAccess(_),
        ) => true,
        Some(bray_bound_tree::BoundExpression::Name(name)) => matches!(
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
    expressions: &[bray_bound_tree::BoundExpressionId],
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

fn expression_type(
    types: &CheckedExpressionTypes,
    expression: bray_bound_tree::BoundExpressionId,
) -> Result<ExpressionTypeResult, CheckerInfrastructureError> {
    types
        .expression(expression)
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)
}

fn operation_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    expression_type: ExpressionTypeResult,
    candidate: &OperationCandidate,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if !operation_matches_expression(request, expression, candidate.operation())? {
        return Ok(false);
    }

    let Some(result_type) = candidate.operation().result_type() else {
        let SelectedOperation::Implementation { requirement, .. } = candidate.operation() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        return Ok(
            candidate.operand_types().is_empty() && requirement.subject() == expression_type.ty()
        );
    };

    if result_type != expression_type.ty() {
        return Ok(false);
    }

    request
        .semantic_values()
        .type_data(result_type)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let SelectedOperation::Conversion {
        target,
        result_type: target_type,
    } = candidate.operation()
    else {
        return Ok(true);
    };

    let [source_type] = candidate.operand_types() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    conversion_is_valid(request, *source_type, *target_type, *target)
}

fn operation_matches_expression<C>(
    request: UnitCheckRequest<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    operation: &SelectedOperation,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let matches = match (operation, expression) {
        (
            SelectedOperation::Operator {
                target: super::OperatorTarget::BuiltIn(target),
                ..
            },
            bray_bound_tree::BoundExpression::Unary(source),
        ) => *target == source.operator(),
        (
            SelectedOperation::Operator {
                target: super::OperatorTarget::BuiltIn(target),
                ..
            },
            bray_bound_tree::BoundExpression::Binary(source),
        ) => *target == source.operator(),
        (
            SelectedOperation::Index { target, .. },
            bray_bound_tree::BoundExpression::Structured(source),
        ) => match source.kind() {
            bray_bound_tree::BoundStructuredExpressionKind::ElementIndex => matches!(
                target,
                super::IndexTarget::ArrayElement
                    | super::IndexTarget::SliceElement
                    | super::IndexTarget::Custom(_)
            ),
            bray_bound_tree::BoundStructuredExpressionKind::SliceIndex => matches!(
                target,
                super::IndexTarget::ArraySlice
                    | super::IndexTarget::Slice
                    | super::IndexTarget::Custom(_)
            ),
            _ => false,
        },
        (
            SelectedOperation::Construction {
                target: super::ConstructionTarget::Struct(_),
                ..
            },
            bray_bound_tree::BoundExpression::StructConstruction(_),
        ) => true,
        (
            SelectedOperation::Construction {
                target: super::ConstructionTarget::TypeForm(_),
                ..
            },
            bray_bound_tree::BoundExpression::Structured(source),
        ) if source.kind()
            == bray_bound_tree::BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            true
        }
        (
            SelectedOperation::Construction {
                target: super::ConstructionTarget::UnionVariant(_),
                ..
            },
            bray_bound_tree::BoundExpression::LeadingDotVariant(_)
            | bray_bound_tree::BoundExpression::MemberAccess(_)
            | bray_bound_tree::BoundExpression::Call(_),
        ) => true,
        (
            SelectedOperation::Conversion { result_type, .. },
            bray_bound_tree::BoundExpression::Conversion(source),
        ) => source.target_type() == Some(*result_type),
        (SelectedOperation::Member(_), _)
        | (
            SelectedOperation::Operator {
                target: super::OperatorTarget::Trait { .. },
                ..
            },
            _,
        )
        | (SelectedOperation::Implementation { .. }, _) => true,
        _ => false,
    };

    Ok(matches)
}

fn conversion_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
    conversion: ConversionTarget,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    match conversion {
        ConversionTarget::Identity => Ok(source == target),
        ConversionTarget::BuiltInScalar => scalar_conversion_is_valid(request, source, target),
        ConversionTarget::BuiltInComposite => {
            composite_conversion_is_valid(request, source, target)
        }
        ConversionTarget::Trait { .. } => Ok(source != target),
    }
}

fn scalar_conversion_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(source) = type_representation(request, source)? else {
        return Ok(false);
    };

    let Some(target) = type_representation(request, target)? else {
        return Ok(false);
    };

    Ok(scalar_shape(source).is_some_and(|source| {
        scalar_shape(target).is_some_and(|target| source.can_represent(target))
    }))
}

fn composite_conversion_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source_data = request
        .semantic_values()
        .type_data(source)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let target_data = request
        .semantic_values()
        .type_data(target)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    match (source_data.as_ref(), target_data.as_ref()) {
        (TypeData::Tuple(source), TypeData::Tuple(target)) if source.len() == target.len() => {
            recursively_convertible(request, source.iter().copied().zip(target.iter().copied()))
        }
        (
            TypeData::Array {
                element: source_element,
                length: source_length,
            },
            TypeData::Array {
                element: target_element,
                length: target_length,
            },
        ) if source_length == target_length => {
            conversion_exists(request, *source_element, *target_element)
        }
        (TypeData::Nullable(source), TypeData::Nullable(target)) => {
            conversion_exists(request, *source, *target)
        }
        (TypeData::Tuple(elements), _) if elements.len() == 2 => {
            tuple_to_complex_is_valid(request, elements, target)
        }
        _ => Ok(false),
    }
}

fn recursively_convertible<C>(
    request: UnitCheckRequest<'_, C>,
    pairs: impl IntoIterator<Item = (TypeId, TypeId)>,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for (source, target) in pairs {
        if !conversion_exists(request, source, target)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn conversion_exists<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if source == target || scalar_conversion_is_valid(request, source, target)? {
        return Ok(true);
    }

    composite_conversion_is_valid(request, source, target)
}

fn tuple_to_complex_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    elements: &[TypeId],
    target: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(target_role) = type_representation(request, target)? else {
        return Ok(false);
    };

    let component = match target_role {
        RepresentationRole::ScalarC32 => RepresentationRole::ScalarR16,
        RepresentationRole::ScalarC64 => RepresentationRole::ScalarR32,
        RepresentationRole::ScalarC128 => RepresentationRole::ScalarR64,
        RepresentationRole::ScalarC256 => RepresentationRole::ScalarR128,
        _ => return Ok(false),
    };

    let component = crate::representation::representation_type(request, component)?;

    recursively_convertible(
        request,
        elements.iter().copied().map(|element| (element, component)),
    )
}

#[derive(Clone, Copy)]
enum ScalarShape {
    Signed(u16),
    Unsigned(u16),
    Real(u16),
    Complex(u16),
    TargetSigned,
    TargetUnsigned,
}

impl ScalarShape {
    const fn can_represent(self, target: Self) -> bool {
        match (self, target) {
            (Self::Signed(source), Self::Signed(target)) => source <= target,
            (Self::Unsigned(source), Self::Unsigned(target)) => source <= target,
            (Self::Unsigned(source), Self::Signed(target)) => source < target,
            (Self::Signed(source), Self::Real(mantissa)) => source - 1 <= mantissa,
            (Self::Unsigned(source), Self::Real(mantissa)) => source <= mantissa,
            (Self::Real(source), Self::Real(target)) => source <= target,
            (Self::Complex(source), Self::Complex(target)) => source <= target,
            (Self::TargetSigned, Self::TargetSigned)
            | (Self::TargetUnsigned, Self::TargetUnsigned) => true,
            _ => false,
        }
    }
}

const fn scalar_shape(role: RepresentationRole) -> Option<ScalarShape> {
    match role {
        RepresentationRole::ScalarI8 => Some(ScalarShape::Signed(8)),
        RepresentationRole::ScalarI16 => Some(ScalarShape::Signed(16)),
        RepresentationRole::ScalarI32 => Some(ScalarShape::Signed(32)),
        RepresentationRole::ScalarI64 => Some(ScalarShape::Signed(64)),
        RepresentationRole::ScalarI128 => Some(ScalarShape::Signed(128)),
        RepresentationRole::ScalarU8 => Some(ScalarShape::Unsigned(8)),
        RepresentationRole::ScalarU16 => Some(ScalarShape::Unsigned(16)),
        RepresentationRole::ScalarU32 => Some(ScalarShape::Unsigned(32)),
        RepresentationRole::ScalarU64 => Some(ScalarShape::Unsigned(64)),
        RepresentationRole::ScalarU128 => Some(ScalarShape::Unsigned(128)),
        RepresentationRole::ScalarIsize => Some(ScalarShape::TargetSigned),
        RepresentationRole::ScalarUsize => Some(ScalarShape::TargetUnsigned),
        RepresentationRole::ScalarR16 => Some(ScalarShape::Real(11)),
        RepresentationRole::ScalarR32 => Some(ScalarShape::Real(24)),
        RepresentationRole::ScalarR64 => Some(ScalarShape::Real(53)),
        RepresentationRole::ScalarR128 => Some(ScalarShape::Real(113)),
        RepresentationRole::ScalarC32 => Some(ScalarShape::Complex(11)),
        RepresentationRole::ScalarC64 => Some(ScalarShape::Complex(24)),
        RepresentationRole::ScalarC128 => Some(ScalarShape::Complex(53)),
        RepresentationRole::ScalarC256 => Some(ScalarShape::Complex(113)),
        _ => None,
    }
}

const fn is_builtin_conversion(operation: &SelectedOperation) -> bool {
    matches!(
        operation,
        SelectedOperation::Conversion {
            target: ConversionTarget::Identity
                | ConversionTarget::BuiltInScalar
                | ConversionTarget::BuiltInComposite,
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBinaryExpression, BoundConversionExpression, BoundExpression, BoundExpressionId,
        BoundMemberAccessExpression, BoundMemberSelector, BoundOperator, BoundUnit, BoundUnitId,
        CheckedExpressionTypes, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
    };
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticSelectionKind};
    use bray_symbols::{FunctionSymbolId, SymbolId, SymbolKind, TypeId};

    use crate::test_support::{
        TestCheckerContext, callable_entry, declaration_key, expression_unit, push_expression,
        symbol_name, tuple_type,
    };
    use crate::{
        CandidateSelection, ConversionTarget, DefaultSemanticSelector, MemberTarget,
        OperationCandidate, OperationCandidateState, OperationSelectionRequest, OperatorTarget,
        SelectedOperation, SelectionCandidateKey, SelectionFailure, SelectionKind,
        SemanticSelector, UnitCheckRequest,
    };

    #[test]
    fn exact_operator_candidates_publish_their_selected_target() {
        let fixture = operator_fixture(BoundUnitId::new(80));

        let candidate = OperationCandidate::built_in(
            SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Add),
                result_type: fixture.value_type,
            },
            [fixture.value_type, fixture.value_type],
            OperationCandidateState::Available,
        );

        let input = OperationSelectionRequest::new(
            fixture.operation,
            SelectionKind::Operator,
            fixture.operands,
            [candidate],
        );

        let result = select(&fixture.unit, &fixture.types, input);

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result.value(),
            &CandidateSelection::Selected(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(BoundOperator::Add),
                result_type: fixture.value_type,
            })
        );
    }

    #[test]
    fn applicable_candidates_are_ambiguous_without_ranking() {
        let fixture = member_fixture(BoundUnitId::new(81));

        let input = OperationSelectionRequest::new(
            fixture.member,
            SelectionKind::Member,
            [fixture.receiver],
            [
                member_candidate(2, fixture.value_type),
                member_candidate(1, fixture.value_type),
            ],
        );

        let result = select(&fixture.unit, &fixture.types, input);

        let CandidateSelection::Failed(SelectionFailure::Ambiguous(keys)) = result.value() else {
            panic!("two exact applicable candidates must remain ambiguous");
        };

        assert_eq!(
            keys.as_ref(),
            [
                SelectionCandidateKey::Symbol(declaration_key(SymbolKind::Function, 1)),
                SelectionCandidateKey::Symbol(declaration_key(SymbolKind::Function, 2)),
            ]
        );

        assert_eq!(
            result.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::CheckingAmbiguousCandidate
        );
    }

    #[test]
    fn incompatible_operation_candidates_emit_structured_category_diagnostics() {
        let fixture = operator_fixture(BoundUnitId::new(82));
        let other_type = tuple_type([fixture.value_type]);

        let input = OperationSelectionRequest::new(
            fixture.operation,
            SelectionKind::Operator,
            fixture.operands,
            [OperationCandidate::built_in(
                SelectedOperation::Operator {
                    target: OperatorTarget::BuiltIn(BoundOperator::Add),
                    result_type: fixture.value_type,
                },
                [other_type, other_type],
                OperationCandidateState::Available,
            )],
        );

        let result = select(&fixture.unit, &fixture.types, input);
        let diagnostic = &result.diagnostics().diagnostics()[0];

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible)
        ));

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingIncompatibleCandidate
        );

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::selection_kind(
                DiagnosticSelectionKind::Operator
            )]
        );
    }

    #[test]
    fn identity_conversion_requires_identical_source_and_target_types() {
        let source_type = tuple_type([]);
        let target_type = tuple_type([source_type]);

        let valid = conversion_fixture(BoundUnitId::new(83), source_type, source_type);
        let input = conversion_request(&valid, ConversionTarget::Identity);
        let result = select(&valid.unit, &valid.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Selected(SelectedOperation::Conversion {
                target: ConversionTarget::Identity,
                result_type,
            }) if *result_type == source_type
        ));

        let invalid = conversion_fixture(BoundUnitId::new(84), source_type, target_type);
        let input = conversion_request(&invalid, ConversionTarget::Identity);
        let result = select(&invalid.unit, &invalid.types, input);

        assert!(matches!(
            result.value(),
            CandidateSelection::Failed(SelectionFailure::Incompatible)
        ));
    }

    struct OperatorFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        operation: BoundExpressionId,
        operands: [BoundExpressionId; 2],
        value_type: TypeId,
    }

    struct ConversionFixture {
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        conversion: BoundExpressionId,
        source: BoundExpressionId,
        source_type: TypeId,
        target_type: TypeId,
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

    fn conversion_fixture(
        unit: BoundUnitId,
        source_type: TypeId,
        target_type: TypeId,
    ) -> ConversionFixture {
        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            let source = push_expression(
                tree,
                crate::test_support::integer_literal_expression(origin, Some(source_type)),
            );

            let conversion = push_expression(
                tree,
                BoundExpression::Conversion(BoundConversionExpression::new(
                    origin,
                    source,
                    origin.source_anchor().syntax(),
                    Some(target_type),
                    None,
                    false,
                )),
            );

            vec![source, conversion]
        });

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [
                ExpressionTypeEntry::new(
                    expressions[0],
                    ExpressionTypeResult::new(source_type, ExpressionTypeStatus::Valid),
                ),
                ExpressionTypeEntry::new(
                    expressions[1],
                    ExpressionTypeResult::new(target_type, ExpressionTypeStatus::Valid),
                ),
            ],
        );

        ConversionFixture {
            conversion: expressions[1],
            source: expressions[0],
            unit,
            types,
            source_type,
            target_type,
        }
    }

    fn conversion_request(
        fixture: &ConversionFixture,
        target: ConversionTarget,
    ) -> OperationSelectionRequest {
        OperationSelectionRequest::new(
            fixture.conversion,
            SelectionKind::Conversion,
            [fixture.source],
            [OperationCandidate::built_in(
                SelectedOperation::Conversion {
                    target,
                    result_type: fixture.target_type,
                },
                [fixture.source_type],
                OperationCandidateState::Available,
            )],
        )
    }

    fn member_fixture(unit: BoundUnitId) -> MemberFixture {
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

        let outcome = DefaultSemanticSelector.select_operation(request, types, input);

        match outcome {
            crate::CheckerOutcome::Complete(result) => result,
            other => panic!("operation selection must complete: {other:?}"),
        }
    }
}
