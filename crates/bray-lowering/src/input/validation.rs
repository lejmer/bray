use super::model::{LoweringInputError, LoweringInputKind};
use crate::plan::dependency_subject_exists;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnit, BoundUnitId, BoundUnitKind, CheckedExpressionTypes, CheckedLiteralValues,
    CheckedPatterns, CheckedRefinements, CheckedSemanticSelections, Liveness, RefinementKind,
    StorageAccessPlan, StorageAccessPurpose, StorageFlow, StorageOperationDecision, StoragePlan,
};
use bray_ir::MirTargetContract;
use bray_symbols::{AnySymbolId, ConstantValueId, SemanticValueStore};

pub(super) fn validate_literal_target(
    literals: &CheckedLiteralValues,
    target: &MirTargetContract,
) -> Result<(), LoweringInputError> {
    let expected = target.machine().pointer_width_bits();
    let actual = literals.target_integer_width_bits();

    if actual != expected {
        return Err(LoweringInputError::LiteralTargetWidthMismatch { expected, actual });
    }

    Ok(())
}

pub(super) fn validate_literal_values(
    literals: &CheckedLiteralValues,
    values: &SemanticValueStore,
) -> Result<(), LoweringInputError> {
    for entry in literals.entries() {
        values.constant_value_data(entry.value())?;
    }

    Ok(())
}

pub(super) fn validate_constant_reference_values(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    values: &SemanticValueStore,
    references: &[(BoundExpressionId, ConstantValueId)],
) -> Result<(), LoweringInputError> {
    if references.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(LoweringInputError::InvalidInputContents(
            LoweringInputKind::ConstantReferences,
        ));
    }

    for (expression, value) in references {
        let Some(BoundExpression::Name(name)) = unit.view().expression(*expression) else {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        };

        if !matches!(
            name.target(),
            BoundReferenceTarget::Surface(
                AnySymbolId::Constant(_)
                    | AnySymbolId::TraitConstantMember(_)
                    | AnySymbolId::TraitConstantFulfillment(_)
            )
        ) {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        }

        let Some(expression_type) = types.expression(*expression) else {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        };

        let value_type = values.constant_value_data(*value)?.ty();

        if expression_type.ty() != value_type {
            return Err(LoweringInputError::InvalidInputContents(
                LoweringInputKind::ConstantReferences,
            ));
        }
    }

    Ok(())
}

pub(super) fn validate_semantic_completeness(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
) -> Result<(), LoweringInputError> {
    for (expression, node) in unit.tree().expressions() {
        if !requires_expression_type(node) {
            continue;
        }

        let Some(result) = types.expression(expression) else {
            return Err(LoweringInputError::MissingExpressionType(expression));
        };

        if result.is_recovered() {
            continue;
        }

        if requires_semantic_selection(node, expression, storage)
            && selections.expression(expression).is_none()
        {
            return Err(LoweringInputError::MissingSemanticSelection(expression));
        }
    }

    Ok(())
}

const fn requires_expression_type(expression: &BoundExpression) -> bool {
    match expression {
        BoundExpression::Name(expression) => expression.ty().is_some(),
        BoundExpression::MemberAccess(expression) => expression.ty().is_some(),
        _ => true,
    }
}

pub(super) fn validate_pattern_completeness(
    unit: &BoundUnit,
    analysis: &CheckedPatterns,
) -> Result<(), LoweringInputError> {
    let patterns_match = unit
        .tree()
        .patterns()
        .map(|(pattern, _)| pattern)
        .eq(analysis.patterns().iter().map(|entry| entry.pattern()));

    let bindings_match = patterns_match && {
        let mut introduced = Vec::new();

        for ((_, pattern), checked) in unit.tree().patterns().zip(analysis.patterns()) {
            introduced.extend(checked.bindings(pattern));
        }

        introduced.sort_unstable();
        introduced.dedup();

        introduced
            .into_iter()
            .eq(analysis.binding_types().iter().map(|entry| entry.binding()))
    };

    let matches_match = unit
        .tree()
        .expressions()
        .filter_map(|(expression, node)| {
            matches!(node, BoundExpression::Match(_)).then_some(expression)
        })
        .eq(analysis.matches().iter().map(|entry| entry.expression()));

    if !patterns_match || !bindings_match || !matches_match {
        return Err(LoweringInputError::InvalidPatternInput);
    }

    Ok(())
}

