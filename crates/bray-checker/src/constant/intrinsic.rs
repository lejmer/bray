use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticBag, DiagnosticId};
use bray_symbols::{
    CallableInstanceData, ConstantValueData, ConstantValueId, ConstantValueKind,
    SemanticValueStore, SemanticValueStoreError, TypeData, TypeId,
};

use super::call::EvaluatedConstantCall;
use super::limits::{ConstantEvaluationLimits, ConstantEvaluationUsage, EvaluationBudget};
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext};

/// Evaluates fixed compiler-known value operations through the owning constant semantics.
/// The coordinator remains responsible for the exact call cache and checked signature.
pub fn evaluate_constant_intrinsic<C>(
    context: &C,
    callable: CallableInstanceData,
    hook: ImplementationHook,
    arguments: &[ConstantValueId],
    result_type: TypeId,
    limits: ConstantEvaluationLimits,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
    C::UpstreamError: From<SemanticValueStoreError>,
{
    let values = context.semantic_values();
    let mut budget = EvaluationBudget::from_limits(limits);
    let usage = ConstantEvaluationUsage::new(string_work(values, hook, arguments), 0, 0);

    if let Err(problem) = budget.try_charge_usage(usage) {
        return match problem.render(context, DiagnosticId::new(0), None, || Ok(result_type)) {
            Ok(diagnostic) => {
                let mut diagnostics = DiagnosticBag::new();

                diagnostics.add(diagnostic);

                CheckerOutcome::complete(None, diagnostics)
            }
            Err(CheckerQueryError::Cancelled) => CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Upstream(error)) => CheckerOutcome::UpstreamFailure(error),
        };
    }

    match intrinsic_value(context, callable, hook, arguments, result_type) {
        Ok(value) => CheckerOutcome::without_diagnostics(
            value.map(|value| EvaluatedConstantCall::new(value, budget.usage(limits))),
        ),
        Err(error) => CheckerOutcome::UpstreamFailure(error.into()),
    }
}

fn string_work(
    values: &SemanticValueStore,
    hook: ImplementationHook,
    arguments: &[ConstantValueId],
) -> u64 {
    let length = |value| {
        let value = values.constant_value_data(values.observed_constant_value(value));

        match value.kind() {
            ConstantValueKind::String(text) => {
                u64::try_from(text.len()).expect("bounded UTF-8 size fits u64")
            }
            _ => 0,
        }
    };

    match (hook, arguments) {
        (
            ImplementationHook::StringScalarCount | ImplementationHook::StringScalarAt,
            [value, ..],
        ) => length(*value),
        (ImplementationHook::StringEquals, [left, right]) => {
            let (left, right) = (length(*left), length(*right));

            if left == right { left } else { 0 }
        }
        _ => 0,
    }
}

