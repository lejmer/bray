use bray_binder::{BindingQueryError, BindingQueryResult};
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, BoundUnitRoot, CheckedBodySemantics, CheckedExpressionSemantics,
    StoragePlan,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{DependencyContractTemplateId, TypeId};

use super::dependency::portable_dependency_contract;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::PublishedUnitResult;

pub(in crate::compilation::binder::symbol) struct CheckedSourceExpression {
    pub(in crate::compilation::binder::symbol) result: TypeId,
    pub(in crate::compilation::binder::symbol) dependency_contract: DependencyContractTemplateId,
    pub(in crate::compilation::binder::symbol) diagnostics: DiagnosticBag,
    pub(in crate::compilation::binder::symbol) is_recovered: bool,
}

pub(in crate::compilation::binder::symbol) struct CheckedSourcePredicateSequence {
    pub(in crate::compilation::binder::symbol) dependency_contracts:
        Vec<DependencyContractTemplateId>,
    pub(in crate::compilation::binder::symbol) diagnostics: DiagnosticBag,
}

pub(in crate::compilation::binder::symbol) fn checked_source_expression(
    context: &CompilationBindingContext<'_>,
    key: BoundUnitKey,
) -> BindingQueryResult<CheckedSourceExpression> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let BoundUnitRoot::Expression(root) = bound.result().value().root() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage = compilation
        .storage_plan_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let body = compilation
        .body_semantics_with_cancellation(key, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let result = semantics
        .result()
        .value()
        .types()
        .expression(root)
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let expression = bound
        .result()
        .value()
        .view()
        .expression(root)
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let dependencies = body.result().value().dependencies();

    let contract = dependencies
        .expression(root)
        .and_then(|contract| dependencies.contract(contract))
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let dependency_contract =
        portable_dependency_contract(context, storage.result().value(), contract)?;

    let diagnostics = checked_source_diagnostics(&bound, &semantics, &storage, &body);

    Ok(CheckedSourceExpression {
        result: result.ty(),
        dependency_contract,
        diagnostics,
        is_recovered: expression.is_recovered()
            || result.is_recovered()
            || dependencies.is_recovered(),
    })
}

pub(in crate::compilation::binder::symbol) fn checked_source_predicate_sequence(
    context: &CompilationBindingContext<'_>,
    key: BoundUnitKey,
) -> BindingQueryResult<CheckedSourcePredicateSequence> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let BoundUnitRoot::ExpressionSequence(root) = bound.result().value().root() else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage = compilation
        .storage_plan_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let body = compilation
        .body_semantics_with_cancellation(key, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let dependencies = body.result().value().dependencies();

    let block = bound.result().value().view().block(root);

    let Some(block) = block else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let mut dependency_contracts = Vec::new();

    for expression in block.items().iter().filter_map(|item| item.expression()) {
        let contract = dependencies
            .expression(expression)
            .and_then(|contract| dependencies.contract(contract));

        let Some(contract) = contract else {
            return Err(BindingQueryError::DependencyUnavailable);
        };

        dependency_contracts.push(portable_dependency_contract(
            context,
            storage.result().value(),
            contract,
        )?);
    }

    let diagnostics = checked_source_diagnostics(&bound, &semantics, &storage, &body);

    Ok(CheckedSourcePredicateSequence {
        dependency_contracts,
        diagnostics,
    })
}

fn checked_source_diagnostics(
    bound: &PublishedUnitResult<BoundUnit>,
    semantics: &PublishedUnitResult<CheckedExpressionSemantics>,
    storage: &PublishedUnitResult<StoragePlan>,
    body: &PublishedUnitResult<CheckedBodySemantics>,
) -> DiagnosticBag {
    DiagnosticBag::merged_all([
        bound.result().diagnostics(),
        semantics.result().diagnostics(),
        storage.result().diagnostics(),
        body.result().diagnostics(),
    ])
}

pub(in crate::compilation::binder::symbol) fn checked_source_body_dependency_contracts(
    context: &CompilationBindingContext<'_>,
    key: BoundUnitKey,
) -> BindingQueryResult<Vec<DependencyContractTemplateId>> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage = compilation
        .storage_plan_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let body = compilation
        .body_semantics_with_cancellation(key, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let dependencies = body.result().value().dependencies();

    let mut contracts = Vec::new();

    for (expression, _) in bound.result().value().tree().expressions() {
        let Some(contract) = dependencies
            .expression(expression)
            .and_then(|contract| dependencies.contract(contract))
        else {
            continue;
        };

        contracts.push(portable_dependency_contract(
            context,
            storage.result().value(),
            contract,
        )?);
    }

    contracts.sort_unstable();
    contracts.dedup();

    Ok(contracts)
}

pub(super) fn syntax_diagnostics(
    context: &CompilationBindingContext<'_>,
    anchor: bray_declarations::SyntaxAnchor,
) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::new();

    diagnostics.add_range(
        context
            .compilation()
            .syntax_tree_result()
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.primary_span().is_some_and(|span| {
                    span.source_id() == anchor.source_id()
                        && anchor.full_range().contains_range(span.range())
                })
            })
            .cloned(),
    );

    diagnostics
}