pub(super) fn validate_liveness(
    unit: &BoundUnit,
    storage: &StoragePlan,
    analysis: &Liveness,
) -> Result<(), LoweringInputError> {
    let valid_last_uses = analysis.last_uses().iter().all(|entry| {
        unit.view().node_is_recovered(entry.operation()).is_some()
            && dependency_subject_exists(unit, storage, entry.subject())
    });

    let valid_scopes = analysis.live_across_scopes().iter().all(|entry| {
        unit.view().block(entry.scope()).is_some()
            && dependency_subject_exists(unit, storage, entry.subject())
    });

    let valid_suspensions = analysis.live_across_suspensions().iter().all(|entry| {
        unit.view().expression(entry.await_expression()).is_some()
            && dependency_subject_exists(unit, storage, entry.subject())
    });

    if !valid_last_uses || !valid_scopes || !valid_suspensions {
        return Err(LoweringInputError::InvalidInputContents(
            LoweringInputKind::LoweringPlans,
        ));
    }

    Ok(())
}

pub(super) fn validate_refinements(
    unit: &BoundUnit,
    storage: &StoragePlan,
    analysis: &CheckedRefinements,
) -> Result<(), LoweringInputError> {
    let valid = analysis.occurrences().iter().all(|occurrence| {
        unit.view().node_is_recovered(occurrence.node()).is_some()
            && occurrence.refinements().iter().all(|input| {
                refinement_kind_exists(unit, storage, input.kind())
                    && input
                        .dependencies()
                        .iter()
                        .all(|access| storage.access(*access).is_some())
            })
    });

    if !valid {
        return Err(LoweringInputError::InvalidInputContents(
            LoweringInputKind::Refinements,
        ));
    }

    Ok(())
}

fn refinement_kind_exists(unit: &BoundUnit, storage: &StoragePlan, kind: RefinementKind) -> bool {
    match kind {
        RefinementKind::Condition { expression, .. }
        | RefinementKind::NullablePresence { expression, .. }
        | RefinementKind::TrustBoundary(expression)
        | RefinementKind::NormalCompletion(expression) => {
            unit.view().expression(expression).is_some()
        }
        RefinementKind::Pattern {
            subject,
            pattern,
            access,
            ..
        } => {
            unit.view().expression(subject).is_some()
                && unit.view().pattern(pattern).is_some()
                && storage.access(access).is_some()
                && matches!(storage.binding(bray_bound_tree::StorageBindingTarget::PatternSubject(pattern)),
                Some(bray_bound_tree::StorageBinding::Access(original))
                    if storage.relationship(original, access) == bray_bound_tree::StorageRelationship::Identical)
        }
    }
}

