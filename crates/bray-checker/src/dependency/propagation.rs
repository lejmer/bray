use bray_bound_tree::{
    BoundExpression, CheckedSemanticSelections, SelectedPropagation, SelectedPropagationBoundary,
    SemanticSelection,
};
use bray_symbols::{DependencyProjection, UnionPayloadFieldSymbolId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

impl super::ValueInputs {
    pub(super) fn collect_propagation<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selections: &CheckedSemanticSelections,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let targets = super::value::transfer_targets(request.unit());

        for entry in selections.entries() {
            let SemanticSelection::Propagation(SelectedPropagation::Result {
                boundary,
                error_conversion,
                ..
            }) = entry.selection()
            else {
                continue;
            };

            if super::value::independent_value_type(request, error_conversion.target_type())? {
                continue;
            }

            let Some(BoundExpression::Structured(expression)) =
                request.unit().tree().expression(entry.expression())
            else {
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in collect_propagation"
                );
            };

            let Some(operand) = expression.operands().first().copied() else {
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in collect_propagation"
                );
            };

            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(
                "ResultVariant1ErrorError",
            )
            .unwrap_or_else(|| {
                panic!("collect_propagation requires compiler-known declaration key")
            });

            let field = request
                .available_compiler_known_symbols()
                .declaration_symbol::<UnionPayloadFieldSymbolId>(&key)
                .unwrap_or_else(|| {
                    panic!("collect_propagation requires compiler-known declaration symbol")
                });

            let projection = DependencyProjection::UnionPayloadField(field);

            self.error_exits
                .insert(entry.expression(), (operand, projection));

            match boundary {
                SelectedPropagationBoundary::Callable => {
                    self.returned_errors.insert(entry.expression());
                }
                SelectedPropagationBoundary::YieldRegion(target) => {
                    let owner = targets
                        .get(target)
                        .copied().unwrap_or_else(|| panic!("collect_propagation requires control transfer target, target: {target:?}"));

                    self.projected
                        .entry(owner)
                        .or_default()
                        .push((operand, vec![projection]));
                }
            }
        }

        Ok(())
    }
}
