use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundCallableBodyKind, BoundExpression, BoundExpressionId,
    BoundReferenceTarget, BoundUnit, BoundUnitRoot, BoundWalkControl, BoundWalkEvent,
    CheckedExpressionTypes, CheckedSemanticSelections, CheckedTemplateOperation,
    ExpressionTypeEntry, ExpressionTypeResult, SelectedOperation, SemanticSelection,
    walk_bound_unit_view,
};
use bray_checker::ConstantReferenceResolution;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, ConstantDefinition, ConstantDefinitionState,
    ConstantInstanceKey, ConstantValueId, GenericSubstitutionId,
};

use crate::compilation::substitution::empty_substitution;
use crate::fact::FactQueryError;

pub(super) fn imported_constant_definition(
    definition: AnyConstantDefinitionId,
    template: Option<&bray_package_interface::ImportedDeclarationTemplate>,
) -> Result<ConstantDefinitionState, FactQueryError> {
    let Some(template) = template else {
        return match definition {
            AnyConstantDefinitionId::TraitMember(_) => Ok(ConstantDefinitionState::Required),
            AnyConstantDefinitionId::Constant(_) | AnyConstantDefinitionId::TraitFulfillment(_) => {
                Err(FactQueryError::InfrastructureFailure)
            }
        };
    };

    let template = template.template();

    let index = usize::try_from(template.result().raw())
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let result = template
        .nodes()
        .get(index)
        .ok_or(FactQueryError::InfrastructureFailure)?;

    let CheckedTemplateOperation::Constant { term, .. } = result.operation() else {
        return Err(FactQueryError::InfrastructureFailure);
    };

    Ok(ConstantDefinitionState::Defined(ConstantDefinition::new(
        result.ty(),
        *term,
    )))
}

pub(in crate::compilation::constant) fn call_parameter_values(
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
            .map_err(FactQueryError::SemanticValueStore)?;

        if actual.ty() != expected {
            return Err(FactQueryError::InfrastructureFailure);
        }

        resolved.insert(parameter, value);
    }

    Ok(resolved)
}

pub(super) struct ConcreteReferenceContext<'parameters> {
    pub(super) substitution: Option<GenericSubstitutionId>,
    pub(super) selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
    pub(super) parameters: &'parameters BTreeMap<AnySymbolId, ConstantValueId>,
    pub(super) current_constant: Option<ConstantInstanceKey>,
}

pub(in crate::compilation::constant) fn constant_callable_root(
    bound: &BoundUnit,
) -> Option<bray_bound_tree::BoundBlockId> {
    let BoundUnitRoot::CallableBody { body, .. } = bound.root() else {
        return None;
    };

    let body = bound.view().callable_body(body)?;

    match body.kind() {
        BoundCallableBodyKind::Block(block) => Some(block),
        BoundCallableBodyKind::Error(_) => None,
    }
}

pub(in crate::compilation) fn collect_constant_references(
    bound: &BoundUnit,
    selections: &CheckedSemanticSelections,
    resolve: impl FnMut(
        BoundExpressionId,
        BoundReferenceTarget,
    ) -> Result<ConstantReferenceResolution, FactQueryError>,
) -> Result<Vec<(BoundExpressionId, ConstantReferenceResolution)>, FactQueryError> {
    collect_constant_references_from(bound, selections, bound.root(), resolve)
}

pub(in crate::compilation) fn collect_constant_references_from(
    bound: &BoundUnit,
    selections: &CheckedSemanticSelections,
    root: impl Into<AnyBoundNodeId>,
    mut resolve: impl FnMut(
        BoundExpressionId,
        BoundReferenceTarget,
    ) -> Result<ConstantReferenceResolution, FactQueryError>,
) -> Result<Vec<(BoundExpressionId, ConstantReferenceResolution)>, FactQueryError> {
    let mut references = Vec::new();
    let mut non_value_heads = BTreeSet::new();
    let mut failure = None;

    let outcome = walk_bound_unit_view(bound.view(), root, |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        let Some(bound_expression) = bound.view().expression(expression) else {
            return BoundWalkControl::Continue;
        };

        if let BoundExpression::StructConstruction(construction) = bound_expression
            && let Some(head) = construction.head()
        {
            non_value_heads.insert(head);

            return BoundWalkControl::Continue;
        }

        match bound_expression {
            BoundExpression::Call(call) => {
                non_value_heads.insert(call.callee());

                return BoundWalkControl::Continue;
            }
            BoundExpression::ErrorCall(call) => {
                non_value_heads.insert(call.callee());

                return BoundWalkControl::Continue;
            }
            _ => {}
        }

        if non_value_heads.contains(&expression) {
            return BoundWalkControl::Continue;
        }

        let target = match bound_expression {
            BoundExpression::Name(name) => match name.target() {
                BoundReferenceTarget::Surface(AnySymbolId::GenericConstParameter(_)) => {
                    Some(name.target())
                }
                BoundReferenceTarget::Surface(
                    AnySymbolId::CallableParameter(_) | AnySymbolId::ReceiverParameter(_),
                ) => Some(name.target()),
                BoundReferenceTarget::Surface(symbol)
                    if constant_definition_id(symbol).is_some() =>
                {
                    Some(name.target())
                }
                BoundReferenceTarget::Surface(_) | BoundReferenceTarget::Local(_) => None,
            },
            BoundExpression::MemberAccess(_) => match selections.expression(expression) {
                Some(SemanticSelection::Operation(SelectedOperation::Member(member))) => {
                    match member.member() {
                        AnySymbolId::Constant(constant) => {
                            Some(BoundReferenceTarget::Surface(constant.into()))
                        }
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };

        let Some(target) = target else {
            return BoundWalkControl::Continue;
        };

        match resolve(expression, target) {
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

pub(in crate::compilation::constant) fn substitute_expression_types(
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
                .map_err(FactQueryError::SemanticValueStore)?;

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

pub(super) fn expression_root(bound: &BoundUnit) -> Result<BoundExpressionId, FactQueryError> {
    match bound.root() {
        BoundUnitRoot::Expression(root) => Ok(root),
        BoundUnitRoot::CallableBody { .. }
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

pub(in crate::compilation) fn empty_concrete_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnyConstantDefinitionId,
) -> Result<bray_symbols::ConcreteGenericSubstitutionId, FactQueryError> {
    let substitution = empty_substitution(values, definition.into_any())?;

    values
        .require_concrete_substitution(substitution)
        .map_err(FactQueryError::SemanticValueStore)
}

pub(super) const fn selected_implementation_for_reference(
    definition: AnyConstantDefinitionId,
    selected_implementation: Option<bray_symbols::ImplementationInstanceId>,
) -> Option<bray_symbols::ImplementationInstanceId> {
    match definition {
        AnyConstantDefinitionId::TraitMember(_) => selected_implementation,
        AnyConstantDefinitionId::Constant(_) | AnyConstantDefinitionId::TraitFulfillment(_) => None,
    }
}
