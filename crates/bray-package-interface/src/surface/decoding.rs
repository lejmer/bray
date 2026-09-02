use std::str;
use std::sync::Arc;

use bray_symbols::{
    CallablePosition, ImportedSymbolIdentityInput, InterfaceSymbolId, PackageIdentity,
    PackageVersion, SymbolName,
};

use super::{
    DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceDependency,
    InterfaceProductIdentity, InterfaceSymbolReference, PackageInterfaceIdentity,
    PackageInterfaceSurface, SymbolRelationship,
};
use crate::decode::{DecodeBudget, read_optional_u32, read_u32, wire_error};
use crate::tag::WireTag;
use crate::validation::is_strictly_sorted;
use crate::wire::WireReader;
use crate::{
    InterfaceContentHash, InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause,
    InterfaceSectionTag, InterfaceUtf8Failure, InterfaceValidationContext,
    InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
    ValidatedInterfaceSection, ValidatedPackageInterface,
};

pub(crate) fn decode_surface(
    interface: &ValidatedPackageInterface,
    limits: InterfaceValidationLimits,
) -> Result<PackageInterfaceSurface, InterfaceValidationError> {
    let strings_section = required_section(interface, InterfaceSectionTag::Strings)?;
    let metadata_section = required_section(interface, InterfaceSectionTag::PackageMetadata)?;
    let dependencies_section = required_section(interface, InterfaceSectionTag::Dependencies)?;
    let symbols_section = required_section(interface, InterfaceSectionTag::SymbolIdentities)?;
    let relationships_section = required_section(interface, InterfaceSectionTag::Relationships)?;
    let exports_section = required_section(interface, InterfaceSectionTag::ExportedLookup)?;

    decode_sections(
        IdentitySections {
            strings: strings_section,
            metadata: metadata_section,
            dependencies: dependencies_section,
            symbols: symbols_section,
            relationships: relationships_section,
            exports: exports_section,
        },
        limits,
    )
}

pub(super) struct IdentitySections<'bytes> {
    pub(super) strings: ValidatedInterfaceSection<'bytes>,
    pub(super) metadata: ValidatedInterfaceSection<'bytes>,
    pub(super) dependencies: ValidatedInterfaceSection<'bytes>,
    pub(super) symbols: ValidatedInterfaceSection<'bytes>,
    pub(super) relationships: ValidatedInterfaceSection<'bytes>,
    pub(super) exports: ValidatedInterfaceSection<'bytes>,
}

pub(super) fn decode_sections(
    sections: IdentitySections<'_>,
    limits: InterfaceValidationLimits,
) -> Result<PackageInterfaceSurface, InterfaceValidationError> {
    let mut budget = DecodeBudget::new(limits);

    let strings = decode_strings(sections.strings, &mut budget)?;
    let identity = decode_metadata(sections.metadata, &strings)?;
    let dependencies = decode_dependencies(sections.dependencies, &strings, &mut budget)?;
    let symbols = decode_symbols(sections.symbols, &strings, &mut budget)?;
    let relationships = decode_relationships(sections.relationships, &mut budget)?;
    let exports = decode_exports(sections.exports, &strings, &mut budget)?;

    validate_order(&dependencies, InterfaceSectionTag::Dependencies)?;
    validate_order(&relationships, InterfaceSectionTag::Relationships)?;
    validate_order(&exports, InterfaceSectionTag::ExportedLookup)?;

    PackageInterfaceSurface::try_new(identity, dependencies, symbols, relationships, exports)
        .map_err(|cause| InterfaceValidationError::SurfaceBuild {
            cause: Box::new(cause),
        })
}

fn required_section(
    interface: &ValidatedPackageInterface,
    tag: InterfaceSectionTag,
) -> Result<ValidatedInterfaceSection<'_>, InterfaceValidationError> {
    interface.section(tag)?.ok_or_else(|| {
        malformed(
            InterfaceValidationContext::Artifact,
            InterfaceMalformedCause::Missing {
                field: section_field(tag),
            },
        )
    })
}

