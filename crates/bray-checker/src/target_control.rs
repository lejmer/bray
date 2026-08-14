use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpressionId, CheckedLiteralValues, CheckedMemoryOperationKind, CheckedMemoryOperations,
    InlineAssemblyContract, MemoryReadKind, PointerAddressComparison, VolatileAddressSpace,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{ConstantValueId, ConstantValueKind, TypeId};
use bray_target::{InlineAssemblyOptions, TargetControlFacts};

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

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
        ImplementationHook::CompilerFence => {
            no_types()?;

            CheckedMemoryOperationKind::CompilerFence
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
) -> Result<TargetControlCheck, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
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
        ImplementationHook::InlineAssembly | ImplementationHook::DivergingInlineAssembly => {
            let (input, output) = match (hook, types) {
                (ImplementationHook::InlineAssembly, [input, output]) => (*input, Some(*output)),
                (ImplementationHook::DivergingInlineAssembly, [input]) => (*input, None),
                _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
            };

            Ok(match assembly_contract(
                request,
                arguments,
                literals,
                control,
                output.is_none(),
            )? {
                Some(contract) => TargetControlCheck::Valid(
                    CheckedMemoryOperationKind::InlineAssembly {
                        input,
                        output,
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
    diverges: bool,
) -> Result<Option<InlineAssemblyContract>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let [template, constraints, clobbers, features, options, _input] = arguments else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some((template_value, template)) = literal_string(request, literals, *template)? else {
        return Ok(None);
    };

    let Some((constraints_value, constraints)) = literal_string(request, literals, *constraints)? else {
        return Ok(None);
    };

    let Some((clobbers_value, clobbers)) = literal_string(request, literals, *clobbers)? else {
        return Ok(None);
    };

    let Some((features_value, features)) = literal_string(request, literals, *features)? else {
        return Ok(None);
    };

    let Some((options_value, options)) = literal_integer(request, literals, *options)? else {
        return Ok(None);
    };

    let Some(options) = InlineAssemblyOptions::try_new(options) else {
        return Ok(None);
    };

    let Some(constraints) = separated_values(&constraints) else {
        return Ok(None);
    };

    let Some(clobbers) = separated_values(&clobbers) else {
        return Ok(None);
    };

    let Some(features) = separated_values(&features) else {
        return Ok(None);
    };

    let valid = control.inline_assembly()
        && template_valid(&template, constraints.len())
        && constraints_valid(control, &constraints, diverges)
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
    )))
}

fn template_valid(template: &str, operand_count: usize) -> bool {
    if template.contains('\0') {
        return false;
    }

    let bytes = template.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }

        index += 1;

        if index < bytes.len() && bytes[index] == b'$' {
            index += 1;
            continue;
        }

        let braced = index < bytes.len() && bytes[index] == b'{';
        index += usize::from(braced);
        let start = index;

        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }

        if start == index {
            return false;
        }

        let Ok(ordinal) = template[start..index].parse::<usize>() else {
            return false;
        };

        if ordinal >= operand_count {
            return false;
        }

        if braced {
            if index >= bytes.len() || bytes[index] != b'}' {
                return false;
            }

            index += 1;
        }
    }

    true
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

fn constraints_valid(
    control: TargetControlFacts,
    constraints: &[&str],
    diverges: bool,
) -> bool {
    let mut output_count = 0;
    let mut input_count = 0;
    let mut saw_input = false;

    for constraint in constraints {
        let Some(constraint) = parse_constraint(control, constraint) else {
            return false;
        };

        if constraint.output && saw_input {
            return false;
        }

        output_count += usize::from(constraint.output);
        input_count += usize::from(constraint.input);
        saw_input |= constraint.input && !constraint.output;
    }

    if diverges {
        output_count == 0 && input_count == 1
    } else {
        output_count == 1 && input_count == 1
    }
}

fn clobbers_valid(
    control: TargetControlFacts,
    clobbers: &[&str],
    options: InlineAssemblyOptions,
) -> bool {
    clobbers.iter().copied().all(|clobber| {
        control.supports_clobber(clobber)
            || clobber
                .strip_prefix("abi:")
                .is_some_and(|abi| control.supports_clobber_abi(abi))
            || control.supports_register(clobber)
    }) && !(options.pure() && clobbers.contains(&"memory"))
}

#[derive(Clone, Copy)]
struct AssemblyConstraint {
    input: bool,
    output: bool,
}

fn parse_constraint(
    control: TargetControlFacts,
    constraint: &str,
) -> Option<AssemblyConstraint> {
    let (input, output, class) = if let Some(class) = constraint.strip_prefix("+&") {
        (true, true, class)
    } else if let Some(class) = constraint.strip_prefix('+') {
        (true, true, class)
    } else if let Some(class) = constraint.strip_prefix("=&") {
        (false, true, class)
    } else if let Some(class) = constraint.strip_prefix('=') {
        (false, true, class)
    } else {
        (true, false, constraint)
    };

    if class.is_empty() || class.starts_with(['=', '+', '&', '*', '%']) {
        return None;
    }

    let explicit = class.starts_with('{') || class.ends_with('}');

    let class = if explicit {
        class.strip_prefix('{')?.strip_suffix('}')?
    } else {
        class
    };

    let register = class == "r" || control.supports_register(class);
    let input_kind = register || matches!(class, "i" | "s" | "m");

    (if output { register } else { input_kind }).then_some(AssemblyConstraint { input, output })
}

fn separated_values(value: &str) -> Option<Vec<&str>> {
    if value.is_empty() {
        return Some(Vec::new());
    }

    let values = value.split(',').map(str::trim).collect::<Vec<_>>();

    values.iter().all(|value| !value.is_empty()).then_some(values)
}

fn feature_name_valid(feature: &str) -> bool {
    feature
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use bray_target::{NativeTarget, TargetArchitecture, TargetControlFacts};

    use bray_target::InlineAssemblyOptions;

    use super::{clobbers_valid, constraints_valid, separated_values, template_valid};

    #[test]
    fn constraints_are_target_checked_and_shape_checked() {
        let control = TargetControlFacts::for_architecture(TargetArchitecture::X86_64);

        assert!(constraints_valid(control, &["=r", "r"], false));
        assert!(constraints_valid(control, &["+{rax}"], false));
        assert!(constraints_valid(control, &["i"], true));
        assert!(!constraints_valid(control, &["={x0}", "r"], false));
        assert!(!constraints_valid(control, &["&r", "r"], false));
        assert!(!constraints_valid(control, &["r"], false));
        assert!(!constraints_valid(control, &["r", "=r"], false));
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