fn intrinsic_value<C: CheckerRequestContext + ?Sized>(
    context: &C,
    callable: CallableInstanceData,
    hook: ImplementationHook,
    arguments: &[ConstantValueId],
    result_type: TypeId,
) -> Result<Option<ConstantValueId>, SemanticValueStoreError> {
    if hook.is_numeric_policy() {
        return numeric_value(context, callable, hook, arguments, result_type);
    }

    let values = context.semantic_values();

    let value = match (hook, arguments) {
        (
            ImplementationHook::NullableIsPresent | ImplementationHook::NullableIsAbsent,
            [receiver],
        ) => {
            let receiver = values.constant_value_data(values.observed_constant_value(*receiver));

            let receiver_type = values.type_data(receiver.ty());

            if !matches!(receiver_type.as_ref(), TypeData::Nullable(_)) {
                return Ok(None);
            }

            let is_present = match receiver.kind() {
                ConstantValueKind::NullableAbsent => false,
                ConstantValueKind::Error => return Ok(None),
                _ => true,
            };

            let value = if hook == ImplementationHook::NullableIsPresent {
                is_present
            } else {
                !is_present
            };

            values.intern_constant_value(ConstantValueData::new(
                result_type,
                ConstantValueKind::Boolean(value),
            ))?
        }
        (ImplementationHook::SequenceLength | ImplementationHook::SequenceIsEmpty, [receiver]) => {
            let receiver = values.constant_value_data(values.observed_constant_value(*receiver));

            let count = match receiver.kind() {
                ConstantValueKind::Array(elements) => u64::try_from(elements.len())
                    .expect("bounded constant sequence length fits u64"),
                ConstantValueKind::String(text) => {
                    u64::try_from(text.len()).expect("bounded UTF-8 byte length fits u64")
                }
                ConstantValueKind::StaticAddress(_) => {
                    let ty = values.type_data(values.unborrowed_type(receiver.ty()));

                    let TypeData::Array { length, .. } = ty.as_ref() else {
                        return Ok(None);
                    };

                    let length =
                        values.substitute_constant_term(*length, callable.substitution())?;

                    let bray_symbols::ConstantTermData::Value(length) =
                        *values.constant_term_data(length)
                    else {
                        return Ok(None);
                    };

                    let length = values.constant_value_data(length);

                    let ConstantValueKind::Integer(length) = length.kind() else {
                        panic!("checked array extent is an integer");
                    };

                    length
                        .to_u64()
                        .expect("checked target array extent fits u64")
                }
                _ => return Ok(None),
            };

            let kind = if hook == ImplementationHook::SequenceIsEmpty {
                ConstantValueKind::Boolean(count == 0)
            } else {
                ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(count))
            };

            values.intern_constant_value(ConstantValueData::new(result_type, kind))?
        }
        (ImplementationHook::StringScalarCount | ImplementationHook::StringIsEmpty, [argument]) => {
            let argument = values.constant_value_data(values.observed_constant_value(*argument));

            let ConstantValueKind::String(text) = argument.kind() else {
                return Ok(None);
            };

            let kind = if hook == ImplementationHook::StringIsEmpty {
                ConstantValueKind::Boolean(text.is_empty())
            } else {
                ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(
                    u64::try_from(text.chars().count())
                        .expect("bounded string scalar count fits u64"),
                ))
            };

            values.intern_constant_value(ConstantValueData::new(result_type, kind))?
        }
        (ImplementationHook::StringEquals, [left, right]) => {
            let left = values.constant_value_data(values.observed_constant_value(*left));
            let right = values.constant_value_data(values.observed_constant_value(*right));

            let (ConstantValueKind::String(left), ConstantValueKind::String(right)) =
                (left.kind(), right.kind())
            else {
                return Ok(None);
            };

            values.intern_constant_value(ConstantValueData::new(
                result_type,
                ConstantValueKind::Boolean(left == right),
            ))?
        }
        (ImplementationHook::StringScalarAt, [argument, index]) => {
            let argument = values.constant_value_data(values.observed_constant_value(*argument));
            let index = values.constant_value_data(values.observed_constant_value(*index));

            let (ConstantValueKind::String(text), ConstantValueKind::Integer(index)) =
                (argument.kind(), index.kind())
            else {
                return Ok(None);
            };

            let index = index
                .to_u64()
                .and_then(|index| usize::try_from(index).ok())
                .expect("checked scalar index fits target and host");

            let kind = if let Some(character) = text.chars().nth(index) {
                let result = values.type_data(result_type);

                let TypeData::Nullable(element) = result.as_ref() else {
                    panic!("recognized scalar lookup retains its nullable character result");
                };

                let character = values.intern_constant_value(ConstantValueData::new(
                    *element,
                    ConstantValueKind::Character(character),
                ))?;

                ConstantValueKind::NullablePresent(character)
            } else {
                ConstantValueKind::NullableAbsent
            };

            values.intern_constant_value(ConstantValueData::new(result_type, kind))?
        }
        (ImplementationHook::RawPointerIsNull, [argument]) => {
            let argument = values.constant_value_data(values.observed_constant_value(*argument));

            let ConstantValueKind::StaticAddress(_) = argument.kind() else {
                return Ok(None);
            };

            values.intern_constant_value(ConstantValueData::new(
                result_type,
                ConstantValueKind::Boolean(false),
            ))?
        }
        (ImplementationHook::RawPointerByteAddress, [argument]) => {
            let argument = values.constant_value_data(values.observed_constant_value(*argument));

            // Pointer representations share interned address identity, never the reached storage.
            values.intern_constant_value(ConstantValueData::new(
                result_type,
                argument.kind().clone(),
            ))?
        }
        (ImplementationHook::AtomicInitialize, [argument]) => {
            let argument = values.constant_value_data(*argument);

            if !matches!(
                argument.kind(),
                ConstantValueKind::Boolean(_)
                    | ConstantValueKind::Integer(_)
                    | ConstantValueKind::StaticAddress(_)
            ) {
                return Ok(None);
            }

            // The accepted variants contain only scalar values or one interned static address.
            values.intern_constant_value(ConstantValueData::new(
                result_type,
                argument.kind().clone(),
            ))?
        }
        (ImplementationHook::UninitFrom, [argument]) => {
            let argument = values.constant_value_data(*argument);

            // Protected storage retains the value's representation. Aggregate children
            // are interned IDs, so this copies no represented payload or ownership.
            values.intern_constant_value(ConstantValueData::new(
                result_type,
                argument.kind().clone(),
            ))?
        }
        (ImplementationHook::UninitNew, []) => values.intern_constant_value(
            ConstantValueData::new(result_type, ConstantValueKind::product([])),
        )?,
        _ => return Ok(None),
    };

    Ok(Some(value))
}

