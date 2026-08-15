use std::collections::BTreeMap;

use bray_symbols::{ImportedSymbolIdentityInput, InterfaceSymbolId};

use super::{
    ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
    PackageInterfaceExportSurfaceError,
};
use crate::{
    ExportedLookupEdge, InterfaceDependency, InterfaceSymbolReference, PackageInterfaceIdentity,
    PackageInterfaceSurface, SymbolRelationship,
};

/// Creates a public surface with canonical symbol identities.
pub fn build_package_interface_surface(
    identity: PackageInterfaceIdentity,
    dependencies: impl IntoIterator<Item = InterfaceDependency>,
    symbols: impl IntoIterator<Item = ExportSymbolInput>,
    relationships: impl IntoIterator<Item = ExportRelationshipInput>,
    exports: impl IntoIterator<Item = ExportLookupInput>,
) -> Result<PackageInterfaceSurface, PackageInterfaceExportSurfaceError> {
    let (symbols, symbol_ids) = canonical_symbols(symbols)?;

    let relationships = resolve_relationships(relationships, &symbol_ids)?;
    let exports = resolve_exports(exports, &symbol_ids)?;

    PackageInterfaceSurface::try_new(identity, dependencies, symbols, relationships, exports)
        .map_err(PackageInterfaceExportSurfaceError::Surface)
}

fn canonical_symbols(
    symbols: impl IntoIterator<Item = ExportSymbolInput>,
) -> Result<
    (
        Vec<ImportedSymbolIdentityInput>,
        BTreeMap<bray_symbols::ExternalSymbolKey, InterfaceSymbolId>,
    ),
    PackageInterfaceExportSurfaceError,
