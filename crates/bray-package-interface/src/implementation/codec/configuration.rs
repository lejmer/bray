use bray_base::StableDigestHasher;
use bray_runtime_interface::{
    ExecutionLaneRequirement, PanicAbiIdentity, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion, RuntimeCapability,
    RuntimeIdentity, RuntimeRequirements,
};
use bray_target::{
    Endianness, ObjectFormat, TargetArchitecture, TargetIdentity, TargetMachineProperties,
    TargetPropertyKind,
};
use std::hash::Hasher;
use std::num::{NonZeroU16, NonZeroU32};

use crate::decode::DecodeBudget;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceLimit, InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
};

use super::super::{
    PackageImplementationConfiguration, PackageImplementationTargetProperties,
    PackageImplementationTargetProperty, PackageImplementationTargetPropertyValue,
};
use super::wire::{checked_u32, invalid_discriminant, read_string, wire_error, write_string};

pub(super) fn write_configuration(
    encoder: &mut WireEncoder,
    configuration: &PackageImplementationConfiguration,
) {
    write_target_properties(encoder, configuration.target_properties());

    match configuration.runtime() {
        Some(runtime) => {
            encoder.write_u8(1);
            write_string(encoder, runtime.as_str());
        }
        None => encoder.write_u8(0),
    }

    encoder.write_u16(configuration.runtime_abi().major());
    encoder.write_u16(configuration.runtime_abi().minor());
    write_string(encoder, configuration.panic_abi().as_str());
}

pub(in crate::implementation) fn configuration_identity(
    configuration: &PackageImplementationConfiguration,
) -> [u8; 32] {
    let mut encoder = WireEncoder::new();
    write_configuration(&mut encoder, configuration);

    let mut digest = StableDigestHasher::new();
    digest.write(b"bray.package-implementation.configuration.v1");
    digest.write(encoder.bytes());

    digest.finalize()
}

pub(super) fn read_configuration(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
) -> Result<PackageImplementationConfiguration, InterfaceValidationError> {
    let target = read_target_properties(reader, budget)?;

    let runtime = match reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?
    {
        0 => None,
        1 => Some(
            RuntimeIdentity::try_new(read_string(reader, budget.limits())?).ok_or(
                crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
            )?,
        ),
        actual => {
            return Err(invalid_discriminant(
                InterfaceValidationField::Discriminant,
                u64::from(actual),
            ));
        }
    };

    let runtime_abi = RuntimeAbiVersion::new(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::RuntimeRequirements))?,
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::RuntimeRequirements))?,
    );

    let panic_abi = PanicAbiIdentity::try_new(read_string(reader, budget.limits())?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
    )?;

    Ok(PackageImplementationConfiguration::new(
        target,
        runtime,
        runtime_abi,
        panic_abi,
    ))
}

fn write_target_properties(
    encoder: &mut WireEncoder,
    target: &PackageImplementationTargetProperties,
) {
    write_string(encoder, target.identity().as_str());
    write_machine(encoder, target.machine());
    encoder.write_u16(u16::try_from(target.properties().len()).unwrap_or(u16::MAX));

    for property in target.properties() {
        let ordinal = TargetPropertyKind::ALL
            .iter()
            .position(|kind| *kind == property.kind())
            .and_then(|ordinal| u16::try_from(ordinal).ok())
            .unwrap_or(u16::MAX);

        encoder.write_u16(ordinal);

        match property.value() {
            PackageImplementationTargetPropertyValue::String(value) => {
                encoder.write_u8(0);
                write_string(encoder, value.as_str());
            }
            PackageImplementationTargetPropertyValue::Usize(value) => {
                encoder.write_u8(1);
                encoder.write_u64(*value);
            }
            PackageImplementationTargetPropertyValue::Boolean(value) => {
                encoder.write_u8(2);
                encoder.write_u8(u8::from(*value));
            }
        }
    }
}

fn read_target_properties(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
) -> Result<PackageImplementationTargetProperties, InterfaceValidationError> {
    let identity = TargetIdentity::try_new(read_string(reader, budget.limits())?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
    )?;

    let machine = read_machine(reader)?;

    let count = usize::from(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::RecordCount))?,
    );

    if count != TargetPropertyKind::ALL.len() {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::RecordCount,
        ));
    }

    let mut properties = budget.allocate_items_with_minimum(
        reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::TargetProperty,
        count,
        3,
    )?;

    for expected in TargetPropertyKind::ALL {
        let ordinal = usize::from(
            reader
                .read_u16()
                .map_err(wire_error(InterfaceValidationField::Ordinal))?,
        );

        let kind = TargetPropertyKind::ALL.get(ordinal).copied().ok_or(
            crate::implementation::invalid_value(crate::InterfaceValidationField::Ordinal),
        )?;

        if kind != *expected {
            return Err(crate::implementation::invalid_value(
                crate::InterfaceValidationField::Ordering,
            ));
        }

        let value = match reader
            .read_u8()
            .map_err(wire_error(InterfaceValidationField::Discriminant))?
        {
            0 => PackageImplementationTargetPropertyValue::String(
                bray_base::NonEmptySharedStr::try_new(read_string(reader, budget.limits())?)
                    .ok_or(crate::implementation::invalid_value(
                        crate::InterfaceValidationField::String,
                    ))?,
            ),
            1 => PackageImplementationTargetPropertyValue::Usize(
                reader
                    .read_u64()
                    .map_err(wire_error(InterfaceValidationField::TargetProperty))?,
            ),
            2 => PackageImplementationTargetPropertyValue::Boolean(
                match reader
                    .read_u8()
                    .map_err(wire_error(InterfaceValidationField::Discriminant))?
                {
                    0 => false,
                    1 => true,
                    actual => {
                        return Err(invalid_discriminant(
                            InterfaceValidationField::Discriminant,
                            u64::from(actual),
                        ));
                    }
                },
            ),
            actual => {
                return Err(invalid_discriminant(
                    InterfaceValidationField::Discriminant,
                    u64::from(actual),
                ));
            }
        };

        properties.push(PackageImplementationTargetProperty::new(kind, value));
    }

    PackageImplementationTargetProperties::try_from_parts(identity, machine, properties).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::TargetProperty),
    )
}

