use bray_bound_tree::{InlineAssemblyOperand, InlineAssemblyOperandKind};
use bray_codegen::CodegenFailure;
use bray_target::TargetControlFacts;

pub(super) fn append_clobber(constraints: &mut String, clobber: &str) {
    if !constraints.is_empty() {
        constraints.push(',');
    }

    constraints.push_str("~{");
    constraints.push_str(clobber);
    constraints.push('}');
}

pub(super) fn assembly_constraints(
    control: TargetControlFacts,
    constraints: &str,
    descriptors: &[InlineAssemblyOperand],
) -> Result<String, CodegenFailure> {
    let mut normalized = String::new();

    for descriptor in descriptors.iter().filter(|operand| operand.output().is_some()) {
        let modifier = match descriptor.kind() {
            InlineAssemblyOperandKind::Output | InlineAssemblyOperandKind::EarlyInOut => "=&",
            InlineAssemblyOperandKind::LateOutput | InlineAssemblyOperandKind::InOut => "=",
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };
        append_constraint(&mut normalized, modifier);
        append_constraint_class(
            &mut normalized,
            control,
            operand_constraint(constraints, *descriptor)?,
        )?;
    }

    for descriptor in descriptors.iter().filter(|operand| operand.input().is_some()) {
        if let Some(output) = descriptor.output() {
            append_constraint(&mut normalized, &output.to_string());
        } else {
            let class = operand_constraint(constraints, *descriptor)?;
            append_constraint(&mut normalized, "");
            append_constraint_class(&mut normalized, control, class)?;
        }
    }

    for _ in descriptors
        .iter()
        .filter(|operand| operand.kind() == InlineAssemblyOperandKind::Label)
    {
        append_constraint(&mut normalized, "!i");
    }

    Ok(normalized)
}

pub(super) fn output_descriptors(
    operands: &[InlineAssemblyOperand],
) -> Vec<InlineAssemblyOperand> {
    let mut outputs = operands
        .iter()
        .copied()
        .filter(|operand| operand.output().is_some())
        .collect::<Vec<_>>();

    outputs.sort_by_key(|operand| operand.output());
    outputs
}

fn operand_constraint(
    constraints: &str,
    operand: InlineAssemblyOperand,
) -> Result<&str, CodegenFailure> {
    let (start, length) = operand.constraint_range();
    let start = usize::from(start);
    let end = start
        .checked_add(usize::from(length))
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;
    let constraint = constraints
        .get(start..end)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    Ok(constraint
        .strip_prefix("+&")
        .or_else(|| constraint.strip_prefix("=&"))
        .or_else(|| constraint.strip_prefix(['+', '=']))
        .unwrap_or(constraint))
}

fn append_constraint(constraints: &mut String, constraint: &str) {
    if !constraints.is_empty() {
        constraints.push(',');
    }

    constraints.push_str(constraint);
}

fn append_constraint_class(
    constraints: &mut String,
    control: TargetControlFacts,
    class: &str,
) -> Result<(), CodegenFailure> {
    if class.starts_with('{') || matches!(class, "r" | "i" | "s" | "m") {
        constraints.push_str(class);
    } else {
        constraints.push_str(
            control
                .register_constraint(class)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
        );
    }

    Ok(())
}
