use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpressionId, CheckedSemanticSelections, SelectedOperation, SemanticSelection,
};
use bray_symbols::TypeAssociatedLifecycleSlot;

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

/// Keeps construction failure paths whenever a newly created owner has a local cleanup action.
pub(super) fn admission_expressions<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
) -> Result<BTreeSet<BoundExpressionId>, CheckerQueryError<C::UpstreamError>> {
    let mut types = BTreeMap::new();
    let mut expressions = BTreeSet::new();

    for entry in selections.entries() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        let SemanticSelection::Operation(SelectedOperation::Construction(construction)) =
            entry.selection()
        else {
            continue;
        };

        let Some(ty) = construction.new_owner_type() else {
            continue;
        };

        let required = if let Some(required) = types.get(&ty) {
            *required
        } else {
            let mut required = false;

            for slot in [
                TypeAssociatedLifecycleSlot::Finalizer,
                TypeAssociatedLifecycleSlot::Destructor,
            ] {
                let selected = request.context().lifecycle_callable(ty, slot)?;

                // Lifecycle input and async analysis retain these diagnostics. CFG recovery stays conservative.
                required |= selected.diagnostics().has_errors() || selected.value().is_some();
            }

            types.insert(ty, required);

            required
        };

        if required {
            expressions.insert(entry.expression());
        }
    }

    Ok(expressions)
}
