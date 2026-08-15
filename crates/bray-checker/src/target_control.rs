// rust-style: allow(module-too-large, reason = "target-control checking is one correlated validation pipeline over shared literal, type, and target facts")

use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, CheckedLiteralValues,
    CheckedMemoryOperationKind, CheckedMemoryOperations, CheckedSemanticSelections,
    ConstructionTarget, InlineAssemblyContract, InlineAssemblyOperand, InlineAssemblyOperandKind,
    MAX_INLINE_ASSEMBLY_OPERANDS, MemoryOrder, MemoryReadKind, PointerAddressComparison,
    SelectedOperation, SemanticSelection, VolatileAddressSpace,
};
use bray_compiler_known::{ImplementationHook, IntegerRepresentation, RepresentationRole};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{CallableExecution, ConstantValueId, ConstantValueKind, TypeData, TypeId};
use bray_target::{InlineAssemblyOptions, TargetControlFacts};

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView,
};
use crate::target_control_contract::{
    clobbers_valid, feature_name_valid, parse_constraint, separated_ranges, separated_values,
    template_valid,
};

pub(crate) enum TargetControlCheck {
    NotApplicable,
    Invalid,
    Valid(CheckedMemoryOperationKind),
}

pub(crate) fn classify_operation<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    types: &[TypeId],
    read_kinds: &mut BTreeMap<TypeId, MemoryReadKind>,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<CheckedMemoryOperationKind>, CheckerOutcome<CheckedMemoryOperations>>
where
    C: CheckerRequestContext + ?Sized,
{
    let one = || {
        let [ty] = types else {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        };

        Ok(*ty)
    };

    let no_types = || {
        if !types.is_empty() {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        }

        Ok(())
    };

    let kind = match hook {
        ImplementationHook::VolatileLoad | ImplementationHook::DeviceVolatileLoad => {
            let pointee = one()?;
            let kind = super::memory::memory_read_kind(request, pointee, read_kinds, diagnostics)?;

            let address_space = if hook == ImplementationHook::DeviceVolatileLoad {
                VolatileAddressSpace::Device
            } else {
                VolatileAddressSpace::Host
            };

            CheckedMemoryOperationKind::VolatileRead {
                pointee,
                address_space,
                kind,
            }
        }
        ImplementationHook::VolatileStore | ImplementationHook::DeviceVolatileStore => {
            let address_space = if hook == ImplementationHook::DeviceVolatileStore {
                VolatileAddressSpace::Device
            } else {
                VolatileAddressSpace::Host
            };

            CheckedMemoryOperationKind::VolatileWrite {
                pointee: one()?,
                address_space,
            }
        }
        ImplementationHook::PointerExposeAddress => CheckedMemoryOperationKind::ExposeAddress {
            pointee: one()?,
        },
        ImplementationHook::PointerFromExposedAddress => {
            CheckedMemoryOperationKind::FromExposedAddress { pointee: one()? }
        }
        ImplementationHook::PointerAddressEqual | ImplementationHook::PointerAddressLess => {
            let comparison = if hook == ImplementationHook::PointerAddressEqual {
                PointerAddressComparison::Equal
            } else {
                PointerAddressComparison::Less
            };

            CheckedMemoryOperationKind::CompareAddress {
                pointee: one()?,
                comparison,
            }
        }
        ImplementationHook::CatastrophicAbort => {
            no_types()?;

            CheckedMemoryOperationKind::CatastrophicAbort
        }
        ImplementationHook::DebuggerTrap => {
            no_types()?;

            CheckedMemoryOperationKind::DebuggerTrap
        }
        ImplementationHook::UnreachableTermination => {
            no_types()?;

            CheckedMemoryOperationKind::UnreachableTermination
        }
        ImplementationHook::SpinLoopHint => {
            no_types()?;

            CheckedMemoryOperationKind::SpinLoopHint
        }
        _ => return Ok(None),
    };

    Ok(Some(kind))
}