pub(crate) fn decode_strings(
    section: ValidatedInterfaceSection<'_>,
    budget: &mut DecodeBudget,
) -> Result<Vec<Arc<str>>, InterfaceValidationError> {
    let context = section_context(section);

    let count = checked_count(
        section.record_count(),
        context,
        InterfaceValidationField::RecordCount,
    )?;

    let mut reader = WireReader::new(section.bytes());

    let mut strings =
        budget.allocate_items(&reader, context, InterfaceValidationField::String, count)?;

    for index in 0..count {
        let record_context = InterfaceValidationContext::Record {
            section: section.tag(),
            index: index as u64,
        };

        let raw_length = read_u32(
            &mut reader,
            record_context,
            InterfaceValidationField::RecordLength,
        )?;

        let length = usize::try_from(raw_length).map_err(|_| {
            numeric_overflow(
                record_context,
                InterfaceValidationField::RecordLength,
                u64::from(raw_length),
                InterfaceIntegerTarget::Usize,
            )
        })?;

        budget.limits().check(
            InterfaceLimit::StringLength,
            u64::try_from(length).unwrap_or(u64::MAX),
        )?;

        budget.charge(length)?;

        let offset = reader.position();

        let bytes = reader
            .read_bytes(length)
            .map_err(wire_error(record_context, InterfaceValidationField::String))?;

        let value =
            str::from_utf8(bytes).map_err(|cause| InterfaceValidationError::InvalidUtf8 {
                context: record_context,
                field: InterfaceValidationField::String,
                offset: offset as u64,
                length: length as u64,
                cause: cause.error_len().map_or(
                    InterfaceUtf8Failure::IncompleteSequence,
                    |error_length| InterfaceUtf8Failure::InvalidSequence {
                        error_length: Some(error_length as u64),
                    },
                ),
            })?;

        if value.is_empty() {
            return Err(invalid_value(
                record_context,
                InterfaceValidationField::String,
            ));
        }

        if strings
            .last()
            .is_some_and(|previous: &Arc<str>| previous.as_ref() >= value)
        {
            return Err(malformed(
                record_context,
                InterfaceMalformedCause::OrderingViolation {
                    field: InterfaceValidationField::String,
                    previous: index.saturating_sub(1) as u64,
                    actual: index as u64,
                },
            ));
        }

        strings.push(Arc::from(value));
    }

    reader
        .finish()
        .map_err(wire_error(context, InterfaceValidationField::RecordPayload))?;

    Ok(strings)
}

pub(crate) fn decode_metadata(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
) -> Result<PackageInterfaceIdentity, InterfaceValidationError> {
    let context = section_context(section);

    if section.record_count() != 1 {
        return Err(malformed(
            context,
            InterfaceMalformedCause::CountMismatch {
                field: InterfaceValidationField::RecordCount,
                expected: 1,
                actual: section.record_count(),
            },
        ));
    }

    let mut reader = WireReader::new(section.bytes());

    let package = package_identity(read_string(&mut reader, strings, context)?, context)?;
    let version = package_version(read_string(&mut reader, strings, context)?, context)?;
    let product = product_identity(read_string(&mut reader, strings, context)?, context)?;

    let kind = read_tag(&mut reader, context, InterfaceValidationField::Discriminant)?;
    let public_surface = read_string(&mut reader, strings, context)?;

    reader
        .finish()
        .map_err(wire_error(context, InterfaceValidationField::RecordPayload))?;

    PackageInterfaceIdentity::try_new(package, version, product, kind, Arc::clone(public_surface))
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::Identity))
}

