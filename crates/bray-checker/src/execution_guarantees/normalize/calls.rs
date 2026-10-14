use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundUnit, CheckedExpressionSemantics, SemanticSelection,
};
use bray_symbols::SemanticValueStore;

use crate::ExecutionCondition;

use super::expression_condition;

pub(super) fn selected_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    patterns: &BTreeMap<bray_bound_tree::BoundPatternId, (bray_bound_tree::BoundOperator, ExecutionCondition)>,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    expression: BoundExpressionId,
    budget: &mut usize,
) -> Option<ExecutionCondition> {
    let bound = unit.view().expression(expression)?;

    if let Some(SemanticSelection::Call(call)) = semantics.selections().expression(expression) {
        if call.implementation_hook()
            == Some(bray_compiler_known::ImplementationHook::RawPointerReinterpret)
            && let [
                bray_bound_tree::SelectedArgument::Explicit {
                    expression: pointer,
                    conversion,
                    ..
                },
            ] = call.arguments()
            && matches!(
                conversion.target(),
                bray_bound_tree::ConversionTarget::Identity
            )
        {
            return Some(expression_condition(
                unit, semantics, values, literals, patterns, current, evaluated, *pointer, budget,
            ));
        }

        let observed_intrinsic = matches!(call.implementation_hook(), Some(
            bray_compiler_known::ImplementationHook::SequenceLength
                | bray_compiler_known::ImplementationHook::SequenceIsEmpty
                | bray_compiler_known::ImplementationHook::RawPointerIsNull
        ));

        let constant = matches!(bound, BoundExpression::Call(bound_call)
            if semantics.types().expression(bound_call.callee()).is_some_and(|ty|
                matches!(values.type_data(ty.ty()).as_ref(), bray_symbols::TypeData::Callable(callable)
                    if callable.constness() == bray_symbols::CallableConstness::Constant)));

        if let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
            && (observed_intrinsic || constant)
        {
            let arguments = call_argument_conditions(call, unit, semantics, values, literals, patterns, current, evaluated, budget);

            let arguments = if matches!(call.implementation_hook(), Some(
                bray_compiler_known::ImplementationHook::SequenceLength
                    | bray_compiler_known::ImplementationHook::SequenceIsEmpty
            )) {
                arguments.into_iter().map(|argument| argument.owner_identity().clone()).collect()
            } else { arguments };

            let condition = ExecutionCondition::call(callable, arguments);

            // Losing an input's symbolic contents does not erase the identity of the
            // actual returned value. Repeated evaluations already retire this identity.
            return Some(if condition == ExecutionCondition::Unknown {
                ExecutionCondition::Expression(expression)
            } else { condition });
        }

        if let bray_bound_tree::BoundCallableTarget::Predicate(predicate) = call.target() {
            let arguments = call_argument_conditions(
                call, unit, semantics, values, literals, patterns, current, evaluated, budget,
            );

            return Some(ExecutionCondition::predicate(
                predicate.definition(),
                predicate.substitution(),
                arguments,
            ));
        }

        // A runtime call has a value identity even when its contents are opaque.
        return Some(ExecutionCondition::Expression(expression));
    }

    if let Some(SemanticSelection::Predicate(predicate)) =
        semantics.selections().expression(expression)
    {
        let mut arguments = predicate
            .arguments()
            .iter()
            .map(|argument| {
                let condition = expression_condition(
                    unit,
                    semantics,
                    values,
                    literals,
                    patterns,
                    current,
                    evaluated,
                    argument.expression(),
                    budget,
                );

                let condition = if matches!(
                    values.type_data(argument.ty()).as_ref(),
                    bray_symbols::TypeData::Borrow { .. }
                ) {
                    condition.borrowed()
                } else {
                    condition
                };

                (argument.parameter(), condition)
            })
            .collect::<Vec<_>>();

        arguments.sort_by_key(|(parameter, _)| *parameter);

        return Some(ExecutionCondition::predicate(
            predicate.predicate(),
            predicate.substitution(),
            arguments.into_iter().map(|(_, value)| value).collect(),
        ));
    }

    None
}

fn call_argument_conditions(
    call: &bray_bound_tree::SelectedCall,
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    patterns: &BTreeMap<bray_bound_tree::BoundPatternId, (bray_bound_tree::BoundOperator, ExecutionCondition)>,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    budget: &mut usize,
) -> Vec<ExecutionCondition> {
    let mut arguments = Vec::new();

    if let Some(receiver) = call.receiver() {
        arguments.push(expression_condition(
            unit,
            semantics,
            values,
            literals,
            patterns,
            current,
            evaluated,
            receiver.expression(),
            budget,
        ));
    }

    let mut parameters = call
        .arguments()
        .iter()
        .map(|argument| match argument {
            bray_bound_tree::SelectedArgument::Explicit {
                expression,
                ordinal,
                conversion,
                ..
            } if matches!(
                conversion.target(),
                bray_bound_tree::ConversionTarget::Identity
                    | bray_bound_tree::ConversionTarget::CallableContract
                    | bray_bound_tree::ConversionTarget::BuiltInScalar
                    | bray_bound_tree::ConversionTarget::NullablePresent
            ) =>
            {
                let condition = expression_condition(
                    unit,
                    semantics,
                    values,
                    literals,
                    patterns,
                    current,
                    evaluated,
                    *expression,
                    budget,
                );

                let condition =
                    if *conversion.target() == bray_bound_tree::ConversionTarget::NullablePresent {
                        condition.present()
                    } else {
                        condition
                    };

                let condition = if matches!(
                    call.target(),
                    bray_bound_tree::BoundCallableTarget::Predicate(_)
                ) && matches!(
                    values.type_data(conversion.target_type()).as_ref(),
                    bray_symbols::TypeData::Borrow { .. }
                ) {
                    condition.borrowed()
                } else {
                    condition
                };

                (*ordinal, condition)
            }
            bray_bound_tree::SelectedArgument::Explicit { ordinal, .. }
            | bray_bound_tree::SelectedArgument::Default { ordinal, .. } => {
                (*ordinal, ExecutionCondition::Unknown)
            }
        })
        .collect::<Vec<_>>();

    parameters.sort_by_key(|(ordinal, _)| *ordinal);
    arguments.extend(parameters.into_iter().map(|(_, value)| value));

    arguments
}
