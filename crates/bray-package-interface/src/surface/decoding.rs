use std::str;
use std::sync::Arc;

use bray_symbols::{ImportedSymbolIdentityInput, InterfaceSymbolId, PackageIdentity, SymbolName};

use super::{
    DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceDependency,
    InterfaceProductIdentity, InterfaceSymbolReference, PackageInterfaceIdentity,
    PackageInterfaceSurface, SymbolRelationship,
};
use crate::decode::{DecodeBudget, map_wire_error, read_optional_u32, read_u32};
use crate::tag::WireTag;
use crate::wire::WireReader;
use crate::{
    InterfaceContentHash, InterfaceLimit, InterfaceSectionTag, InterfaceValidationError,
    InterfaceValidationLimits, ValidatedInterfaceSection, ValidatedPackageInterface,
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

    if !is_strictly_sorted(&dependencies)
        || !is_strictly_sorted(&relationships)
        || !is_strictly_sorted(&exports)
    {
        return Err(InterfaceValidationError::Malformed);
    }

    PackageInterfaceSurface::try_new(identity, dependencies, symbols, relationships, exports)
        .map_err(|_| InterfaceValidationError::Malformed)
}

fn required_section(
    interface: &ValidatedPackageInterface,
    tag: InterfaceSectionTag,
) -> Result<ValidatedInterfaceSection<'_>, InterfaceValidationError> {
    interface
        .section(tag)
        .ok_or(InterfaceValidationError::Malformed)
}

fn decode_strings(
    section: ValidatedInterfaceSection<'_>,
    budget: &mut DecodeBudget,
) -> Result<Vec<Arc<str>>, InterfaceValidationError> {
    let count = checked_count(section.record_count())?;

    budget.charge_items::<Arc<str>>(count)?;

    let mut reader = WireReader::new(section.bytes());
    let mut strings = Vec::with_capacity(count);

    for _ in 0..count {
        let length = usize::try_from(read_u32(&mut reader)?)
            .map_err(|_| InterfaceValidationError::Malformed)?;

        budget.limits().check(
            InterfaceLimit::StringLength,
            u64::try_from(length).unwrap_or(u64::MAX),
        )?;
        budget.charge(length)?;

        let bytes = reader.read_bytes(length).map_err(map_wire_error)?;
        let value = str::from_utf8(bytes).map_err(|_| InterfaceValidationError::Malformed)?;

        if value.is_empty()
            || strings
                .last()
                .is_some_and(|previous: &Arc<str>| previous.as_ref() >= value)
        {
            return Err(InterfaceValidationError::Malformed);
        }

        strings.push(Arc::from(value));
    }

    reader.finish().map_err(map_wire_error)?;

    Ok(strings)
}

fn decode_metadata(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
) -> Result<PackageInterfaceIdentity, InterfaceValidationError> {
    if section.record_count() != 1 {
        return Err(InterfaceValidationError::Malformed);
    }

    let mut reader = WireReader::new(section.bytes());
    let package = package_identity(read_string(&mut reader, strings)?)?;
    let product = product_identity(read_string(&mut reader, strings)?)?;
    let kind = read_tag(&mut reader)?;
    let public_surface = read_string(&mut reader, strings)?;

    reader.finish().map_err(map_wire_error)?;

    PackageInterfaceIdentity::try_new(package, product, kind, Arc::clone(public_surface))
        .ok_or(InterfaceValidationError::Malformed)
}