pub(crate) fn decode_dependencies(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<Vec<InterfaceDependency>, InterfaceValidationError> {
    let context = section_context(section);

    let count = checked_count(
        section.record_count(),
        context,
        InterfaceValidationField::RecordCount,
    )?;

    let mut reader = WireReader::new(section.bytes());

    let mut dependencies = budget.allocate_items(
        &reader,
        context,
        InterfaceValidationField::Dependency,
        count,
    )?;

    for index in 0..count {
        let record_context = record_context(section, index);

        let package = package_identity(
            read_string(&mut reader, strings, record_context)?,
            record_context,
        )?;

        let product = product_identity(
            read_string(&mut reader, strings, record_context)?,
            record_context,
        )?;

        let hash = InterfaceContentHash::from_bytes(reader.read_array::<32>().map_err(
            wire_error(record_context, InterfaceValidationField::ContentHash),
        )?);

        dependencies.push(InterfaceDependency::new(package, product, hash));
    }

    reader
        .finish()
        .map_err(wire_error(context, InterfaceValidationField::RecordPayload))?;

    Ok(dependencies)
}

pub(crate) fn decode_symbols(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<Vec<ImportedSymbolIdentityInput>, InterfaceValidationError> {
    let context = section_context(section);

    let count = checked_count(
        section.record_count(),
        context,
        InterfaceValidationField::RecordCount,
    )?;

    let mut reader = WireReader::new(section.bytes());

    let mut symbols =
        budget.allocate_items(&reader, context, InterfaceValidationField::Identity, count)?;

    for index in 0..count {
        let record_context = record_context(section, index);

        let kind = read_tag(
            &mut reader,
            record_context,
            InterfaceValidationField::SymbolKind,
        )?;

        let container = read_optional_u32(
            &mut reader,
            record_context,
            InterfaceValidationField::Container,
        )?
        .map(InterfaceSymbolId::new);

        let key = super::reference::decode_local_key_component(
            &mut reader,
            strings,
            kind,
            container,
            &symbols,
            budget,
            record_context,
        )?;

        let id = InterfaceSymbolId::try_from_index(index).ok_or_else(|| {
            numeric_overflow(
                record_context,
                InterfaceValidationField::Index,
                index as u64,
                InterfaceIntegerTarget::U32,
            )
        })?;

        symbols.push(ImportedSymbolIdentityInput::new(id, key, kind, container));
    }

    reader
        .finish()
        .map_err(wire_error(context, InterfaceValidationField::RecordPayload))?;

    Ok(symbols)
}

pub(crate) fn decode_relationships(
    section: ValidatedInterfaceSection<'_>,
    budget: &mut DecodeBudget,
) -> Result<Vec<SymbolRelationship>, InterfaceValidationError> {
    let context = section_context(section);

    let count = checked_count(
        section.record_count(),
        context,
        InterfaceValidationField::RecordCount,
    )?;

    let mut reader = WireReader::new(section.bytes());

    let mut relationships =
        budget.allocate_items(&reader, context, InterfaceValidationField::Reference, count)?;

    for index in 0..count {
        let record_context = record_context(section, index);

        let relationship = SymbolRelationship::new(
            read_tag(
                &mut reader,
                record_context,
                InterfaceValidationField::Discriminant,
            )?,
            InterfaceSymbolId::new(read_u32(
                &mut reader,
                record_context,
                InterfaceValidationField::Owner,
            )?),
            InterfaceSymbolId::new(read_u32(
                &mut reader,
                record_context,
                InterfaceValidationField::Subject,
            )?),
            read_u32(
                &mut reader,
                record_context,
                InterfaceValidationField::Ordinal,
            )?,
        )
        .with_position({
            let actual = read_u32(&mut reader, record_context, InterfaceValidationField::Role)?;

            CallablePosition::from_wire(actual).ok_or_else(|| {
                invalid_discriminant(record_context, InterfaceValidationField::Role, actual)
            })?
        });

        let actual = read_u32(&mut reader, record_context, InterfaceValidationField::Value)?;

        let relationship = match actual {
            0 => relationship,
            1 => relationship.with_mutation(),
            _ => {
                return Err(invalid_discriminant(
                    record_context,
                    InterfaceValidationField::Value,
                    actual,
                ));
            }
        };

        relationships.push(relationship);
    }

    reader
        .finish()
        .map_err(wire_error(context, InterfaceValidationField::RecordPayload))?;

    Ok(relationships)
}

pub(crate) fn decode_exports(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<Vec<ExportedLookupEdge>, InterfaceValidationError> {
    let context = section_context(section);

    let count = checked_count(
        section.record_count(),
        context,
        InterfaceValidationField::RecordCount,
    )?;

    let mut reader = WireReader::new(section.bytes());

    let mut exports =
        budget.allocate_items(&reader, context, InterfaceValidationField::Reference, count)?;

    for index in 0..count {
        let record_context = record_context(section, index);

        let owner = InterfaceSymbolId::new(read_u32(
            &mut reader,
            record_context,
            InterfaceValidationField::Owner,
        )?);

        let name = symbol_name(
            read_string(&mut reader, strings, record_context)?,
            record_context,
        )?;

        let kind: ExportedLookupKind = read_tag(
            &mut reader,
            record_context,
            InterfaceValidationField::SymbolKind,
        )?;

        let actual = read_u32(
            &mut reader,
            record_context,
            InterfaceValidationField::Discriminant,
        )?;

        let target = match actual {
            1 => InterfaceSymbolReference::Local(InterfaceSymbolId::new(read_u32(
                &mut reader,
                record_context,
                InterfaceValidationField::Reference,
            )?)),
            2 => InterfaceSymbolReference::Dependency {
                dependency: DependencyInterfaceId::new(read_u32(
                    &mut reader,
                    record_context,
                    InterfaceValidationField::Dependency,
                )?),
                key: super::reference::decode_external_key(
                    &mut reader,
                    strings,
                    budget,
                    record_context,
                )?,
            },
            3 => {
                let key = super::reference::decode_compiler_known_key(
                    &mut reader,
                    strings,
                    budget,
                    record_context,
                )?;

                let reference =
                    crate::CompilerKnownSymbolReference::try_new(key).ok_or_else(|| {
                        invalid_value(record_context, InterfaceValidationField::Reference)
                    })?;

                InterfaceSymbolReference::CompilerKnown(reference)
            }
            _ => {
                return Err(invalid_discriminant(
                    record_context,
                    InterfaceValidationField::Discriminant,
                    actual,
                ));
            }
        };

        exports.push(ExportedLookupEdge::new(owner, name, kind, target));
    }

    reader
        .finish()
        .map_err(wire_error(context, InterfaceValidationField::RecordPayload))?;

    Ok(exports)
}

pub(super) fn read_string<'a>(
    reader: &mut WireReader<'_>,
    strings: &'a [Arc<str>],
    context: InterfaceValidationContext,
) -> Result<&'a Arc<str>, InterfaceValidationError> {
    let raw = read_u32(reader, context, InterfaceValidationField::StringIndex)?;

    let index = usize::try_from(raw).map_err(|_| {
        numeric_overflow(
            context,
            InterfaceValidationField::StringIndex,
            u64::from(raw),
            InterfaceIntegerTarget::Usize,
        )
    })?;

    strings.get(index).ok_or_else(|| {
        malformed(
            context,
            InterfaceMalformedCause::InvalidReference {
                field: InterfaceValidationField::StringIndex,
                index: u64::from(raw),
                available: strings.len() as u64,
            },
        )
    })
}

pub(super) fn package_identity(
    value: &Arc<str>,
    context: InterfaceValidationContext,
) -> Result<PackageIdentity, InterfaceValidationError> {
    PackageIdentity::try_new(Arc::clone(value))
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::PackageName))
}

