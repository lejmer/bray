use std::hash::Hasher;
use std::str;
use std::sync::Arc;

use bray_base::StableDigestHasher;
use bray_runtime_interface::{
    ExecutionLaneRequirement, PanicAbiIdentity, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion, RuntimeCapability,
    RuntimeIdentity, RuntimeRequirements,
};
use bray_symbols::{PackageIdentity, PackageVersion};
use bray_target::TargetIdentity;

use crate::decode::map_wire_error;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceContentHash, InterfaceDependency, InterfaceLimit, InterfaceProductIdentity,
    InterfaceProductKind, InterfaceValidationError, InterfaceValidationLimits,
    PackageInterfaceIdentity,
};

use super::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    ImplementationSpecializationArgument, ImplementationSpecializationArgumentKind,
    ImplementationSpecializationWitness, PackageImplementationConfiguration,
    PackageImplementationIdentity, PackageImplementationSpecializationKey,
};

pub(super) fn encode_identity(identity: &PackageImplementationIdentity) -> Vec<u8> {
    let mut encoder = WireEncoder::new();
    let interface = identity.interface();

    write_string(&mut encoder, interface.package().as_str());
    write_string(&mut encoder, &interface.version().to_string());
    write_string(&mut encoder, interface.product().as_str());
    encoder.write_u32(product_kind_to_wire(interface.kind()));
    write_string(&mut encoder, interface.public_surface());
    encoder.write_bytes(identity.interface_content_hash().as_bytes());
    encoder.write_u16(identity.language_revision().raw());
    encoder.write_u16(identity.template_schema_revision().raw());

    write_configuration(&mut encoder, identity.configuration());
    encoder.write_bytes(identity.runtime_requirements_identity());
    encoder.write_u32(checked_u32(identity.runtime_requirements().len()));

    for requirements in identity.runtime_requirements() {
        write_runtime_requirements(&mut encoder, requirements);
    }

    encoder.write_u32(checked_u32(identity.dependencies().len()));

    for dependency in identity.dependencies() {
        write_dependency(&mut encoder, dependency);
    }

    encoder.into_bytes()
}