pub(crate) fn check_contract<C>(
    request: CheckerUnitView<'_, C>,
    hook: ImplementationHook,
    types: &[TypeId],
    arguments: &[BoundExpressionId],
    literals: &CheckedLiteralValues,
    selections: &CheckedSemanticSelections,
) -> Result<TargetControlCheck, CheckerInfrastructureError>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<bray_symbols::CallableSignatureFact>
        + ?Sized,
{
    let control = TargetControlFacts::for_profile(request.selected_target());

    match hook {
        ImplementationHook::TargetFeatureEnabled => {
            if !types.is_empty() {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }

            let [feature] = arguments else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            Ok(match literal_string(request, literals, *feature)? {
                Some((feature, _)) => TargetControlCheck::Valid(
                    CheckedMemoryOperationKind::TargetFeatureEnabled { feature },
                ),
                None => TargetControlCheck::Invalid,
            })
        }
        ImplementationHook::CompilerFence | ImplementationHook::HardwareFence => {
            if !types.is_empty() {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }

            let [order] = arguments else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            Ok(match literal_memory_order(request, literals, selections, *order)? {
                Some(order) if order.valid_for_fence() => {
                    TargetControlCheck::Valid(CheckedMemoryOperationKind::Fence {
                        compiler_only: hook == ImplementationHook::CompilerFence,
                        order,
                    })
                }
                _ => TargetControlCheck::Invalid,
            })
        }
        ImplementationHook::InlineAssembly
        | ImplementationHook::DivergingInlineAssembly
        | ImplementationHook::BranchingInlineAssembly => {
            let (inputs, output, labels) = match (hook, types) {
                (ImplementationHook::InlineAssembly, [inputs, output]) => {
                    (*inputs, Some(*output), None)
                }
                (ImplementationHook::DivergingInlineAssembly, [inputs]) => (*inputs, None, None),
                (ImplementationHook::BranchingInlineAssembly, [inputs, output, labels]) => {
                    (*inputs, Some(*output), Some(*labels))
                }
                _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
            };

            Ok(match assembly_contract(
                request,
                arguments,
                literals,
                control,
                inputs,
                output,
                labels,
            )? {
                Some(contract) => TargetControlCheck::Valid(
                    CheckedMemoryOperationKind::InlineAssembly {
                        inputs,
                        output,
                        labels,
                        contract,
                    },
                ),
                None => TargetControlCheck::Invalid,
            })
        }
        _ => Ok(TargetControlCheck::NotApplicable),
    }
}

fn assembly_contract<C>(
    request: CheckerUnitView<'_, C>,
    arguments: &[BoundExpressionId],
    literals: &CheckedLiteralValues,
    control: TargetControlFacts,
    inputs_type: TypeId,
    output_type: Option<TypeId>,
    labels_type: Option<TypeId>,
) -> Result<Option<InlineAssemblyContract>, CheckerInfrastructureError>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<bray_symbols::CallableSignatureFact>
        + ?Sized,
{
    let (template, constraints, clobbers, features, options, inputs) = match arguments {
        [template, constraints, clobbers, features, options, inputs] if labels_type.is_none() => {
            (*template, *constraints, *clobbers, *features, *options, *inputs)
        }
        [template, constraints, clobbers, features, options, inputs, _]
            if labels_type.is_some() =>
        {
            (
                *template,
                *constraints,
                *clobbers,
                *features,
                *options,
                *inputs,
            )
        }
        _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
    };

    let Some((template_value, template)) = literal_string(request, literals, template)? else {
        return Ok(None);
    };

    let Some((constraints_value, constraints)) = literal_string(request, literals, constraints)? else {
        return Ok(None);
    };

    let Some((clobbers_value, clobbers)) = literal_string(request, literals, clobbers)? else {
        return Ok(None);
    };

    let Some((features_value, features)) = literal_string(request, literals, features)? else {
        return Ok(None);
    };

    let Some((options_value, options)) = literal_integer(request, literals, options)? else {
        return Ok(None);
    };

    let Some(options) = InlineAssemblyOptions::try_new(options) else {
        return Ok(None);
    };

    let Some(constraint_ranges) = separated_ranges(&constraints) else {
        return Ok(None);
    };

    let Some(clobbers) = separated_values(&clobbers) else {
        return Ok(None);
    };

    let Some(features) = separated_values(&features) else {
        return Ok(None);
    };

    let Some((operands, operand_count)) = checked_operands(
        request,
        literals,
        control,
        &constraints,
        &constraint_ranges,
        inputs_type,
        output_type,
        labels_type,
        inputs,
    )? else {
        return Ok(None);
    };

    let valid = control.inline_assembly()
        && template_valid(
            &template,
            llvm_operand_count(&operands, usize::from(operand_count)),
        )
        && clobbers_valid(control, &clobbers, options)
        && features.iter().all(|feature| feature_name_valid(feature))
        && features
            .iter()
            .all(|feature| control.supports_feature(feature))
        && (!options.intel_dialect() || control.intel_assembly_dialect())
        && !options.may_unwind();

    Ok(valid.then_some(InlineAssemblyContract::new(
        template_value,
        constraints_value,
        clobbers_value,
        features_value,
        options_value,
        operands,
        operand_count,
    )))
}