fn package_version(
    value: &str,
    context: InterfaceValidationContext,
) -> Result<PackageVersion, InterfaceValidationError> {
    PackageVersion::try_new(value)
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::PackageVersion))
}

fn product_identity(
    value: &Arc<str>,
    context: InterfaceValidationContext,
) -> Result<InterfaceProductIdentity, InterfaceValidationError> {
    InterfaceProductIdentity::try_new(Arc::clone(value))
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::ProductName))
}

pub(super) fn symbol_name(
    value: &Arc<str>,
    context: InterfaceValidationContext,
) -> Result<SymbolName, InterfaceValidationError> {
    SymbolName::try_new(Arc::clone(value))
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::SymbolName))
}

pub(super) fn read_tag<T: WireTag>(
    reader: &mut WireReader<'_>,
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> Result<T, InterfaceValidationError> {
    let actual = read_u32(reader, context, field)?;

    T::from_wire(actual).ok_or_else(|| invalid_discriminant(context, field, actual))
}

fn checked_count(
    count: u64,
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> Result<usize, InterfaceValidationError> {
    usize::try_from(count)
        .map_err(|_| numeric_overflow(context, field, count, InterfaceIntegerTarget::Usize))
}

fn section_context(section: ValidatedInterfaceSection<'_>) -> InterfaceValidationContext {
    InterfaceValidationContext::Section(section.tag())
}

fn record_context(
    section: ValidatedInterfaceSection<'_>,
    index: usize,
) -> InterfaceValidationContext {
    InterfaceValidationContext::Record {
        section: section.tag(),
        index: index as u64,
    }
}

fn section_field(tag: InterfaceSectionTag) -> InterfaceValidationField {
    match tag {
        InterfaceSectionTag::Strings => InterfaceValidationField::String,
        InterfaceSectionTag::PackageMetadata => InterfaceValidationField::Identity,
        InterfaceSectionTag::Dependencies => InterfaceValidationField::Dependency,
        InterfaceSectionTag::SymbolIdentities => InterfaceValidationField::SymbolName,
        InterfaceSectionTag::Relationships => InterfaceValidationField::Reference,
        InterfaceSectionTag::ExportedLookup => InterfaceValidationField::Reference,
        _ => InterfaceValidationField::Value,
    }
}

fn validate_order<T: Ord>(
    values: &[T],
    section: InterfaceSectionTag,
) -> Result<(), InterfaceValidationError> {
    if let Some(index) = values.windows(2).position(|pair| pair[0] >= pair[1]) {
        return Err(malformed(
            InterfaceValidationContext::Record {
                section,
                index: (index + 1) as u64,
            },
            InterfaceMalformedCause::OrderingViolation {
                field: InterfaceValidationField::Ordering,
                previous: index as u64,
                actual: (index + 1) as u64,
            },
        ));
    }

    debug_assert!(is_strictly_sorted(values));

    Ok(())
}

pub(super) const fn malformed(
    context: InterfaceValidationContext,
    cause: InterfaceMalformedCause,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed { context, cause }
}

pub(super) const fn invalid_value(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> InterfaceValidationError {
    malformed(context, InterfaceMalformedCause::InvalidValue { field })
}

pub(super) const fn invalid_discriminant(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    actual: u32,
) -> InterfaceValidationError {
    malformed(
        context,
        InterfaceMalformedCause::InvalidDiscriminant {
            field,
            actual: actual as u64,
        },
    )
}

const fn numeric_overflow(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    value: u64,
    target: InterfaceIntegerTarget,
) -> InterfaceValidationError {
    malformed(
        context,
        InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target,
        },
    )
}