fn write_machine(encoder: &mut WireEncoder, machine: &TargetMachineProperties) {
    encoder.write_u8(match machine.architecture() {
        TargetArchitecture::X86 => 0,
        TargetArchitecture::X86_64 => 1,
        TargetArchitecture::Arm => 2,
        TargetArchitecture::Aarch64 => 3,
        TargetArchitecture::Riscv32 => 4,
        TargetArchitecture::Riscv64 => 5,
        TargetArchitecture::PowerPc64 => 6,
        TargetArchitecture::Wasm32 => 7,
        TargetArchitecture::Wasm64 => 8,
    });

    encoder.write_u8(match machine.object_format() {
        ObjectFormat::Coff => 0,
        ObjectFormat::Elf => 1,
        ObjectFormat::MachO => 2,
        ObjectFormat::WebAssembly => 3,
        ObjectFormat::Xcoff => 4,
    });

    encoder.write_u8(match machine.endianness() {
        Endianness::Little => 0,
        Endianness::Big => 1,
    });

    encoder.write_u16(machine.pointer_width_bits().get());
    encoder.write_u32(machine.pointer_alignment_bytes().get());
    encoder.write_u32(machine.stack_alignment_bytes().get());
}

fn read_machine(
    reader: &mut WireReader<'_>,
) -> Result<TargetMachineProperties, InterfaceValidationError> {
    let architecture = match reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?
    {
        0 => TargetArchitecture::X86,
        1 => TargetArchitecture::X86_64,
        2 => TargetArchitecture::Arm,
        3 => TargetArchitecture::Aarch64,
        4 => TargetArchitecture::Riscv32,
        5 => TargetArchitecture::Riscv64,
        6 => TargetArchitecture::PowerPc64,
        7 => TargetArchitecture::Wasm32,
        8 => TargetArchitecture::Wasm64,
        actual => {
            return Err(invalid_discriminant(
                InterfaceValidationField::Discriminant,
                u64::from(actual),
            ));
        }
    };

    let object_format = match reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?
    {
        0 => ObjectFormat::Coff,
        1 => ObjectFormat::Elf,
        2 => ObjectFormat::MachO,
        3 => ObjectFormat::WebAssembly,
        4 => ObjectFormat::Xcoff,
        actual => {
            return Err(invalid_discriminant(
                InterfaceValidationField::Discriminant,
                u64::from(actual),
            ));
        }
    };

    let endianness = match reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?
    {
        0 => Endianness::Little,
        1 => Endianness::Big,
        actual => {
            return Err(invalid_discriminant(
                InterfaceValidationField::Discriminant,
                u64::from(actual),
            ));
        }
    };

    let pointer_width = NonZeroU16::new(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::TargetProperty))?,
    )
    .ok_or(crate::implementation::invalid_value(
        crate::InterfaceValidationField::TargetProperty,
    ))?;

    let pointer_alignment = NonZeroU32::new(
        reader
            .read_u32()
            .map_err(wire_error(InterfaceValidationField::TargetProperty))?,
    )
    .ok_or(crate::implementation::invalid_value(
        crate::InterfaceValidationField::TargetProperty,
    ))?;

    let stack_alignment = NonZeroU32::new(
        reader
            .read_u32()
            .map_err(wire_error(InterfaceValidationField::TargetProperty))?,
    )
    .ok_or(crate::implementation::invalid_value(
        crate::InterfaceValidationField::TargetProperty,
    ))?;

    TargetMachineProperties::try_new(
        architecture,
        object_format,
        endianness,
        pointer_width,
        pointer_alignment,
        stack_alignment,
    )
    .ok_or(crate::implementation::invalid_value(
        crate::InterfaceValidationField::TargetProperty,
    ))
}

