use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpressionId, BoundUnit, CheckedExpressionSemantics, SemanticSelection,
};
use bray_symbols::SemanticValueStore;

use crate::ExecutionCondition;

use super::expression_condition;

#[expect(clippy::too_many_arguments, reason = "condition normalization consumes independent path, pattern, evaluation and predicate facts")]
pub(super) fn selected_condition(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    patterns: &BTreeMap<
        bray_bound_tree::BoundPatternId,
        (bray_bound_tree::BoundOperator, ExecutionCondition),
    >,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    expression: BoundExpressionId,
    budget: &mut usize,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
) -> Option<ExecutionCondition> {
    if let Some(SemanticSelection::Call(call)) = semantics.selections().expression(expression) {
        return Some(selected_call_condition(
            values,
            predicates,
            expression,
            call,
            |budget| {
                call_argument_conditions(
                    call, unit, semantics, values, literals, patterns, current, evaluated, budget,
                    predicates,
                )
            },
            budget,
        ));
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
                    predicates,
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

        return Some(predicate_condition(
            values,
            predicates,
            bray_symbols::PredicateInstanceData::new(
                predicate.predicate(),
                predicate.substitution(),
            ),
            arguments.into_iter().map(|(_, value)| value).collect(),
            budget,
        ));
    }

    None
}

pub(crate) fn selected_call_condition(
    values: &SemanticValueStore,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
    expression: BoundExpressionId,
    call: &bray_bound_tree::SelectedCall,
    arguments: impl FnOnce(&mut usize) -> Vec<ExecutionCondition>,
    budget: &mut usize,
) -> ExecutionCondition {
    use bray_compiler_known::ImplementationHook as Hook;

    if matches!(
        call.implementation_hook(),
        Some(
            Hook::RawPointerReinterpret
                | Hook::FunctionPointerReinterpret
                | Hook::FunctionPointerFromData
                | Hook::FunctionPointerToData
                | Hook::RawPointerByteAddress
        )
    ) && let [bray_bound_tree::SelectedArgument::Explicit { conversion, .. }] = call.arguments()
        && matches!(
            conversion.target(),
            bray_bound_tree::ConversionTarget::Identity
        )
    {
        return arguments(budget)
            .into_iter()
            .next()
            .expect("pointer projection has its single input");
    }

    if let bray_bound_tree::BoundCallableTarget::Predicate(predicate) = call.target() {
        let mut arguments = arguments(budget);

        for input in call.arguments() {
            let (ordinal, ty) = match input {
                bray_bound_tree::SelectedArgument::Explicit {
                    ordinal,
                    conversion,
                    ..
                } => (*ordinal, conversion.target_type()),
                bray_bound_tree::SelectedArgument::Default { ordinal, ty, .. } => (*ordinal, *ty),
            };

            if matches!(
                values.type_data(ty).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            ) {
                let index = usize::try_from(ordinal).expect("input ordinal fits host")
                    + usize::from(call.receiver().is_some());

                arguments[index] = arguments[index].clone().borrowed();
            }
        }

        return predicate_condition(values, predicates, predicate, arguments, budget);
    }

    let observed_intrinsic = matches!(
        call.implementation_hook(),
        Some(Hook::SequenceLength | Hook::SequenceIsEmpty | Hook::RawPointerIsNull)
    );

    if let Some(callable) = call.resolution().declaration_source()
        && (observed_intrinsic || call.constness() == bray_symbols::CallableConstness::Constant)
    {
        let mut arguments = arguments(budget);

        if matches!(
            call.implementation_hook(),
            Some(Hook::SequenceLength | Hook::SequenceIsEmpty)
        ) {
            for value in &mut arguments {
                *value = value.owner_identity().clone();
            }
        }

        let condition = ExecutionCondition::call(callable, arguments);

        return if condition == ExecutionCondition::Unknown {
            ExecutionCondition::Expression(expression)
        } else {
            condition
        };
    }

    ExecutionCondition::Expression(expression)
}

#[expect(clippy::too_many_arguments, reason = "argument normalization consumes independent path, pattern, evaluation and predicate facts")]
fn call_argument_conditions(
    call: &bray_bound_tree::SelectedCall,
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    literals: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    patterns: &BTreeMap<
        bray_bound_tree::BoundPatternId,
        (bray_bound_tree::BoundOperator, ExecutionCondition),
    >,
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    evaluated: &BTreeMap<BoundExpressionId, ExecutionCondition>,
    budget: &mut usize,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
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
            predicates,
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
                    predicates,
                );

                let condition =
                    if *conversion.target() == bray_bound_tree::ConversionTarget::NullablePresent {
                        condition.present()
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

    for argument in call.arguments() {
        if let bray_bound_tree::SelectedArgument::Default {
            provider, ordinal, ..
        } = argument
        {
            let place = bray_bound_tree::BoundReferenceTarget::Surface((*provider).into()).into();

            if let Some(meaning) = current.get(&place) {
                let meaning = match call.resolution().declaration_source() {
                    Some(callable) => crate::substitute_execution_condition(
                        values,
                        meaning,
                        callable.substitution(),
                    )
                    .expect("checked default application belongs to this semantic store"),
                    None => meaning.clone(), // Lexical anonymous input paths are already selected.
                };

                let inputs = arguments
                    .iter()
                    .enumerate()
                    .map(|(ordinal, value)| {
                        (
                            crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                                u32::try_from(ordinal).expect("argument ordinal fits"),
                            )),
                            value.clone(),
                        )
                    })
                    .collect();

                arguments[usize::try_from(*ordinal).expect("default ordinal fits")
                    + usize::from(call.receiver().is_some())] = meaning.substitute(
                    &|place| {
                        place
                            .value_in(&inputs)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &ExecutionCondition::Unknown,
                    budget,
                );
            }
        }
    }

    arguments
}

fn predicate_condition(
    values: &SemanticValueStore,
    predicates: Option<
        &BTreeMap<bray_symbols::PredicateInstanceData, bray_symbols::PredicateSemanticSummary>,
    >,
    predicate: bray_symbols::PredicateInstanceData,
    arguments: Vec<ExecutionCondition>,
    budget: &mut usize,
) -> ExecutionCondition {
    if let Some(term) = predicates
        .and_then(|predicates| predicates.get(&predicate))
        .and_then(|predicate| predicate.condition())
    {
        return crate::execution_guarantees::portable::decode(values, term, &arguments, budget)
            .expect("checked predicate terms belong to this semantic value store");
    }

    ExecutionCondition::predicate(predicate.definition(), predicate.substitution(), arguments)
}
