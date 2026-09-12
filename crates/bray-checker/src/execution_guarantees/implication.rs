use std::collections::BTreeMap;

use bray_bound_tree::BoundReferenceTarget;

use super::ExecutionCondition;

/// Checks whether entry assumptions establish a condition after matching callable inputs.
/// Unmapped inputs and exhausted reasoning remain unknown and cannot establish a guarantee.
pub fn execution_condition_is_implied(
    condition: &ExecutionCondition,
    assumptions: &[ExecutionCondition],
    inputs: &BTreeMap<BoundReferenceTarget, BoundReferenceTarget>,
) -> bool {
    let condition = remap_execution_condition_inputs(condition, inputs);
    let mut known = Default::default();

    for assumption in assumptions {
        // The proof set retains the immutable terms for the duration of this implication.
        assumption.clone().assume(true, &mut known);
    }

    condition.prove(&known, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
}

/// Matches callable input identities while preserving field paths and completion results.
/// Unmapped inputs cannot supply logical evidence.
pub fn remap_execution_condition_inputs(
    condition: &ExecutionCondition,
    inputs: &BTreeMap<BoundReferenceTarget, BoundReferenceTarget>,
) -> ExecutionCondition {
    condition.substitute(
        &|place| {
            let Some(root) = inputs.get(&place.root) else {
                return ExecutionCondition::Unknown;
            };

            let mut mapped = super::ExecutionPlace::from(*root);

            for field in place.fields.iter() {
                mapped = mapped.field(*field);
            }

            ExecutionCondition::Input(mapped)
        },
        &ExecutionCondition::Result,
        &mut { ExecutionCondition::WORK_LIMIT },
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundOperator;
    use bray_symbols::{CallableParameterSymbolId, SymbolId};

    use super::{BoundReferenceTarget, ExecutionCondition, execution_condition_is_implied};

    #[test]
    fn implication_maps_inputs_and_preserves_unknowns() {
        let input = |index| {
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
            )
        };

        let source = ExecutionCondition::Input(input(1).into());
        let target = ExecutionCondition::Input(input(2).into());
        let mapping = [(input(1), input(2))].into();

        assert!(execution_condition_is_implied(&source, &[target], &mapping));
        assert!(!execution_condition_is_implied(&source, &[], &mapping));

        assert!(!execution_condition_is_implied(
            &ExecutionCondition::Unknown,
            &[ExecutionCondition::Unknown],
            &mapping,
        ));

        assert!(!execution_condition_is_implied(
            &source,
            &[source.clone()],
            &Default::default(),
        ));
    }

    #[test]
    fn conjunction_can_establish_a_weaker_entry_domain() {
        let input = BoundReferenceTarget::Surface(
            CallableParameterSymbolId::from_symbol_id(SymbolId::new(1)).into(),
        );

        let condition = ExecutionCondition::Input(input.into());

        let conjunction = ExecutionCondition::operation(
            BoundOperator::LogicalAnd,
            vec![condition.clone(), ExecutionCondition::Boolean(true)],
        );

        assert!(execution_condition_is_implied(
            &condition,
            &[conjunction],
            &[(input, input)].into(),
        ));
    }
}
