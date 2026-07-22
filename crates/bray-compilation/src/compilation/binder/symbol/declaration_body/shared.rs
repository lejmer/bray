use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult};
use bray_bound_tree::{BoundUnitKey, BoundUnitRoot};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::TypeId;

use crate::compilation::binder::CompilationBinderFacts;

pub(super) struct CheckedSourceExpression {
    pub(super) result: TypeId,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) is_recovered: bool,
}

pub(super) fn checked_source_expression(
    context: &CompilationBinderFacts<'_>,
    key: BoundUnitKey,
) -> BinderFactResult<CheckedSourceExpression> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let BoundUnitRoot::Expression(root) = bound.result().value().root() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let control_flow = compilation
        .control_flow_with_cancellation(key, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let (types, _) = semantics.result().value();
    let result = types
        .expression(root)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let expression = bound
        .result()
        .value()
        .view()
        .expression(root)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let diagnostics = DiagnosticBag::merged_all([
        bound.result().diagnostics(),
        semantics.result().diagnostics(),
        control_flow.result().diagnostics(),
    ]);

    Ok(CheckedSourceExpression {
        result: result.ty(),
        diagnostics,
        is_recovered: expression.is_recovered() || result.is_recovered(),
    })
}

pub(super) fn empty_dependency_contract(
    context: &CompilationBinderFacts<'_>,
) -> BinderFactResult<bray_symbols::DependencyContractTemplateId> {
    context
        .semantic_values()
        .empty_dependency_contract_template()
        .map_err(|_| BinderFactError::DependencyUnavailable)
}

pub(super) fn syntax_diagnostics(
    context: &CompilationBinderFacts<'_>,
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