#[expect(
    clippy::too_many_arguments,
    reason = "operand checking joins the complete structural source and target contract"
)]
fn checked_operands<C>(
    request: CheckerUnitView<'_, C>,
    literals: &CheckedLiteralValues,
    control: TargetControlFacts,
    constraints: &str,
    ranges: &[(usize, usize)],
    inputs_type: TypeId,
    output_type: Option<TypeId>,
    labels_type: Option<TypeId>,
    inputs_expression: BoundExpressionId,
) -> Result<
    Option<([Option<InlineAssemblyOperand>; MAX_INLINE_ASSEMBLY_OPERANDS], u8)>,
    CheckerInfrastructureError,
>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<bray_symbols::CallableSignatureFact>
        + ?Sized,
{
    if ranges.len() > MAX_INLINE_ASSEMBLY_OPERANDS {
        return Ok(None);
    }

    let Some(inputs) = tuple_elements(request, inputs_type)? else {
        return Ok(None);
    };

    let outputs = match output_type {
        Some(output) => {
            let Some(outputs) = tuple_elements(request, output)? else {
                return Ok(None);
            };

            outputs
        }
        None => Vec::new(),
    };

    let labels = match labels_type {
        Some(labels) => {
            let Some(labels) = tuple_elements(request, labels)? else {
                return Ok(None);
            };

            labels
        }
        None => Vec::new(),
    };

    for label in labels.iter().copied() {
        if !label_type_valid(request, label)? {
            return Ok(None);
        }
    }

    let input_expressions = tuple_expression_elements(request, inputs_expression)
        .filter(|elements| elements.len() == inputs.len());

    let pointer_width = request.selected_target().machine().pointer_width_bits().get();
    let mut descriptors = [None; MAX_INLINE_ASSEMBLY_OPERANDS];
    let mut input_index = 0_usize;
    let mut runtime_input_index = 0_usize;
    let mut output_index = 0_usize;
    let mut label_index = 0_usize;
    let mut saw_pure_input = false;

    for (descriptor_index, &(start, length)) in ranges.iter().enumerate() {
        let Some(parsed) = parse_constraint(control, &constraints[start..start + length]) else {
            return Ok(None);
        };

        let has_output = matches!(
            parsed.kind,
            InlineAssemblyOperandKind::Output
                | InlineAssemblyOperandKind::LateOutput
                | InlineAssemblyOperandKind::InOut
                | InlineAssemblyOperandKind::EarlyInOut
        );

        if saw_pure_input && has_output {
            return Ok(None);
        }

        saw_pure_input |= !has_output;

        let (input, output, ty, constant, symbol) = match parsed.kind {
            InlineAssemblyOperandKind::Input
            | InlineAssemblyOperandKind::Immediate
            | InlineAssemblyOperandKind::Symbol
            | InlineAssemblyOperandKind::Memory => {
                let Some(&ty) = inputs.get(input_index) else {
                    return Ok(None);
                };

                let constant = if parsed.kind == InlineAssemblyOperandKind::Immediate {
                    let Some(expressions) = input_expressions.as_ref() else {
                        return Ok(None);
                    };

                    literals.expression(expressions[input_index])
                } else {
                    None
                };

                let symbol = if parsed.kind == InlineAssemblyOperandKind::Symbol {
                    let Some(expressions) = input_expressions.as_ref() else {
                        return Ok(None);
                    };

                    crate::target_control_symbol::callable_symbol(
                        request,
                        expressions[input_index],
                        ty,
                    )?
                } else {
                    None
                };

                (Some(input_index), None, ty, constant, symbol)
            }
            InlineAssemblyOperandKind::Output | InlineAssemblyOperandKind::LateOutput => {
                let Some(&ty) = outputs.get(output_index) else {
                    return Ok(None);
                };

                (None, Some(output_index), ty, None, None)
            }
            InlineAssemblyOperandKind::InOut | InlineAssemblyOperandKind::EarlyInOut => {
                let (Some(&input), Some(&output)) =
                    (inputs.get(input_index), outputs.get(output_index))
                else {
                    return Ok(None);
                };

                if input != output {
                    return Ok(None);
                }

                (Some(input_index), Some(output_index), input, None, None)
            }
            InlineAssemblyOperandKind::Label => {
                let Some(&ty) = labels.get(label_index) else {
                    return Ok(None);
                };

                (None, None, ty, None, None)
            }
        };

        if !operand_type_valid(request, parsed.kind, parsed.class, ty, constant, pointer_width)? {
            return Ok(None);
        }

        if parsed.kind == InlineAssemblyOperandKind::Symbol && symbol.is_none() {
            return Ok(None);
        }

        let Some(start) = u16::try_from(start).ok() else {
            return Ok(None);
        };

        let Some(length) = u16::try_from(length).ok() else {
            return Ok(None);
        };

        let runtime_input = input.and_then(|_| {
            (!matches!(
                parsed.kind,
                InlineAssemblyOperandKind::Immediate | InlineAssemblyOperandKind::Symbol
            ))
            .then_some(runtime_input_index)
        });

        descriptors[descriptor_index] = Some(InlineAssemblyOperand::new(
            parsed.kind,
            ty,
            input.and_then(|value| u16::try_from(value).ok()),
            runtime_input.and_then(|value| u16::try_from(value).ok()),
            output.and_then(|value| u16::try_from(value).ok()),
            constant,
            symbol,
            start,
            length,
        ));

        input_index += usize::from(input.is_some());
        runtime_input_index += usize::from(runtime_input.is_some());
        output_index += usize::from(output.is_some());
        label_index += usize::from(parsed.kind == InlineAssemblyOperandKind::Label);
    }

    if input_index != inputs.len() || output_index != outputs.len() || label_index != labels.len() {
        return Ok(None);
    }

    let Some(count) = u8::try_from(ranges.len()).ok() else {
        return Ok(None);
    };

    Ok(Some((descriptors, count)))
}