fn decode_dependencies(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<Vec<InterfaceDependency>, InterfaceValidationError> {
    let count = checked_count(section.record_count())?;

    budget.charge_items::<InterfaceDependency>(count)?;

    let mut reader = WireReader::new(section.bytes());
    let mut dependencies = Vec::with_capacity(count);

    for _ in 0..count {
        let package = package_identity(read_string(&mut reader, strings)?)?;
        let product = product_identity(read_string(&mut reader, strings)?)?;
        let hash =
            InterfaceContentHash::from_bytes(reader.read_array::<32>().map_err(map_wire_error)?);

        dependencies.push(InterfaceDependency::new(package, product, hash));
    }

    reader.finish().map_err(map_wire_error)?;

    Ok(dependencies)
}

fn decode_symbols(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<Vec<ImportedSymbolIdentityInput>, InterfaceValidationError> {
    let count = checked_count(section.record_count())?;

    budget.charge_items::<ImportedSymbolIdentityInput>(count)?;

    let mut reader = WireReader::new(section.bytes());
    let mut symbols = Vec::with_capacity(count);

    for index in 0..count {
        let kind = read_tag(&mut reader)?;
        let container = read_optional_u32(&mut reader)?.map(InterfaceSymbolId::new);
        let key = super::reference::decode_local_key_component(
            &mut reader,
            strings,
            kind,
            container,
            &symbols,
        )?;
        let id =
            InterfaceSymbolId::try_from_index(index).ok_or(InterfaceValidationError::Malformed)?;

        symbols.push(ImportedSymbolIdentityInput::new(id, key, kind, container));
    }

    reader.finish().map_err(map_wire_error)?;

    Ok(symbols)
}

fn decode_relationships(
    section: ValidatedInterfaceSection<'_>,
    budget: &mut DecodeBudget,
) -> Result<Vec<SymbolRelationship>, InterfaceValidationError> {
    let count = checked_count(section.record_count())?;

    budget.charge_items::<SymbolRelationship>(count)?;

    let mut reader = WireReader::new(section.bytes());
    let mut relationships = Vec::with_capacity(count);

    for _ in 0..count {
        relationships.push(SymbolRelationship::new(
            read_tag(&mut reader)?,
            InterfaceSymbolId::new(read_u32(&mut reader)?),
            InterfaceSymbolId::new(read_u32(&mut reader)?),
            read_u32(&mut reader)?,
        ));
    }

    reader.finish().map_err(map_wire_error)?;

    Ok(relationships)
}

fn decode_exports(
    section: ValidatedInterfaceSection<'_>,
    strings: &[Arc<str>],
    budget: &mut DecodeBudget,
) -> Result<Vec<ExportedLookupEdge>, InterfaceValidationError> {
    let count = checked_count(section.record_count())?;

    budget.charge_items::<ExportedLookupEdge>(count)?;

    let mut reader = WireReader::new(section.bytes());
    let mut exports = Vec::with_capacity(count);

    for _ in 0..count {
        let owner = InterfaceSymbolId::new(read_u32(&mut reader)?);
        let name = symbol_name(read_string(&mut reader, strings)?)?;
        let kind: ExportedLookupKind = read_tag(&mut reader)?;
        let target = match read_u32(&mut reader)? {
            1 => InterfaceSymbolReference::Local(InterfaceSymbolId::new(read_u32(&mut reader)?)),
            2 => InterfaceSymbolReference::Dependency {
                dependency: DependencyInterfaceId::new(read_u32(&mut reader)?),
                key: super::reference::decode_external_key(&mut reader, strings, budget)?,
            },
            _ => return Err(InterfaceValidationError::Malformed),
        };

        exports.push(ExportedLookupEdge::new(owner, name, kind, target));
    }

    reader.finish().map_err(map_wire_error)?;

    Ok(exports)
}

fn is_strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(super) fn read_string<'a>(
    reader: &mut WireReader<'_>,
    strings: &'a [Arc<str>],
) -> Result<&'a Arc<str>, InterfaceValidationError> {
    usize::try_from(read_u32(reader)?)
        .ok()
        .and_then(|index| strings.get(index))
        .ok_or(InterfaceValidationError::Malformed)
}

pub(super) fn package_identity(
    value: &Arc<str>,
) -> Result<PackageIdentity, InterfaceValidationError> {
    PackageIdentity::try_new(Arc::clone(value)).ok_or(InterfaceValidationError::Malformed)
}

fn product_identity(
    value: &Arc<str>,
) -> Result<InterfaceProductIdentity, InterfaceValidationError> {
    InterfaceProductIdentity::try_new(Arc::clone(value)).ok_or(InterfaceValidationError::Malformed)
}

pub(super) fn symbol_name(value: &Arc<str>) -> Result<SymbolName, InterfaceValidationError> {
    SymbolName::try_new(Arc::clone(value)).ok_or(InterfaceValidationError::Malformed)
}

pub(super) fn read_tag<T: WireTag>(
    reader: &mut WireReader<'_>,
) -> Result<T, InterfaceValidationError> {
    T::from_wire(read_u32(reader)?).ok_or(InterfaceValidationError::Malformed)
}

fn checked_count(count: u64) -> Result<usize, InterfaceValidationError> {
    usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed)
}
