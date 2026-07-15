use std::collections::BTreeSet;

use bray_symbols::{ExternalSymbolKey, PackageIdentity, SymbolKind};

use crate::test_support::insert_key_and_owners;
use crate::{
    ExportSymbolInput, InterfaceContentHash, InterfaceDependency, InterfaceProductIdentity,
    InterfaceProductKind, InterfaceSymbolReference, PackageInterfaceIdentity,
    PackageInterfaceSurface, build_package_interface_surface,
};

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

pub(super) fn local(
    surface: &PackageInterfaceSurface,
    key: &ExternalSymbolKey,
) -> InterfaceSymbolReference {
    let id = surface
        .symbol_by_external_key(key)
        .unwrap_or_else(|| panic!("test symbol must be present in the identity surface"));

    InterfaceSymbolReference::Local(id)
}

pub(super) fn local_by_kind(
    surface: &PackageInterfaceSurface,
    kind: SymbolKind,
) -> InterfaceSymbolReference {
    local(surface, &key_by_kind(surface, kind))
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
