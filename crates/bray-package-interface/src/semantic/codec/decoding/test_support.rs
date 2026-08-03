use std::collections::BTreeSet;
use std::ops::Range;

use bray_symbols::{ExternalSymbolKey, PackageIdentity, SymbolKind};

use crate::test_support::{insert_key_and_owners, package_version};
use crate::{
    EncodedSemanticSection, ExportSymbolInput, InterfaceContentHash, InterfaceDependency,
    InterfaceProductIdentity, InterfaceProductKind, InterfaceSectionTag, PackageInterfaceIdentity,
    PackageInterfaceSurface, ValidatedInterfaceSection, build_package_interface_surface,
};

pub(super) use crate::test_support::local_by_kind;
pub(super) type OwnedSection = (InterfaceSectionTag, u64, Vec<u8>);

pub(super) fn owned_section_views(sections: &[OwnedSection]) -> Vec<ValidatedInterfaceSection<'_>> {
    sections
        .iter()
        .map(|(tag, count, payload)| ValidatedInterfaceSection::for_test(*tag, *count, payload))
        .collect()
}

pub(super) fn encoded_section_views(
    sections: &[EncodedSemanticSection],
) -> Vec<ValidatedInterfaceSection<'_>> {
    sections
        .iter()
        .map(|section| {
            ValidatedInterfaceSection::for_test(
                section.tag(),
                section.record_count(),
                section.payload(),
            )
        })
        .collect()
}

pub(super) fn owned_sections(sections: &[EncodedSemanticSection]) -> Vec<OwnedSection> {
    sections
        .iter()
        .map(|section| {
            (
                section.tag(),
                section.record_count(),
                section.payload().to_vec(),
            )
        })
        .collect()
}

pub(super) fn record_range(
    payload: &[u8],
    table_index: usize,
    record_index: usize,
) -> Range<usize> {
    let table = table_range(payload, table_index);
    let count = read_u32(payload, table.start) as usize;

    if record_index >= count {
        panic!("test record index must exist");
    }

    let directory_start = table.start + 4;
    let payload_start = directory_start + count * 8;
    let entry = directory_start + record_index * 8;
    let offset = read_u32(payload, entry) as usize;
    let length = read_u32(payload, entry + 4) as usize;

    payload_start + offset..payload_start + offset + length
}

pub(super) fn record_range_with_local_owner(
    payload: &[u8],
    table_index: usize,
    owner: bray_symbols::InterfaceSymbolId,
) -> Range<usize> {
    let table = table_range(payload, table_index);
    let count = read_u32(payload, table.start) as usize;

    (0..count)
        .map(|record| record_range(payload, table_index, record))
        .find(|range| {
            read_u32(payload, range.start) == 1 && read_u32(payload, range.start + 4) == owner.raw()
        })
        .unwrap_or_else(|| panic!("test record owner must exist"))
}

pub(super) fn append_record(payload: &mut Vec<u8>, table_index: usize, record: &[u8]) {
    let table = table_range(payload, table_index);
    let count = read_u32(payload, table.start) as usize;
    let directory_start = table.start + 4;
    let payload_start = directory_start + count * 8;
    let payload_length = table.end - payload_start;

    let mut replacement = Vec::with_capacity(table.len() + 8 + record.len());

    replacement.extend_from_slice(
        &u32::try_from(count + 1)
            .unwrap_or_else(|_| panic!("test record count must fit the wire format"))
            .to_le_bytes(),
    );

    replacement.extend_from_slice(&payload[directory_start..payload_start]);

    replacement.extend_from_slice(
        &u32::try_from(payload_length)
            .unwrap_or_else(|_| panic!("test table length must fit the wire format"))
            .to_le_bytes(),
    );

    replacement.extend_from_slice(
        &u32::try_from(record.len())
            .unwrap_or_else(|_| panic!("test record length must fit the wire format"))
            .to_le_bytes(),
    );

    replacement.extend_from_slice(&payload[payload_start..table.end]);
    replacement.extend_from_slice(record);

    payload.splice(table, replacement);
}

pub(super) fn record_directory_entry(
    payload: &[u8],
    table_index: usize,
    record_index: usize,
) -> Range<usize> {
    let table = table_range(payload, table_index);
    let count = read_u32(payload, table.start) as usize;

    if record_index >= count {
        panic!("test record index must exist");
    }

    let start = table.start + 4 + record_index * 8;

    start..start + 8
}

fn table_range(payload: &[u8], table_index: usize) -> Range<usize> {
    let mut table_start = 0_usize;

    for current_table in 0..=table_index {
        let count = read_u32(payload, table_start) as usize;
        let directory_start = table_start + 4;
        let payload_start = directory_start + count * 8;

        let payload_length = if count == 0 {
            0
        } else {
            let last = directory_start + (count - 1) * 8;

            read_u32(payload, last) as usize + read_u32(payload, last + 4) as usize
        };

        let table_end = payload_start + payload_length;

        if current_table == table_index {
            return table_start..table_end;
        }

        table_start = table_end;
    }

    unreachable!("requested test table must exist")
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let end = offset + 4;

    let bytes = bytes
        .get(offset..end)
        .unwrap_or_else(|| panic!("test record framing must be complete"));

    u32::from_le_bytes(
        bytes
            .try_into()
            .unwrap_or_else(|_| panic!("test record scalar must be complete")),
    )
}

pub(super) fn interface_surface(
    package: PackageIdentity,
    symbols: impl IntoIterator<Item = ExternalSymbolKey>,
    dependencies: impl IntoIterator<Item = PackageIdentity>,
) -> PackageInterfaceSurface {
    let mut keys = BTreeSet::new();

    for symbol in symbols {
        insert_key_and_owners(&mut keys, symbol);
    }

    keys.insert(ExternalSymbolKey::package(package.clone()));

    let records = keys
        .iter()
        .map(|key| ExportSymbolInput::new(key.clone(), key.owner().cloned()));

    let dependencies = dependencies.into_iter().map(|dependency| {
        InterfaceDependency::new(
            dependency,
            product_identity("dependency"),
            InterfaceContentHash::from_bytes([1; 32]),
        )
    });

    build_package_interface_surface(
        package_interface_identity(package),
        dependencies,
        records,
        [],
        [],
    )
    .unwrap_or_else(|error| panic!("test interface surface must be valid: {error:?}"))
}

pub(super) fn key_by_kind(
    surface: &PackageInterfaceSurface,
    kind: SymbolKind,
) -> ExternalSymbolKey {
    surface
        .symbols()
        .symbols()
        .iter()
        .find(|symbol| symbol.kind() == kind)
        .map(|symbol| symbol.key().clone())
        .unwrap_or_else(|| panic!("test symbol kind must be present"))
}

fn package_interface_identity(package: PackageIdentity) -> PackageInterfaceIdentity {
    PackageInterfaceIdentity::try_new(
        package,
        package_version(),
        product_identity("library"),
        InterfaceProductKind::Library,
        "test-surface",
    )
    .unwrap_or_else(|| panic!("test package interface identity must be valid"))
}

fn product_identity(value: &str) -> InterfaceProductIdentity {
    InterfaceProductIdentity::try_new(value)
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}
