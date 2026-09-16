use bray_bound_tree::{InlineAssemblyConstraint, InlineAssemblyOperand, InlineAssemblyOperandKind};
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
) -> String {
    let mut normalized = String::new();

    for descriptor in descriptors
        .iter()
        .filter(|operand| operand.output().is_some())
    {
        let modifier = match descriptor.kind() {
            InlineAssemblyOperandKind::Output | InlineAssemblyOperandKind::EarlyInOut => "=&",
            InlineAssemblyOperandKind::LateOutput | InlineAssemblyOperandKind::InOut => "=",
            unexpected => panic!(
                "checked MIR memory translation violated an established compiler contract: {unexpected:?}"
            ),
        };

        append_constraint(&mut normalized, modifier);

        append_constraint_class(
            &mut normalized,
            control,
            operand_constraint(constraints, *descriptor),
            descriptor.kind(),
        );
    }

    for descriptor in descriptors
        .iter()
        .filter(|operand| operand.input().is_some())
    {
        if let Some(output) = descriptor.output() {
            append_constraint(&mut normalized, &output.to_string());
        } else {
            let constraint = operand_constraint(constraints, *descriptor);
            append_constraint(&mut normalized, "");

            append_constraint_class(&mut normalized, control, constraint, descriptor.kind());
        }
    }

    for _ in descriptors
        .iter()
        .filter(|operand| operand.kind() == InlineAssemblyOperandKind::Label)
    {
        append_constraint(&mut normalized, "!i");
    }

    normalized
}

pub(super) fn output_descriptors(operands: &[InlineAssemblyOperand]) -> Vec<InlineAssemblyOperand> {
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
) -> InlineAssemblyConstraint<'_> {
    let (start, length) = operand.constraint_range();

    let start = usize::from(start);

    let end = start.checked_add(usize::from(length)).unwrap_or_else(|| {
        panic!("inline-assembly constraint range overflows: start={start}, length={length}")
    });

    let constraint = constraints.get(start..end).unwrap_or_else(|| {
        panic!("inline-assembly operand constraint range {start}..{end} is outside {constraints:?}")
    });

    InlineAssemblyConstraint::try_parse(constraint).unwrap_or_else(|| {
        panic!("checked inline assembly retained invalid constraint {constraint:?}")
    })
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
) {
    if kind == InlineAssemblyOperandKind::Memory {
        constraints.push('*');
    }

    let class = constraint.class();

    if constraint.explicit() {
        if !control.supports_physical_register(class) {
            panic!("target control does not support inline-assembly register {class:?}");
        }

        constraints.push('{');
        constraints.push_str(class);
        constraints.push('}');
    } else if matches!(class, "r" | "i" | "s" | "m") {
        constraints.push_str(class);
    } else {
        constraints.push_str(control.register_constraint(class).unwrap_or_else(|| {
            panic!("target control has no inline-assembly register class for {class:?}")
        }));
    }
}