fn llvm_operand_count(
    operands: &[Option<InlineAssemblyOperand>; MAX_INLINE_ASSEMBLY_OPERANDS],
    count: usize,
) -> usize {
    operands
        .iter()
        .take(count)
        .flatten()
        .map(|operand| {
            usize::from(operand.output().is_some())
                + usize::from(operand.input().is_some())
                + usize::from(operand.kind() == InlineAssemblyOperandKind::Label)
        })
        .sum()
}

fn literal_string<C>(
    request: CheckerUnitView<'_, C>,
    literals: &CheckedLiteralValues,
    expression: BoundExpressionId,
) -> Result<Option<(ConstantValueId, String)>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(identity) = literals.expression(expression) else {
        return Ok(None);
    };

    let value = request
        .semantic_values()
        .constant_value_data(identity)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(match value.kind() {
        ConstantValueKind::String(text) => Some((identity, text.to_string())),
        _ => None,
    })
}

fn literal_integer<C>(
    request: CheckerUnitView<'_, C>,
    literals: &CheckedLiteralValues,
    expression: BoundExpressionId,
) -> Result<Option<(ConstantValueId, u64)>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(identity) = literals.expression(expression) else {
        return Ok(None);
    };

    let value = request
        .semantic_values()
        .constant_value_data(identity)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(match value.kind() {
        ConstantValueKind::Integer(integer) => {
            integer.to_u64().map(|integer| (identity, integer))
        }
        _ => None,
    })
}

fn operand_type_valid<C>(
    request: CheckerUnitView<'_, C>,
    kind: InlineAssemblyOperandKind,
    class: &str,
    ty: TypeId,
    constant: Option<ConstantValueId>,
    pointer_width: u16,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let representation = crate::representation::type_representation(request, ty)?;

    Ok(match kind {
        InlineAssemblyOperandKind::Immediate => {
            constant.is_some_and(|constant| constant_integer(request, constant).is_some())
                && representation.is_some_and(|role| role.integer_representation().is_some())
        }
        InlineAssemblyOperandKind::Symbol => matches!(
            request
                .semantic_values()
                .type_data(ty)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?
                .as_ref(),
            TypeData::Callable(_)
        ),
        InlineAssemblyOperandKind::Memory => matches!(
            representation,
            Some(RepresentationRole::RawPointer | RepresentationRole::DevicePointer)
        ),
        InlineAssemblyOperandKind::Label => label_type_valid(request, ty)?,
        InlineAssemblyOperandKind::Input
        | InlineAssemblyOperandKind::Output
        | InlineAssemblyOperandKind::LateOutput
        | InlineAssemblyOperandKind::InOut
        | InlineAssemblyOperandKind::EarlyInOut => {
            register_type_valid(representation, class, pointer_width)
        }
    })
}

