use bray_bound_tree::{InlineAssemblyConstraint, InlineAssemblyOperandKind};
use bray_target::{InlineAssemblyOptions, TargetControlFacts};

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

pub(crate) fn parse_constraint(
    control: TargetControlFacts,
    constraint: &str,
) -> Option<InlineAssemblyConstraint<'_>> {
    let parsed = InlineAssemblyConstraint::try_parse(constraint)?;

    let valid = match parsed.kind() {
        InlineAssemblyOperandKind::Immediate => parsed.class() == "i",
        InlineAssemblyOperandKind::Symbol => parsed.class() == "s",
        InlineAssemblyOperandKind::Memory => parsed.class() == "m",
        InlineAssemblyOperandKind::Input
        | InlineAssemblyOperandKind::Output
        | InlineAssemblyOperandKind::LateOutput
        | InlineAssemblyOperandKind::InOut
        | InlineAssemblyOperandKind::EarlyInOut => {
            if parsed.explicit() {
                control.supports_physical_register(parsed.class())
            } else {
                parsed.class() == "r" || control.register_constraint(parsed.class()).is_some()
            }
        }
        InlineAssemblyOperandKind::Label => true,
    };

    valid.then_some(parsed)
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
