use bray_bound_tree::{
    InlineAssemblyConstraint, InlineAssemblyOperand, InlineAssemblyOperandKind,
};
use bray_codegen::CodegenFailure;
use bray_target::TargetControlSupport;

pub(super) fn append_clobber(constraints: &mut String, clobber: &str) {
    if !constraints.is_empty() {
        constraints.push(',');
    }

    constraints.push_str("~{");
    constraints.push_str(clobber);
    constraints.push('}');
}

pub(super) fn assembly_constraints(
    control: TargetControlSupport,
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
            descriptor.kind(),
        )?;
    }

    for descriptor in descriptors.iter().filter(|operand| operand.input().is_some()) {
        if let Some(output) = descriptor.output() {
            append_constraint(&mut normalized, &output.to_string());
        } else {
            let constraint = operand_constraint(constraints, *descriptor)?;
            append_constraint(&mut normalized, "");

            append_constraint_class(
                &mut normalized,
                control,
                constraint,
                descriptor.kind(),
            )?;
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
) -> Result<InlineAssemblyConstraint<'_>, CodegenFailure> {
    let (start, length) = operand.constraint_range();

    let start = usize::from(start);

    let end = start
        .checked_add(usize::from(length))
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let constraint = constraints
        .get(start..end)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    InlineAssemblyConstraint::try_parse(constraint)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
}

fn append_constraint(constraints: &mut String, constraint: &str) {
    if !constraints.is_empty() {
        constraints.push(',');
    }

    constraints.push_str(constraint);
}

fn append_constraint_class(
    constraints: &mut String,
    control: TargetControlSupport,
    constraint: InlineAssemblyConstraint<'_>,
    kind: InlineAssemblyOperandKind,
) -> Result<(), CodegenFailure> {
    if kind == InlineAssemblyOperandKind::Memory {
        constraints.push('*');
    }

    let class = constraint.class();

    if constraint.explicit() {
        if !control.supports_physical_register(class) {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        constraints.push('{');
        constraints.push_str(class);
        constraints.push('}');
    } else if matches!(class, "r" | "i" | "s" | "m") {
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