fn register_type_valid(
    representation: Option<RepresentationRole>,
    class: &str,
    pointer_width: u16,
) -> bool {
    let Some(role) = representation else {
        return false;
    };

    if matches!(role, RepresentationRole::RawPointer | RepresentationRole::DevicePointer) {
        return !floating_register_class(class) && pointer_width <= register_width_limit(class, pointer_width);
    }

    if let Some(integer) = role.integer_representation() {
        let width = match integer {
            IntegerRepresentation::Signed(width) | IntegerRepresentation::Unsigned(width) => width,
            IntegerRepresentation::TargetSigned | IntegerRepresentation::TargetUnsigned => {
                pointer_width
            }
        };

        return !floating_register_class(class) && width <= register_width_limit(class, pointer_width);
    }

    let width = match role {
        RepresentationRole::ScalarR16 => Some(16),
        RepresentationRole::ScalarR32 => Some(32),
        RepresentationRole::ScalarR64 => Some(64),
        RepresentationRole::ScalarR128 => Some(128),
        _ => None,
    };

    width.is_some_and(|width| {
        floating_register_class(class) && width <= register_width_limit(class, pointer_width)
    })
}

fn register_width_limit(class: &str, pointer_width: u16) -> u16 {
    if class == "reg_byte" {
        8
    } else if class == "sreg" {
        32
    } else if matches!(class, "dreg" | "freg") {
        64
    } else if class.contains("xmm") || matches!(class, "qreg" | "vreg") {
        128
    } else if class.contains("ymm") {
        256
    } else if class.contains("zmm") {
        512
    } else {
        pointer_width
    }
}

fn floating_register_class(class: &str) -> bool {
    class.contains("xmm")
        || class.contains("ymm")
        || class.contains("zmm")
        || matches!(class, "x" | "w" | "f" | "v" | "sreg" | "dreg" | "qreg" | "vreg" | "freg")
}

fn label_type_valid<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(callable) = data.as_ref() else {
        return Ok(false);
    };

    Ok(callable.parameters().is_empty()
        && callable.execution() == CallableExecution::Synchronous
        && crate::representation::type_representation(request, callable.result())?
            == Some(RepresentationRole::Never))
}

fn tuple_elements<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<Option<Vec<TypeId>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(match data.as_ref() {
        TypeData::Tuple(elements) => Some(elements.to_vec()),
        _ => None,
    })
}

fn tuple_expression_elements<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Option<Vec<BoundExpressionId>>
where
    C: CheckerRequestContext + ?Sized,
{
    let BoundExpression::Structured(tuple) = request.view().expression(expression)? else {
        return None;
    };

    (tuple.kind() == BoundStructuredExpressionKind::Tuple).then(|| tuple.operands().to_vec())
}

fn constant_integer<C>(
    request: CheckerUnitView<'_, C>,
    identity: ConstantValueId,
) -> Option<u64>
where
    C: CheckerRequestContext + ?Sized,
{
    let value = request.semantic_values().constant_value_data(identity).ok()?;

    let ConstantValueKind::Integer(integer) = value.kind() else {
        return None;
    };

    integer.to_u64()
}

