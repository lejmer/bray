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
            let SemanticSelection::Propagation(selection) = entry.selection() else { continue; };

            let Some(BoundExpression::Structured(expression)) = request.unit().tree().expression(entry.expression()) else {
                unreachable!("checked propagation retains its structured expression");
            };

            let operand = *expression.operands().first().expect("checked propagation retains its operand");
            let projection = selection.success().map_or(DependencyProjection::NullableValue, |(_, field)| DependencyProjection::UnionPayloadField(field));

            self.expressions.remove(&entry.expression());
            self.projected.entry(entry.expression()).or_default().push((operand, vec![projection]));

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
