use std::sync::Arc;

use crate::{CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView, ExecutionCondition};
use bray_symbols::{CallableSignatureQuery, SymbolQueryRequest};

pub(super) fn references<C>(request: CheckerUnitView<'_, C>)
    -> Result<bray_bound_tree::ConditionCallReferences, CheckerQueryError<C::UpstreamError>>
where C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let mut references = bray_bound_tree::ConditionCallReferences::new();

    let Some(inputs) = request.trusted_contracts() else { return Ok(references); };

    let conditions = inputs.requirements.iter().chain(&inputs.preconditions)
        .chain(inputs.guarantees.iter().map(|(condition, _)| condition))
        .chain(inputs.calls.values().flat_map(|call| call.requirements.iter()
            .chain(&call.execution_requirements)
            .chain(call.preconditions.iter().map(|(condition, _)| condition))
            .chain(&call.guarantees).chain(&call.postconditions)));

    for condition in conditions {
        crate::map_execution_condition_references(condition, &Ok,
            &|place| Ok(ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()))),
            &|value| Ok(Arc::clone(value)),
            &mut |callable, dispatch, witnesses, operands, reference_type| {
                let witnesses: Arc<[_]> = witnesses.into();

                if reference_type.is_some() {
                    let key = (callable, dispatch, Arc::clone(&witnesses));

                    if let std::collections::btree_map::Entry::Vacant(entry) = references.entry(key) {
                        let template = request.callable_result_dependencies(callable.definition().callable_symbol())?;
                        let template = super::resolved_result_template(request, Some(callable), template, None)?;

                        let signature = request.resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                            callable.definition().callable_symbol()))?;

                        entry.insert((signature.value().receiver().is_some(), template.borrows().into()));
                    }
                }

                Ok(ExecutionCondition::new(crate::ExecutionConditionKind::Call(callable, dispatch, witnesses, operands.into(), reference_type)))
            })?;
    }

    Ok(references)
}