fn numeric_value<C: CheckerRequestContext + ?Sized>(
    context: &C,
    callable: CallableInstanceData,
    hook: ImplementationHook,
    arguments: &[ConstantValueId],
    result_type: TypeId,
) -> Result<Option<ConstantValueId>, SemanticValueStoreError> {
    let values = context.semantic_values();

    if !values.substitution_is_concrete(callable.substitution()) {
        return Ok(None);
    }

    let value = match (hook, arguments) {
        (ImplementationHook::NumericTruncationSupported, []) => {
            let supported = bray_symbols::NumericTruncation::for_instance(
                values,
                context.available_compiler_known_symbols(),
                callable,
                context
                    .selected_target()
                    .machine()
                    .pointer_width_bits()
                    .get(),
            )
            .is_some();

            values.intern_constant_value(ConstantValueData::new(
                result_type,
                ConstantValueKind::Boolean(supported),
            ))?
        }
        (ImplementationHook::NumericTruncationDomain, [argument]) => {
            let plan = bray_symbols::NumericTruncation::for_instance(
                values,
                context.available_compiler_known_symbols(),
                callable,
                context
                    .selected_target()
                    .machine()
                    .pointer_width_bits()
                    .get(),
            )
            .expect("checked domain query satisfies its scalar type constraint");

            let argument = values.constant_value_data(values.observed_constant_value(*argument));

            let valid = match plan {
                bray_symbols::NumericTruncation::RealToInteger {
                    minimum,
                    minimum_inclusive,
                    maximum,
                    ..
                } => {
                    let ConstantValueKind::Real(value) = argument.kind() else {
                        panic!("checked real domain query retains a real source");
                    };

                    let compare = |operator, bound| match super::floating::fold_real_binary(
                        operator, *value, bound,
                    )
                    .expect("same-format ordered comparison is defined for every real bit pattern")
                    {
                        ConstantValueKind::Boolean(result) => result,
                        _ => unreachable!("ordered real comparison returns a Boolean"),
                    };

                    compare(
                        if minimum_inclusive {
                            bray_bound_tree::BoundOperator::GreaterEqual
                        } else {
                            bray_bound_tree::BoundOperator::Greater
                        },
                        minimum,
                    ) && compare(bray_bound_tree::BoundOperator::Less, maximum)
                }
                _ => true,
            };

            values.intern_constant_value(ConstantValueData::new(
                result_type,
                ConstantValueKind::Boolean(valid),
            ))?
        }
        (ImplementationHook::NumericTruncate, [argument]) => {
            let argument = values.constant_value_data(values.observed_constant_value(*argument));

            let plan = bray_symbols::NumericTruncation::for_instance(
                values,
                context.available_compiler_known_symbols(),
                callable,
                context
                    .selected_target()
                    .machine()
                    .pointer_width_bits()
                    .get(),
            )
            .expect("checked truncation satisfies its scalar type constraint");

            let kind = match plan {
                bray_symbols::NumericTruncation::Integer { source, target } => {
                    let ConstantValueKind::Integer(value) = argument.kind() else {
                        panic!("checked integer truncation retains an integer source");
                    };

                    super::machine::fold_machine_integer_truncate(
                        value,
                        source,
                        target,
                        context.selected_target().machine().pointer_width_bits(),
                    )
                }
                bray_symbols::NumericTruncation::RealToInteger { target, .. } => {
                    let ConstantValueKind::Real(value) = argument.kind() else {
                        panic!("checked real truncation retains a real source");
                    };

                    super::floating::truncate_real(
                        *value,
                        target,
                        context.selected_target().machine().pointer_width_bits(),
                    )
                    .expect(
                        "checked truncation requirement establishes the integer conversion domain",
                    )
                }
                bray_symbols::NumericTruncation::Lossless => {
                    let target = context
                        .available_compiler_known_symbols()
                        .type_representation(values, result_type)
                        .expect("checked lossless conversion retains a scalar target");

                    super::conversion::convert_scalar(argument.kind(), target, || {
                        context.selected_target().machine().pointer_width_bits()
                    })
                    .expect("checked lossless scalar conversion preserves every source value")
                }
            };

            values.intern_constant_value(ConstantValueData::new(result_type, kind))?
        }
        _ => panic!(
            "checked numeric policy {hook:?} retains its declared argument count, actual: {}",
            arguments.len()
        ),
    };

    Ok(Some(value))
}
