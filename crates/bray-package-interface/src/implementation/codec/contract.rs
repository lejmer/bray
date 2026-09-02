use bray_base::StableDigestHasher;
use bray_symbols::{PackageIdentity, PackageVersion};
use std::hash::Hasher;

use crate::decode::DecodeBudget;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceContentHash, InterfaceLimit, InterfaceProductIdentity, InterfaceValidationContext,
    InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
    PackageInterfaceIdentity,
};

use super::super::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    ImplementationSpecializationWitness, PackageImplementationIdentity,
    PackageImplementationSpecializationKey,
};
use super::wire::{
    checked_u32, product_kind_from_wire, product_kind_to_wire, read_arguments, read_dependency,
    read_string, wire_error, write_arguments, write_dependency, write_string,
};
use crate::external_key::{read_external_key, write_external_key};

use super::configuration::{
    read_configuration, read_runtime_requirements, runtime_requirements_identity,
    write_configuration, write_runtime_requirements,
};

pub(in crate::implementation) fn encode_identity(
    identity: &PackageImplementationIdentity,
) -> Vec<u8> {
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

pub(in crate::implementation) fn decode_identity(
    bytes: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<PackageImplementationIdentity, InterfaceValidationError> {
    let mut reader = WireReader::new(bytes);
    let mut budget = DecodeBudget::new(limits);

    let package = PackageIdentity::try_new(read_string(&mut reader, limits)?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::PackageName),
    )?;

    let version = PackageVersion::try_new(&read_string(&mut reader, limits)?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::PackageVersion),
    )?;

    let product = InterfaceProductIdentity::try_new(read_string(&mut reader, limits)?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::ProductName),
    )?;

    let kind = product_kind_from_wire(
        reader
            .read_u32()
            .map_err(wire_error(InterfaceValidationField::Discriminant))?,
    )?;

    let public_surface = read_string(&mut reader, limits)?;

    let interface =
        PackageInterfaceIdentity::try_new(package, version, product, kind, public_surface).ok_or(
            crate::implementation::invalid_value(crate::InterfaceValidationField::Identity),
        )?;

    let interface_content_hash = InterfaceContentHash::from_bytes(
        reader
            .read_array::<32>()
            .map_err(wire_error(InterfaceValidationField::ContentHash))?,
    );

    let language_revision = crate::InterfaceLanguageRevision::new(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::LanguageRevision))?,
    );

    let template_schema_revision = reader
        .read_u16()
        .map_err(wire_error(InterfaceValidationField::SchemaRevision))?;

    if template_schema_revision != CURRENT_TEMPLATE_SCHEMA_REVISION.raw() {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::SchemaRevision,
        ));
    }

    let configuration = read_configuration(&mut reader, &mut budget)?;

    let expected_runtime_requirements_identity = reader
        .read_array::<32>()
        .map_err(wire_error(InterfaceValidationField::Hash))?;

    let runtime_requirements_count = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::RecordCount))?;

    limits.check(
        InterfaceLimit::RecordCount,
        u64::from(runtime_requirements_count),
    )?;

    let runtime_requirements_count = usize::try_from(runtime_requirements_count).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordCount)
    })?;

    let mut runtime_requirements = budget.allocate_items_with_minimum(
        &reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::RuntimeRequirements,
        runtime_requirements_count,
        26,
    )?;

    for _ in 0..runtime_requirements_count {
        runtime_requirements.push(read_runtime_requirements(&mut reader, &mut budget)?);
    }

    if runtime_requirements
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || runtime_requirements_identity(runtime_requirements.iter())
            != expected_runtime_requirements_identity
    {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::RuntimeRequirements,
        ));
    }

    let dependency_count = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::RecordCount))?;

    limits.check(InterfaceLimit::RecordCount, u64::from(dependency_count))?;

    let dependency_count = usize::try_from(dependency_count).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordCount)
    })?;

    let mut dependencies = budget.allocate_items_with_minimum(
        &reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::Dependency,
        dependency_count,
        40,
    )?;

    for _ in 0..dependency_count {
        dependencies.push(read_dependency(&mut reader, limits)?);
    }

    reader
        .finish()
        .map_err(wire_error(InterfaceValidationField::Value))?;

    if dependencies.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Ordering,
        ));
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

pub(in crate::implementation) fn encode_specialization_key(
    key: &PackageImplementationSpecializationKey,
    encoder: &mut WireEncoder,
) {
    write_external_key(encoder, key.declaration().key());
    write_arguments(encoder, key.substitution());
    encoder.write_u32(checked_u32(key.witnesses().len()));

    for witness in key.witnesses() {
        write_external_key(encoder, witness.definition().key());
        write_arguments(encoder, witness.substitution());
    }

    write_configuration(encoder, key.configuration());
    encoder.write_u16(key.template_schema_revision().raw());
    encoder.write_u32(checked_u32(key.dependencies().len()));

    for dependency in key.dependencies() {
        write_dependency(encoder, dependency);
    }
}

pub(in crate::implementation) fn decode_specialization_key(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<PackageImplementationSpecializationKey, InterfaceValidationError> {
    let mut budget = DecodeBudget::new(limits);

    let declaration =
        ImplementationExternalSymbolIdentity::new(&read_external_key(reader, &mut budget)?);

    let substitution = read_arguments(reader, &mut budget)?;

    let witness_count = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::RecordCount))?;

    limits.check(InterfaceLimit::RecordCount, u64::from(witness_count))?;

    let witness_count = usize::try_from(witness_count).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordCount)
    })?;

    let mut witnesses = budget.allocate_items_with_minimum(
        reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::Reference,
        witness_count,
        12,
    )?;

    for _ in 0..witness_count {
        let definition =
            ImplementationExternalSymbolIdentity::new(&read_external_key(reader, &mut budget)?);

        witnesses.push(ImplementationSpecializationWitness::new(
            definition,
            read_arguments(reader, &mut budget)?,
        ));
    }

    if witnesses.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Ordering,
        ));
    }

    let configuration = read_configuration(reader, &mut budget)?;

    let template_schema_revision = super::super::ImplementationTemplateSchemaRevision::new(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::SchemaRevision))?,
    );

    if template_schema_revision != CURRENT_TEMPLATE_SCHEMA_REVISION {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::SchemaRevision,
        ));
    }

    let dependency_count = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::RecordCount))?;

    limits.check(InterfaceLimit::RecordCount, u64::from(dependency_count))?;

    let dependency_count = usize::try_from(dependency_count).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordCount)
    })?;

    let mut dependencies = budget.allocate_items_with_minimum(
        reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::Dependency,
        dependency_count,
        40,
    )?;

    for _ in 0..dependency_count {
        dependencies.push(read_dependency(reader, limits)?);
    }

    if dependencies.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Ordering,
        ));
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

pub(in crate::implementation) fn specialization_key_identity(
    key: &PackageImplementationSpecializationKey,
) -> [u8; 32] {
    let mut encoder = WireEncoder::new();

    encode_specialization_key(key, &mut encoder);

    let mut digest = StableDigestHasher::new();

    digest.write(b"bray.package-implementation.specialization.v2");
    digest.write(encoder.bytes());

    digest.finalize()
}