> {
    // External keys are Arc-backed stable identities shared by inputs, indexes, and records.
    let mut selected = BTreeMap::new();

    for symbol in symbols {
        let key = symbol.key().clone();

        if selected.insert(key.clone(), symbol).is_some() {
            return Err(PackageInterfaceExportSurfaceError::DuplicateSymbol(key));
        }
    }

    let symbol_ids = selected
        .keys()
        .enumerate()
        .map(|(index, key)| {
            let id = InterfaceSymbolId::try_from_index(index)
                .ok_or(PackageInterfaceExportSurfaceError::SymbolCountOverflow)?;

            Ok((key.clone(), id))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let records = selected
        .into_values()
        .map(|symbol| {
            let id = resolve_symbol(&symbol_ids, symbol.key())?;

            let containing = symbol
                .containing_symbol()
                .map(|owner| resolve_symbol(&symbol_ids, owner))
                .transpose()?;

            Ok(ImportedSymbolIdentityInput::new(
                id,
                symbol.key().clone(),
                symbol.key().kind(),
                containing,
            ))
        })
        .collect::<Result<Vec<_>, PackageInterfaceExportSurfaceError>>()?;

    Ok((records, symbol_ids))
}

fn resolve_relationships(
    relationships: impl IntoIterator<Item = ExportRelationshipInput>,
    symbols: &BTreeMap<bray_symbols::ExternalSymbolKey, InterfaceSymbolId>,
) -> Result<Vec<SymbolRelationship>, PackageInterfaceExportSurfaceError> {
    relationships
        .into_iter()
        .map(|relationship| {
            let allows_mutation = relationship.allows_mutation();
            let position = relationship.position();

            let resolved = SymbolRelationship::new(
                relationship.kind(),
                resolve_symbol(symbols, relationship.owner())?,
                resolve_symbol(symbols, relationship.member())?,
                relationship.ordinal(),
            )
            .with_position(position);

            Ok(if allows_mutation {
                resolved.with_mutation()
            } else {
                resolved
            })
        })
        .collect()
}

fn resolve_exports(
    exports: impl IntoIterator<Item = ExportLookupInput>,
    symbols: &BTreeMap<bray_symbols::ExternalSymbolKey, InterfaceSymbolId>,
) -> Result<Vec<ExportedLookupEdge>, PackageInterfaceExportSurfaceError> {
    // Export names and dependency keys are Arc-backed values retained by the frozen surface.
    exports
        .into_iter()
        .map(|export| {
            let target = match export.target() {
                ExportSymbolReferenceInput::Local(key) => {
                    InterfaceSymbolReference::Local(resolve_symbol(symbols, key)?)
                }
                ExportSymbolReferenceInput::Dependency { dependency, key } => {
                    InterfaceSymbolReference::Dependency {
                        dependency: *dependency,
                        key: key.clone(),
                    }
                }
            };

            Ok(ExportedLookupEdge::new(
                resolve_symbol(symbols, export.owner())?,
                export.name().clone(),
                export.kind(),
                target,
            ))
        })
        .collect()
}

fn resolve_symbol(
    symbols: &BTreeMap<bray_symbols::ExternalSymbolKey, InterfaceSymbolId>,
    key: &bray_symbols::ExternalSymbolKey,
) -> Result<InterfaceSymbolId, PackageInterfaceExportSurfaceError> {
    symbols
        .get(key)
        .copied()
        .ok_or_else(|| PackageInterfaceExportSurfaceError::MissingSymbol(key.clone()))
}

#[cfg(test)]
mod tests {
    use bray_symbols::{ExternalSymbolKey, ModulePathKey, PackageIdentity, SymbolName};

    use super::build_package_interface_surface;
    use crate::test_support::package_version;
    use crate::{
        ExportLookupInput, ExportRelationshipInput, ExportSymbolInput, ExportSymbolReferenceInput,
        ExportedLookupKind, InterfaceContentHash, InterfaceDependency, InterfaceLanguageRevision,
        InterfaceProductIdentity, InterfaceProductKind, InterfaceSemantics,
        PackageInterfaceExportBundle, PackageInterfaceIdentity, SymbolRelationshipKind,
        encode_package_interface,
    };

    #[test]
    fn canonical_surface_is_independent_of_selected_input_order() {
        let fixture = fixture();

        let forward = build_package_interface_surface(
            fixture.identity.clone(),
            fixture.dependencies.clone(),
            fixture.symbols.clone(),
            fixture.relationships.clone(),
            fixture.exports.clone(),
        )
        .unwrap_or_else(|error| panic!("forward surface must build: {error:?}"));

        let reverse = build_package_interface_surface(
            fixture.identity,
            fixture.dependencies.into_iter().rev(),
            fixture.symbols.into_iter().rev(),
            fixture.relationships.into_iter().rev(),
            fixture.exports.into_iter().rev(),
        )
        .unwrap_or_else(|error| panic!("reversed surface must build: {error:?}"));

        assert_eq!(forward, reverse);

        let first = encode(forward);
        let second = encode(reverse);

        assert_eq!(first, second);
    }

    struct Fixture {
        identity: PackageInterfaceIdentity,
        dependencies: Vec<InterfaceDependency>,
        symbols: Vec<ExportSymbolInput>,
        relationships: Vec<ExportRelationshipInput>,
        exports: Vec<ExportLookupInput>,
    }

    fn fixture() -> Fixture {
        let package_identity = package("example.package");
        let package_key = ExternalSymbolKey::package(package_identity.clone());
        let app_key = module_key(package_key.clone(), "app");
        let util_key = module_key(package_key.clone(), "util");

        let Some(identity) = PackageInterfaceIdentity::try_new(
            package_identity,
            package_version(),
            product("library"),
            InterfaceProductKind::Library,
            "public-v1",
        ) else {
            panic!("test interface identity must be valid");
        };

        Fixture {
            identity,
            dependencies: vec![
                InterfaceDependency::new(
                    package("z.dependency"),
                    product("library"),
                    InterfaceContentHash::from_bytes([2; 32]),
                ),
                InterfaceDependency::new(
                    package("a.dependency"),
                    product("library"),
                    InterfaceContentHash::from_bytes([1; 32]),
                ),
            ],
            symbols: vec![
                ExportSymbolInput::new(util_key.clone(), Some(package_key.clone())),
                ExportSymbolInput::new(package_key.clone(), None),
                ExportSymbolInput::new(app_key.clone(), Some(package_key.clone())),
            ],
            relationships: vec![
                ExportRelationshipInput::new(
                    SymbolRelationshipKind::PackageModule,
                    package_key.clone(),
                    util_key.clone(),
                    1,
                ),
                ExportRelationshipInput::new(
                    SymbolRelationshipKind::PackageModule,
                    package_key.clone(),
                    app_key.clone(),
                    0,
                ),
            ],
            exports: vec![
                ExportLookupInput::new(
                    package_key.clone(),
                    name("util"),
                    ExportedLookupKind::Direct,
                    ExportSymbolReferenceInput::Local(util_key),
                ),
                ExportLookupInput::new(
                    package_key,
                    name("app"),
                    ExportedLookupKind::Direct,
                    ExportSymbolReferenceInput::Local(app_key),
                ),
            ],
        }
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }

    fn product(value: &str) -> InterfaceProductIdentity {
        InterfaceProductIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }

    fn name(value: &str) -> SymbolName {
        SymbolName::try_new(value).unwrap_or_else(|| panic!("test symbol name must be valid"))
    }

    fn module_key(package: ExternalSymbolKey, segment: &str) -> ExternalSymbolKey {
        let path = ModulePathKey::try_new([segment])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        ExternalSymbolKey::module(package, path)
            .unwrap_or_else(|| panic!("test module key must be valid"))
    }

    fn encode(surface: crate::PackageInterfaceSurface) -> crate::InterfaceArtifact {
        let bundle = PackageInterfaceExportBundle::try_new(
            surface,
            InterfaceSemantics::new(),
            InterfaceLanguageRevision::new(0),
            crate::test_support::implementation_configuration(),
        )
        .unwrap_or_else(|error| panic!("test export bundle must be valid: {error:?}"));

        encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test export bundle must encode: {error:?}"))
    }
}