pub(super) fn decode_identity(
    bytes: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<PackageImplementationIdentity, InterfaceValidationError> {
    let mut reader = WireReader::new(bytes);

    let package = PackageIdentity::try_new(read_string(&mut reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let version = PackageVersion::try_new(&read_string(&mut reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let product = InterfaceProductIdentity::try_new(read_string(&mut reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let kind = product_kind_from_wire(reader.read_u32().map_err(map_wire_error)?)?;
    let public_surface = read_string(&mut reader, limits)?;

    let interface = PackageInterfaceIdentity::try_new(
        package,
        version,
        product,
        kind,
        public_surface,
    )
    .ok_or(InterfaceValidationError::Malformed)?;

    let interface_content_hash =
        InterfaceContentHash::from_bytes(reader.read_array::<32>().map_err(map_wire_error)?);

    let language_revision =
        crate::InterfaceLanguageRevision::new(reader.read_u16().map_err(map_wire_error)?);

    let template_schema_revision = reader.read_u16().map_err(map_wire_error)?;

    if template_schema_revision != CURRENT_TEMPLATE_SCHEMA_REVISION.raw() {
        return Err(InterfaceValidationError::Malformed);
    }

    let configuration = read_configuration(&mut reader, limits)?;
    let expected_runtime_requirements_identity =
        reader.read_array::<32>().map_err(map_wire_error)?;
    let runtime_requirements_count = reader.read_u32().map_err(map_wire_error)?;

    limits.check(
        InterfaceLimit::RecordCount,
        u64::from(runtime_requirements_count),
    )?;

    let runtime_requirements_count = usize::try_from(runtime_requirements_count)
        .map_err(|_| InterfaceValidationError::Malformed)?;

    let mut runtime_requirements = Vec::with_capacity(runtime_requirements_count);

    for _ in 0..runtime_requirements_count {
        runtime_requirements.push(read_runtime_requirements(&mut reader, limits)?);
    }

    if runtime_requirements.windows(2).any(|pair| pair[0] >= pair[1])
        || runtime_requirements_identity(runtime_requirements.iter())
            != expected_runtime_requirements_identity
    {
        return Err(InterfaceValidationError::Malformed);
    }

    let dependency_count = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::RecordCount, u64::from(dependency_count))?;

    let dependency_count =
        usize::try_from(dependency_count).map_err(|_| InterfaceValidationError::Malformed)?;

    let mut dependencies = Vec::with_capacity(dependency_count);

    for _ in 0..dependency_count {
        dependencies.push(read_dependency(&mut reader, limits)?);
    }

    reader.finish().map_err(map_wire_error)?;

    if dependencies.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(PackageImplementationIdentity::new(
        interface,
        interface_content_hash,
        language_revision,
        dependencies,
        configuration,
        runtime_requirements,
    ))
}

pub(super) fn encode_specialization_key(
    key: &PackageImplementationSpecializationKey,
    encoder: &mut WireEncoder,
) {
    encoder.write_bytes(key.declaration().as_bytes());
    write_arguments(encoder, key.substitution());
    encoder.write_u32(checked_u32(key.witnesses().len()));

    for witness in key.witnesses() {
        encoder.write_bytes(witness.definition().as_bytes());
        write_arguments(encoder, witness.substitution());
    }

    write_configuration(encoder, key.configuration());
    encoder.write_u16(key.template_schema_revision().raw());
    encoder.write_u32(checked_u32(key.dependencies().len()));

    for dependency in key.dependencies() {
        write_dependency(encoder, dependency);
    }
}

pub(super) fn decode_specialization_key(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<PackageImplementationSpecializationKey, InterfaceValidationError> {
    let declaration = ImplementationExternalSymbolIdentity::from_bytes(
        reader.read_array::<32>().map_err(map_wire_error)?,
    );

    let substitution = read_arguments(reader, limits)?;
    let witness_count = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::RecordCount, u64::from(witness_count))?;

    let witness_count =
        usize::try_from(witness_count).map_err(|_| InterfaceValidationError::Malformed)?;

    let mut witnesses = Vec::with_capacity(witness_count);

    for _ in 0..witness_count {
        let definition = ImplementationExternalSymbolIdentity::from_bytes(
            reader.read_array::<32>().map_err(map_wire_error)?,
        );

        witnesses.push(ImplementationSpecializationWitness::new(
            definition,
            read_arguments(reader, limits)?,
        ));
    }

    if witnesses.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(InterfaceValidationError::Malformed);
    }

    let configuration = read_configuration(reader, limits)?;
    let template_schema_revision =
        super::ImplementationTemplateSchemaRevision::new(reader.read_u16().map_err(map_wire_error)?);

    if template_schema_revision != CURRENT_TEMPLATE_SCHEMA_REVISION {
        return Err(InterfaceValidationError::Malformed);
    }

    let dependency_count = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::RecordCount, u64::from(dependency_count))?;

    let dependency_count =
        usize::try_from(dependency_count).map_err(|_| InterfaceValidationError::Malformed)?;

    let mut dependencies = Vec::with_capacity(dependency_count);

    for _ in 0..dependency_count {
        dependencies.push(read_dependency(reader, limits)?);
    }

    if dependencies.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(PackageImplementationSpecializationKey::new(
        declaration,
        substitution,
        witnesses,
        configuration,
        template_schema_revision,
        dependencies,
    ))
}

fn write_configuration(
    encoder: &mut WireEncoder,
    configuration: &PackageImplementationConfiguration,
) {
    write_string(encoder, configuration.target().as_str());
    encoder.write_bytes(configuration.target_properties());

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

fn read_configuration(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<PackageImplementationConfiguration, InterfaceValidationError> {
    let target = TargetIdentity::try_new(read_string(reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let target_properties = reader.read_array::<32>().map_err(map_wire_error)?;

    let runtime = match reader.read_u8().map_err(map_wire_error)? {
        0 => None,
        1 => Some(
            RuntimeIdentity::try_new(read_string(reader, limits)?)
                .ok_or(InterfaceValidationError::Malformed)?,
        ),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let runtime_abi = RuntimeAbiVersion::new(
        reader.read_u16().map_err(map_wire_error)?,
        reader.read_u16().map_err(map_wire_error)?,
    );

    let panic_abi = PanicAbiIdentity::try_new(read_string(reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    Ok(PackageImplementationConfiguration::new(
        target,
        target_properties,
        runtime,
        runtime_abi,
        panic_abi,
    ))
}

fn write_runtime_requirements(encoder: &mut WireEncoder, requirements: &RuntimeRequirements) {
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

pub(super) fn runtime_requirements_identity<'a>(
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

fn read_runtime_requirements(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<RuntimeRequirements, InterfaceValidationError> {
    let runtime = match reader.read_u8().map_err(map_wire_error)? {
        0 => None,
        1 => Some(
            RuntimeIdentity::try_new(read_string(reader, limits)?)
                .ok_or(InterfaceValidationError::Malformed)?,
        ),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let abi_version = read_version(reader)?;

    let frame_abi = match reader.read_u8().map_err(map_wire_error)? {
        0 => None,
        1 => Some(ProtectedFrameAbiVersions::new(
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
        )),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let target = TargetIdentity::try_new(read_string(reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let panic_abi = PanicAbiIdentity::try_new(read_string(reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let roles = read_ordinals(reader, limits, RuntimeAbiRole::ALL)?;
    let capabilities = read_ordinals(reader, limits, RuntimeCapability::ALL)?;
    let lanes = read_ordinals(
        reader,
        limits,
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
        reader.read_u16().map_err(map_wire_error)?,
        reader.read_u16().map_err(map_wire_error)?,
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

fn read_ordinals<T: Copy + Ord, const N: usize>(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    universe: [T; N],
) -> Result<Vec<T>, InterfaceValidationError> {
    let count = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::RecordCount, u64::from(count))?;

    let count = usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed)?;
    let mut values = Vec::with_capacity(count);

    for _ in 0..count {
        let ordinal = usize::from(reader.read_u16().map_err(map_wire_error)?);
        let value = universe
            .get(ordinal)
            .copied()
            .ok_or(InterfaceValidationError::Malformed)?;

        values.push(value);
    }

    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(values)
}

fn write_arguments(encoder: &mut WireEncoder, arguments: &[ImplementationSpecializationArgument]) {
    encoder.write_u32(checked_u32(arguments.len()));

    for argument in arguments {
        encoder.write_u8(match argument.kind() {
            ImplementationSpecializationArgumentKind::Type => 0,
            ImplementationSpecializationArgumentKind::Constant => 1,
        });

        encoder.write_bytes(&argument.identity());
    }
}

fn read_arguments(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<Vec<ImplementationSpecializationArgument>, InterfaceValidationError> {
    let count = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::RecordCount, u64::from(count))?;

    let count = usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed)?;
    let mut arguments = Vec::with_capacity(count);

    for _ in 0..count {
        let kind = match reader.read_u8().map_err(map_wire_error)? {
            0 => ImplementationSpecializationArgumentKind::Type,
            1 => ImplementationSpecializationArgumentKind::Constant,
            _ => return Err(InterfaceValidationError::Malformed),
        };

        let identity = reader.read_array::<32>().map_err(map_wire_error)?;

        arguments.push(ImplementationSpecializationArgument::new(kind, identity));
    }

    Ok(arguments)
}

fn write_dependency(encoder: &mut WireEncoder, dependency: &InterfaceDependency) {
    write_string(encoder, dependency.package().as_str());
    write_string(encoder, dependency.product().as_str());
    encoder.write_bytes(dependency.content_hash().as_bytes());
}

fn read_dependency(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceDependency, InterfaceValidationError> {
    let package = PackageIdentity::try_new(read_string(reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let product = InterfaceProductIdentity::try_new(read_string(reader, limits)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let content_hash =
        InterfaceContentHash::from_bytes(reader.read_array::<32>().map_err(map_wire_error)?);

    Ok(InterfaceDependency::new(package, product, content_hash))
}

fn write_string(encoder: &mut WireEncoder, value: &str) {
    encoder.write_u32(checked_u32(value.len()));
    encoder.write_bytes(value.as_bytes());
}

fn read_string(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<Arc<str>, InterfaceValidationError> {
    let length = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::StringLength, u64::from(length))?;

    let length = usize::try_from(length).map_err(|_| InterfaceValidationError::Malformed)?;
    let bytes = reader.read_bytes(length).map_err(map_wire_error)?;
    let value = str::from_utf8(bytes).map_err(|_| InterfaceValidationError::Malformed)?;

    if value.is_empty() {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(Arc::from(value))
}

const fn product_kind_to_wire(kind: InterfaceProductKind) -> u32 {
    match kind {
        InterfaceProductKind::Library => 0,
        InterfaceProductKind::Executable => 1,
        InterfaceProductKind::Test => 2,
    }
}

const fn product_kind_from_wire(raw: u32) -> Result<InterfaceProductKind, InterfaceValidationError> {
    match raw {
        0 => Ok(InterfaceProductKind::Library),
        1 => Ok(InterfaceProductKind::Executable),
        2 => Ok(InterfaceProductKind::Test),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn checked_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
