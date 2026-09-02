use crate::compilation::binder::BindingQueryResult;
use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_checker::{ConstantChecker, ConstantEvaluationInput, DefaultConstantChecker};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::StaticSymbolId;

use super::binding::binder_error;
use crate::compilation::binder::CompilationBindingContext;
use crate::compilation::checker::checker_result;
use crate::compilation::unit::{checker_unit_view, semantic_unit_context_for};

pub(super) fn validate_static_initializer_template(
    context: &CompilationBindingContext<'_>,
    key: &BoundUnitKey,
) -> BindingQueryResult<DiagnosticBag> {
    let bound = context
        .compilation()
        .bound_unit_with_cancellation(key.clone(), context.cancellation())
        .map_err(binder_error)?;

    let semantics = context
        .compilation()
        .expression_semantics_with_cancellation(key.clone(), context.cancellation())
        .map_err(binder_error)?;

    let checker_context = context
        .compilation()
        .checker_context_for(key, context.cancellation())
        .map_err(binder_error)?;

    let semantic_context =
        semantic_unit_context_for(checker_context.symbols(), bound.result().value())
            .map_err(binder_error)?;

    let references = context
        .compilation()
        .symbolic_references(
            bound.result().value(),
            semantics.result().value().selections(),
        )
        .map_err(binder_error)?;

    let resolver = crate::compilation::constant::CompilationConstantCallResolver::new(
        context.compilation(),
        context.cancellation(),
    );

    let input = ConstantEvaluationInput::new(
        semantics.result().value().types(),
        semantics.result().value().selections(),
    )
    .with_references(references)
    .with_call_resolver(&resolver)
    .with_static_address_borrows();

    let unit = checker_unit_view(bound.result().value(), &semantic_context, &checker_context)
        .map_err(binder_error)?;

    let checked = checker_result(DefaultConstantChecker.check_constant_term(unit, &input))
        .map_err(binder_error)?;

    Ok(checked.diagnostics().clone())
}

impl crate::compilation::Compilation {
    pub(in crate::compilation) fn static_initializer_key(
        &self,
        declaration: StaticSymbolId,
    ) -> Result<Option<BoundUnitKey>, crate::fact::FactQueryError> {
        let symbols = self.symbol_graph()?;

        let Some(owner) = symbols.symbol_key(declaration.into()) else {
            return Ok(None);
        };

        Ok(self.declared_unit_keys()?.into_iter().find(|key| {
            key.kind() == BoundUnitKind::ConstantTemplate && key.declared_owner() == owner
        }))
    }
}
