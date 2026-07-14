use std::collections::BTreeSet;

use bray_symbols::{
    ExternalSymbolKey, ImportedSymbolIdentityInput, InterfaceSymbolId, ModulePathKey,
    PackageIdentity, SymbolKind, SymbolName,
};

use crate::{
    InterfaceContentHash, InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind,
    InterfaceSymbolReference, PackageInterfaceIdentity, PackageInterfaceSurface,
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

    let keys: Vec<_> = keys.into_iter().collect();
    let records = keys.iter().enumerate().map(|(index, key)| {
        let container = key
            .owner()
            .and_then(|owner| keys.binary_search(owner).ok().map(interface_symbol_id));

        ImportedSymbolIdentityInput::new(
            interface_symbol_id(index),
            key.clone(),
            key.kind(),
            container,
        )
    });

    let dependencies = dependencies.into_iter().map(|dependency| {
        InterfaceDependency::new(
            dependency,
            product_identity("dependency"),
            InterfaceContentHash::from_bytes([1; 32]),
        )
    });

    PackageInterfaceSurface::try_new(
        package_interface_identity(package),
        dependencies,
        records,
        [],
        [],
    )
    .unwrap_or_else(|error| panic!("test interface surface must be valid: {error:?}"))
}

fn interface_symbol_id(index: usize) -> InterfaceSymbolId {
    InterfaceSymbolId::new(
        u32::try_from(index)
            .unwrap_or_else(|error| panic!("test symbol count must fit interface IDs: {error:?}")),
    )
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

pub(super) fn module_key(package: PackageIdentity, segment: &str) -> ExternalSymbolKey {
    let path = ModulePathKey::try_new([segment])
        .unwrap_or_else(|| panic!("test module path must be valid"));

    ExternalSymbolKey::module(ExternalSymbolKey::package(package), path)
        .unwrap_or_else(|| panic!("test module key must be valid"))
}

pub(super) fn named_key(
    owner: ExternalSymbolKey,
    kind: SymbolKind,
    name: &str,
) -> ExternalSymbolKey {
    let name =
        SymbolName::try_new(name).unwrap_or_else(|| panic!("test symbol name must be valid"));

    ExternalSymbolKey::named(owner, kind, name)
        .unwrap_or_else(|| panic!("test symbol key must be valid"))
}

fn insert_key_and_owners(keys: &mut BTreeSet<ExternalSymbolKey>, key: ExternalSymbolKey) {
    let mut current = Some(key);

    while let Some(key) = current {
        current = key.owner().cloned();
        keys.insert(key);
    }
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