fn literal_memory_order<C>(
    request: CheckerUnitView<'_, C>,
    literals: &CheckedLiteralValues,
    selections: &CheckedSemanticSelections,
    expression: BoundExpressionId,
) -> Result<Option<MemoryOrder>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let variant = if let Some(identity) = literals.expression(expression) {
        let value = request
            .semantic_values()
            .constant_value_data(identity)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let ConstantValueKind::Union { variant, fields } = value.kind() else {
            return Ok(None);
        };

        if !fields.is_empty() {
            return Ok(None);
        }

        *variant
    } else {
        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            selections.expression(expression)
        else {
            return Ok(None);
        };

        let ConstructionTarget::UnionVariant(variant) = construction.target() else {
            return Ok(None);
        };

        if !construction.inputs().is_empty() {
            return Ok(None);
        }

        variant
    };

    let Some(name) = request.member_name(variant.into()).map_err(|error| match error {
        crate::CheckerFactError::Infrastructure(error) => error,
        crate::CheckerFactError::Cancelled => CheckerInfrastructureError::SemanticValueUnavailable,
    })? else {
        return Ok(None);
    };

    Ok(match name.as_str() {
        "Relaxed" => Some(MemoryOrder::Relaxed),
        "Acquire" => Some(MemoryOrder::Acquire),
        "Release" => Some(MemoryOrder::Release),
        "AcquireRelease" => Some(MemoryOrder::AcquireRelease),
        "SequentiallyConsistent" => Some(MemoryOrder::SequentiallyConsistent),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::RepresentationRole;
    use bray_target::{NativeTarget, TargetArchitecture, TargetControlFacts};

    use bray_target::InlineAssemblyOptions;

    use super::{
        clobbers_valid, parse_constraint, register_type_valid, separated_values, template_valid,
    };

    #[test]
    fn constraints_are_target_checked_and_structurally_classified() {
        let control = TargetControlFacts::for_architecture(TargetArchitecture::X86_64);
        use bray_bound_tree::InlineAssemblyOperandKind as Kind;

        assert_eq!(
            parse_constraint(control, "+{rax}").map(|constraint| constraint.kind),
            Some(Kind::InOut)
        );

        assert_eq!(
            parse_constraint(control, "+&reg").map(|constraint| constraint.kind),
            Some(Kind::EarlyInOut)
        );

        assert_eq!(
            parse_constraint(control, "=&reg").map(|constraint| constraint.kind),
            Some(Kind::Output)
        );

        assert_eq!(
            parse_constraint(control, "=reg").map(|constraint| constraint.kind),
            Some(Kind::LateOutput)
        );

        assert_eq!(
            parse_constraint(control, "reg").map(|constraint| constraint.kind),
            Some(Kind::Input)
        );

        assert_eq!(
            parse_constraint(control, "i").map(|constraint| constraint.kind),
            Some(Kind::Immediate)
        );

        assert_eq!(
            parse_constraint(control, "s").map(|constraint| constraint.kind),
            Some(Kind::Symbol)
        );

        assert_eq!(
            parse_constraint(control, "m").map(|constraint| constraint.kind),
            Some(Kind::Memory)
        );

        assert_eq!(
            parse_constraint(control, "label").map(|constraint| constraint.kind),
            Some(Kind::Label)
        );

        assert!(parse_constraint(control, "={x0}").is_none());
        assert!(parse_constraint(control, "&reg").is_none());
    }

    #[test]
    fn register_constraints_validate_exact_representation_classes_and_widths() {
        assert!(register_type_valid(Some(RepresentationRole::ScalarI8), "reg_byte", 64));
        assert!(!register_type_valid(Some(RepresentationRole::ScalarI32), "reg_byte", 64));
        assert!(register_type_valid(Some(RepresentationRole::ScalarI32), "reg", 64));
        assert!(register_type_valid(Some(RepresentationRole::ScalarR32), "xmm_reg", 64));
        assert!(!register_type_valid(Some(RepresentationRole::ScalarR32), "reg", 64));
        assert!(register_type_valid(Some(RepresentationRole::RawPointer), "reg", 64));
        assert!(!register_type_valid(Some(RepresentationRole::DevicePointer), "xmm_reg", 64));
        assert!(!register_type_valid(Some(RepresentationRole::Unit), "reg", 64));
    }

    #[test]
    fn clobbers_require_known_registers_and_coherent_options() {
        let profile = NativeTarget::X86_64WindowsMsvc.profile();
        let control = TargetControlFacts::for_profile(&profile);

        let Some(impure) = InlineAssemblyOptions::try_new(0) else {
            panic!("empty inline assembly options must be valid");
        };

        let Some(pure) = InlineAssemblyOptions::try_new(1) else {
            panic!("pure inline assembly option must be valid");
        };

        assert!(clobbers_valid(control, &["memory", "rax", "abi:C"], impure));
        assert!(!clobbers_valid(control, &["memory"], pure));
        assert!(!clobbers_valid(control, &["x0"], impure));
        assert!(!clobbers_valid(control, &["abi:unknown"], impure));
    }

    #[test]
    fn templates_reference_only_declared_operands() {
        assert!(template_valid("add $0, ${1}", 2));
        assert!(template_valid("literal $$0", 0));
        assert!(!template_valid("add $2, $0", 2));
        assert!(!template_valid("add ${0", 1));
    }

    #[test]
    fn separated_contract_values_reject_implicit_empty_entries() {
        assert_eq!(separated_values("").map(|values| values.len()), Some(0));
        assert_eq!(separated_values("r, =r").map(|values| values.len()), Some(2));
        assert!(separated_values("r,,=r").is_none());
    }
}