pub(super) fn write_runtime_requirements(
    encoder: &mut WireEncoder,
    requirements: &RuntimeRequirements,
) {
    match requirements.runtime() {
        Some(runtime) => {
            encoder.write_u8(1);
            write_string(encoder, runtime.as_str());
        }
        None => encoder.write_u8(0),
    }

    write_version(encoder, requirements.abi_version());

    match requirements.frame_abi() {
        Some(frame_abi) => {
            encoder.write_u8(1);

            for operation in ProtectedFrameAbiOperation::ALL {
                write_version(encoder, frame_abi.operation(operation));
            }
        }
        None => encoder.write_u8(0),
    }

    write_string(encoder, requirements.target().as_str());
    write_string(encoder, requirements.panic_abi().as_str());
    write_ordinals(encoder, RuntimeAbiRole::ALL, requirements.roles());
    write_ordinals(encoder, RuntimeCapability::ALL, requirements.capabilities());

    write_ordinals(
        encoder,
        [
            ExecutionLaneRequirement::Blocking,
            ExecutionLaneRequirement::Compute,
            ExecutionLaneRequirement::MainThread,
        ],
        requirements.lanes(),
    );
}

pub(in crate::implementation) fn runtime_requirements_identity<'a>(
    requirements: impl IntoIterator<Item = &'a RuntimeRequirements>,
) -> [u8; 32] {
    let mut requirements = requirements.into_iter().collect::<Vec<_>>();

    requirements.sort_unstable();

    let mut encoder = WireEncoder::new();

    encoder.write_u32(checked_u32(requirements.len()));

    for requirement in requirements {
        write_runtime_requirements(&mut encoder, requirement);
    }

    let mut digest = StableDigestHasher::new();

    digest.write(b"bray.package-implementation.runtime-requirements.v1");
    digest.write(encoder.bytes());

    digest.finalize()
}

pub(super) fn read_runtime_requirements(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
) -> Result<RuntimeRequirements, InterfaceValidationError> {
    let runtime = match reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?
    {
        0 => None,
        1 => Some(
            RuntimeIdentity::try_new(read_string(reader, budget.limits())?).ok_or(
                crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
            )?,
        ),
        actual => {
            return Err(invalid_discriminant(
                InterfaceValidationField::Discriminant,
                u64::from(actual),
            ));
        }
    };

    let abi_version = read_version(reader)?;

    let frame_abi = match reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?
    {
        0 => None,
        1 => Some(ProtectedFrameAbiVersions::new(
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
        )),
        actual => {
            return Err(invalid_discriminant(
                InterfaceValidationField::Discriminant,
                u64::from(actual),
            ));
        }
    };

    let target = TargetIdentity::try_new(read_string(reader, budget.limits())?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
    )?;

    let panic_abi = PanicAbiIdentity::try_new(read_string(reader, budget.limits())?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
    )?;

    let roles = read_ordinals(reader, budget, RuntimeAbiRole::ALL)?;
    let capabilities = read_ordinals(reader, budget, RuntimeCapability::ALL)?;

    let lanes = read_ordinals(
        reader,
        budget,
        [
            ExecutionLaneRequirement::Blocking,
            ExecutionLaneRequirement::Compute,
            ExecutionLaneRequirement::MainThread,
        ],
    )?;

    Ok(RuntimeRequirements::new(
        runtime,
        abi_version,
        frame_abi,
        target,
        panic_abi,
        roles,
        capabilities,
        lanes,
    ))
}

fn write_version(encoder: &mut WireEncoder, version: RuntimeAbiVersion) {
    encoder.write_u16(version.major());
    encoder.write_u16(version.minor());
}

fn read_version(
    reader: &mut WireReader<'_>,
) -> Result<RuntimeAbiVersion, InterfaceValidationError> {
    Ok(RuntimeAbiVersion::new(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::RuntimeRequirements))?,
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::RuntimeRequirements))?,
    ))
}

fn write_ordinals<T: Copy + Eq, const N: usize>(
    encoder: &mut WireEncoder,
    universe: [T; N],
    values: &[T],
) {
    encoder.write_u32(checked_u32(values.len()));

    for value in values {
        let ordinal = universe
            .iter()
            .position(|candidate| candidate == value)
            .and_then(|ordinal| u16::try_from(ordinal).ok())
            .unwrap_or(u16::MAX);

        encoder.write_u16(ordinal);
    }
}

pub(super) fn read_ordinals<T: Copy + Ord, const N: usize>(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
    universe: [T; N],
) -> Result<Vec<T>, InterfaceValidationError> {
    let count = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::RecordCount))?;

    budget
        .limits()
        .check(InterfaceLimit::RecordCount, u64::from(count))?;

    let count = usize::try_from(count).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordCount)
    })?;

    let mut values = budget.allocate_items_with_minimum(
        reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::Ordinal,
        count,
        2,
    )?;

    for _ in 0..count {
        let ordinal = usize::from(
            reader
                .read_u16()
                .map_err(wire_error(InterfaceValidationField::Ordinal))?,
        );

        let value = universe
            .get(ordinal)
            .copied()
            .ok_or(crate::implementation::invalid_value(
                crate::InterfaceValidationField::Ordinal,
            ))?;

        values.push(value);
    }

    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Ordering,
        ));
    }

    Ok(values)
}