pub(super) fn validate_storage_analysis(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlow,
) -> Result<(), LoweringInputError> {
    let plans = storage.access_plans();
    let decisions = flow.operations();

    if plans.len() != decisions.len() {
        return Err(LoweringInputError::StorageOperationCountMismatch {
            expected: plans.len(),
            actual: decisions.len(),
        });
    }

    for (plan, decision) in plans.iter().copied().zip(decisions.iter().copied()) {
        let plan_matches = plan.node() == decision.node()
            && plan.expression() == decision.expression()
            && plan.access() == decision.access()
            && plan.purpose().matches_checked(decision.purpose());

        let has_expression = unit.view().expression(decision.expression()).is_some();
        let has_node = unit.view().node_is_recovered(decision.node()).is_some();

        let has_access = storage.access(decision.access()).is_some();

        let borrow_matches = storage_borrow_matches(storage, plan, decision);

        if !plan_matches || !has_expression || !has_node || !has_access || !borrow_matches {
            return Err(LoweringInputError::InvalidStorageOperation(
                decision.expression(),
            ));
        }
    }

    for exit in flow.exits() {
        let has_scope = unit.view().block(exit.scope()).is_some();

        let has_identities = exit
            .initialized()
            .iter()
            .all(|identity| storage.identity(*identity).is_some());

        let has_accesses = exit
            .moved()
            .iter()
            .all(|access| storage.access(*access).is_some());

        let has_borrows = exit
            .active_borrows()
            .iter()
            .all(|borrow| storage.borrow_capability(*borrow).is_some());

        if !has_scope || !has_identities || !has_accesses || !has_borrows {
            return Err(LoweringInputError::InvalidStorageExit(exit.scope()));
        }
    }

    Ok(())
}

fn storage_borrow_matches(
    storage: &StoragePlan,
    plan: StorageAccessPlan,
    decision: StorageOperationDecision,
) -> bool {
    let StorageAccessPurpose::Borrow(kind) = plan.purpose() else {
        return decision.borrow().is_none();
    };

    let inherited = storage
        .access(plan.access())
        .and_then(|access| access.root().borrow_capability())
        .filter(|borrow| {
            storage
                .borrow_capability(*borrow)
                .is_some_and(|capability| capability.kind() == kind)
        });

    let created = storage
        .borrow_capability_entries()
        .find(|(_, capability)| {
            capability.expression() == Some(plan.expression())
                && capability.kind() == kind
                && capability.access() == plan.access()
        })
        .map(|(borrow, _)| borrow);

    decision.borrow() == created.or(inherited)
}

fn requires_semantic_selection(
    expression: &BoundExpression,
    id: BoundExpressionId,
    storage: &StoragePlan,
) -> bool {
    match expression {
        BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Call(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_)
        | BoundExpression::BoxConstruction(_)
        | BoundExpression::For(_)
        | BoundExpression::Generator(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::TraitQualifiedMember(_)
        | BoundExpression::PatternReference(_) => true,
        BoundExpression::MemberAccess(expression) => {
            !matches!(
                expression.selector(),
                Some(bray_bound_tree::BoundMemberSelector::TupleElement(_))
            ) && storage.expression_plans(id).next().is_none()
        }
        BoundExpression::Structured(expression) => matches!(
            expression.kind(),
            BoundStructuredExpressionKind::ElementIndex
                | BoundStructuredExpressionKind::SliceIndex
                | BoundStructuredExpressionKind::BooleanAllFold
                | BoundStructuredExpressionKind::BooleanAnyFold
                | BoundStructuredExpressionKind::NullablePropagation
                | BoundStructuredExpressionKind::ResultPropagation
        ),
        BoundExpression::Block(_)
        | BoundExpression::Literal(_)
        | BoundExpression::Name(_)
        | BoundExpression::UnresolvedReference(_)
        | BoundExpression::Assignment(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::AnonymousCallable(_)
        | BoundExpression::Await(_)
        | BoundExpression::ControlTransfer(_)
        | BoundExpression::Match(_)
        | BoundExpression::Error(_) => false,
    }
}

pub(super) fn validate_input_owner(
    unit: &BoundUnit,
    actual_unit: BoundUnitId,
    actual_kind: BoundUnitKind,
    input: LoweringInputKind,
) -> Result<(), LoweringInputError> {
    if actual_unit != unit.unit() {
        return Err(LoweringInputError::ForeignInput {
            input,
            expected: unit.unit(),
            actual: actual_unit,
        });
    }

    if actual_kind != unit.key().kind() {
        return Err(LoweringInputError::InputKindMismatch {
            input,
            expected: unit.key().kind(),
            actual: actual_kind,
        });
    }

    Ok(())
}
