use bray_bound_tree::InlineAssemblyOperandKind;
use bray_target::{InlineAssemblyOptions, TargetControlFacts};

pub(crate) fn template_valid(template: &str, operand_count: usize) -> bool {
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

pub(crate) fn clobbers_valid(
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
pub(crate) struct AssemblyConstraint<'constraint> {
    pub(crate) kind: InlineAssemblyOperandKind,
    pub(crate) class: &'constraint str,
}

pub(crate) fn parse_constraint(
    control: TargetControlFacts,
    constraint: &str,
) -> Option<AssemblyConstraint<'_>> {
    let (kind, class) = if constraint == "label" {
        (InlineAssemblyOperandKind::Label, "label")
    } else if let Some(class) = constraint.strip_prefix("+&") {
        (InlineAssemblyOperandKind::EarlyInOut, class)
    } else if let Some(class) = constraint.strip_prefix('+') {
        (InlineAssemblyOperandKind::InOut, class)
    } else if let Some(class) = constraint.strip_prefix("=&") {
        (InlineAssemblyOperandKind::Output, class)
    } else if let Some(class) = constraint.strip_prefix('=') {
        (InlineAssemblyOperandKind::LateOutput, class)
    } else if constraint == "i" {
        (InlineAssemblyOperandKind::Immediate, constraint)
    } else if constraint == "s" {
        (InlineAssemblyOperandKind::Symbol, constraint)
    } else if constraint == "m" {
        (InlineAssemblyOperandKind::Memory, constraint)
    } else {
        (InlineAssemblyOperandKind::Input, constraint)
    };

    if kind == InlineAssemblyOperandKind::Label {
        return Some(AssemblyConstraint { kind, class });
    }

    if class.is_empty() || class.starts_with(['=', '+', '&', '*', '%']) {
        return None;
    }

    let explicit = class.starts_with('{') || class.ends_with('}');

    let class = if explicit {
        class.strip_prefix('{')?.strip_suffix('}')?
    } else {
        class
    };

    let valid = match kind {
        InlineAssemblyOperandKind::Immediate => class == "i",
        InlineAssemblyOperandKind::Symbol => class == "s",
        InlineAssemblyOperandKind::Memory => class == "m",
        InlineAssemblyOperandKind::Input
        | InlineAssemblyOperandKind::Output
        | InlineAssemblyOperandKind::LateOutput
        | InlineAssemblyOperandKind::InOut
        | InlineAssemblyOperandKind::EarlyInOut => {
            class == "r" || control.supports_register(class)
        }
        InlineAssemblyOperandKind::Label => false,
    };

    valid.then_some(AssemblyConstraint { kind, class })
}

pub(crate) fn separated_ranges(value: &str) -> Option<Vec<(usize, usize)>> {
    if value.is_empty() {
        return Some(Vec::new());
    }

    let mut ranges = Vec::new();
    let mut offset = 0_usize;

    for part in value.split(',') {
        let trimmed = part.trim();

        if trimmed.is_empty() {
            return None;
        }

        let leading = part.len() - part.trim_start().len();
        ranges.push((offset + leading, trimmed.len()));
        offset += part.len() + 1;
    }

    Some(ranges)
}

pub(crate) fn separated_values(value: &str) -> Option<Vec<&str>> {
    if value.is_empty() {
        return Some(Vec::new());
    }

    let values = value.split(',').map(str::trim).collect::<Vec<_>>();

    values.iter().all(|value| !value.is_empty()).then_some(values)
}

pub(crate) fn feature_name_valid(feature: &str) -> bool {
    feature
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-' | b'.'))
}
