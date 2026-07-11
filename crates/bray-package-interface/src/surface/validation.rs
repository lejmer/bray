use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    ImportedPackageIdentitySurface, ImportedSymbolIdentityInput, InterfaceSymbolId, SymbolKind,
    SymbolName,
};

use super::{
    DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceDependency,
    InterfaceProductKind, InterfaceSymbolReference, PackageInterfaceIdentity,
    PackageInterfaceSurface, PackageInterfaceSurfaceBuildError, SymbolRelationship,
    SymbolRelationshipKind,
};

type CanonicalExports = (
    Vec<ExportedLookupEdge>,
    BTreeMap<(InterfaceSymbolId, SymbolName), usize>,
);

impl PackageInterfaceSurface {
    /// Validates and canonicalizes all eagerly loaded identity-surface records.
    pub fn try_new(
        identity: PackageInterfaceIdentity,
        dependencies: impl IntoIterator<Item = InterfaceDependency>,
        symbols: impl IntoIterator<Item = ImportedSymbolIdentityInput>,
        relationships: impl IntoIterator<Item = SymbolRelationship>,
        exports: impl IntoIterator<Item = ExportedLookupEdge>,
    ) -> Result<Self, PackageInterfaceSurfaceBuildError> {
        if identity.kind() != InterfaceProductKind::Library {
            return Err(PackageInterfaceSurfaceBuildError::NonLibraryProduct);
        }

        let (dependencies, dependency_remap) = canonical_dependencies(dependencies)?;
        let symbols: Vec<_> = symbols.into_iter().collect();

        for pair in symbols.windows(2) {
            if pair[0].key() >= pair[1].key() {
                return Err(PackageInterfaceSurfaceBuildError::NonCanonicalSymbolOrder {
                    previous: pair[0].id(),
                    current: pair[1].id(),
                });
            }
        }

        // Package identities, external keys, and names are Arc-backed immutable values.
        let symbols = ImportedPackageIdentitySurface::try_new(identity.package().clone(), symbols)
            .map_err(PackageInterfaceSurfaceBuildError::Identity)?;
        let symbol_index = symbols
            .symbols()
            .iter()
            .map(|symbol| (symbol.key().clone(), symbol.id()))
            .collect();

        let relationships = canonical_relationships(&symbols, relationships)?;
        let (exports, export_index) =
            canonical_exports(&symbols, &dependencies, &dependency_remap, exports)?;

        Ok(Self {
            identity,
            dependencies: dependencies.into(),
            symbols,
            relationships: relationships.into(),
            exports: exports.into(),
            symbol_index,
            export_index,
        })
    }
}

fn canonical_dependencies(
    dependencies: impl IntoIterator<Item = InterfaceDependency>,
) -> Result<(Vec<InterfaceDependency>, Vec<DependencyInterfaceId>), PackageInterfaceSurfaceBuildError>
{
    let mut dependencies: Vec<_> = dependencies.into_iter().enumerate().collect();

    if dependencies.len() > u32::MAX as usize {
        return Err(PackageInterfaceSurfaceBuildError::DependencyCountOverflow);
    }

    dependencies.sort_by(|(_, left), (_, right)| left.cmp(right));

    for pair in dependencies.windows(2) {
        if pair[0].1.package() == pair[1].1.package() {
            return Err(
                PackageInterfaceSurfaceBuildError::DuplicateDependencyPackage(
                    pair[1].1.package().clone(),
                ),
            );
        }
    }

    let mut remap = vec![DependencyInterfaceId::new(0); dependencies.len()];

    for (canonical_index, (original_index, _)) in dependencies.iter().enumerate() {
        let Some(canonical) = DependencyInterfaceId::try_from_index(canonical_index) else {
            return Err(PackageInterfaceSurfaceBuildError::DependencyCountOverflow);
        };

        remap[*original_index] = canonical;
    }

    Ok((
        dependencies
            .into_iter()
            .map(|(_, dependency)| dependency)
            .collect(),
        remap,
    ))
}

fn canonical_relationships(
    symbols: &ImportedPackageIdentitySurface,
    relationships: impl IntoIterator<Item = SymbolRelationship>,
) -> Result<Vec<SymbolRelationship>, PackageInterfaceSurfaceBuildError> {
    let mut relationships: Vec<_> = relationships.into_iter().collect();

    relationships.sort();

    let mut positions = BTreeSet::new();

    for relationship in &relationships {
        let Some(owner) = symbols.symbol(relationship.owner()) else {
            return Err(
                PackageInterfaceSurfaceBuildError::RelationshipSymbolOutOfBounds(*relationship),
            );
        };

        let Some(member) = symbols.symbol(relationship.member()) else {
            return Err(
                PackageInterfaceSurfaceBuildError::RelationshipSymbolOutOfBounds(*relationship),
            );
        };

        if !relationship.kind().supports(owner.kind(), member.kind()) {
            return Err(PackageInterfaceSurfaceBuildError::InvalidRelationship(
                *relationship,
            ));
        }

        if relationship.kind() != SymbolRelationshipKind::OverloadArm
            && member.container() != Some(relationship.owner())
        {
            return Err(PackageInterfaceSurfaceBuildError::InvalidRelationship(
                *relationship,
            ));
        }

        if !positions.insert((
            relationship.kind(),
            relationship.owner(),
            relationship.ordinal(),
        )) {
            return Err(
                PackageInterfaceSurfaceBuildError::DuplicateRelationshipPosition(*relationship),
            );
        }
    }

    Ok(relationships)
}

fn canonical_exports(
    symbols: &ImportedPackageIdentitySurface,
    dependencies: &[InterfaceDependency],
    dependency_remap: &[DependencyInterfaceId],
    exports: impl IntoIterator<Item = ExportedLookupEdge>,
) -> Result<CanonicalExports, PackageInterfaceSurfaceBuildError> {
    let mut exports: Vec<_> = exports
        .into_iter()
        .map(|edge| {
            edge.remap_dependency(dependency_remap)
                .map_err(PackageInterfaceSurfaceBuildError::DependencyOutOfBounds)
        })
        .collect::<Result<_, _>>()?;

    exports.sort();

    let mut index = BTreeMap::new();

    for (position, edge) in exports.iter().enumerate() {
        let Some(owner) = symbols.symbol(edge.owner()) else {
            return Err(PackageInterfaceSurfaceBuildError::ExportOwnerOutOfBounds(
                edge.owner(),
            ));
        };

        if !matches!(owner.kind(), SymbolKind::Package | SymbolKind::Module) {
            return Err(PackageInterfaceSurfaceBuildError::InvalidExportOwner(
                edge.owner(),
            ));
        }

        validate_export_target(symbols, dependencies, edge)?;

        let key = (edge.owner(), edge.name().clone());

        if index.insert(key, position).is_some() {
            return Err(PackageInterfaceSurfaceBuildError::DuplicateExportName {
                owner: edge.owner(),
                name: edge.name().clone(),
            });
        }
    }

    Ok((exports, index))
}

fn validate_export_target(
    symbols: &ImportedPackageIdentitySurface,
    dependencies: &[InterfaceDependency],
    edge: &ExportedLookupEdge,
) -> Result<(), PackageInterfaceSurfaceBuildError> {
    match edge.target() {
        InterfaceSymbolReference::Local(target) => {
            let Some(target_symbol) = symbols.symbol(*target) else {
                return Err(PackageInterfaceSurfaceBuildError::ExportTargetOutOfBounds(
                    *target,
                ));
            };

            if edge.kind() == ExportedLookupKind::Direct
                && target_symbol.container() != Some(edge.owner())
            {
                return Err(PackageInterfaceSurfaceBuildError::InvalidDirectExportTarget(*target));
            }
        }
        InterfaceSymbolReference::Dependency { dependency, key } => {
            let Some(reference) = dependency
                .to_index()
                .and_then(|index| dependencies.get(index))
            else {
                return Err(PackageInterfaceSurfaceBuildError::DependencyOutOfBounds(
                    *dependency,
                ));
            };

            if key.package_identity() != reference.package() {
                return Err(
                    PackageInterfaceSurfaceBuildError::DependencyKeyPackageMismatch(*dependency),
                );
            }

            if edge.kind() == ExportedLookupKind::Direct {
                return Err(
                    PackageInterfaceSurfaceBuildError::InvalidDirectExportTarget(edge.owner()),
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_symbols::PackageIdentity;

    use crate::{
        InterfaceContentHash, InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind,
        PackageInterfaceIdentity, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    };

    #[test]
    fn only_library_products_can_publish_interfaces() {
        let identity = identity(InterfaceProductKind::Executable);

        assert_eq!(
            PackageInterfaceSurface::try_new(identity, [], [], [], []),
            Err(PackageInterfaceSurfaceBuildError::NonLibraryProduct)
        );
    }

    #[test]
    fn dependency_packages_are_unique_after_canonical_ordering() {
        let dependency = InterfaceDependency::new(
            package("dependency"),
            product("library"),
            InterfaceContentHash::from_bytes([1; 32]),
        );

        assert_eq!(
            PackageInterfaceSurface::try_new(
                identity(InterfaceProductKind::Library),
                [dependency.clone(), dependency],
                [],
                [],
                []
            ),
            Err(
                PackageInterfaceSurfaceBuildError::DuplicateDependencyPackage(package(
                    "dependency"
                ))
            )
        );
    }

    fn identity(kind: InterfaceProductKind) -> PackageInterfaceIdentity {
        match PackageInterfaceIdentity::try_new(package("example"), product("main"), kind, "v1") {
            Some(identity) => identity,
            None => panic!("test package-interface identity must be valid"),
        }
    }

    fn package(value: &str) -> PackageIdentity {
        match PackageIdentity::try_new(value) {
            Some(identity) => identity,
            None => panic!("test package identity must be valid"),
        }
    }

    fn product(value: &str) -> InterfaceProductIdentity {
        match InterfaceProductIdentity::try_new(value) {
            Some(identity) => identity,
            None => panic!("test product identity must be valid"),
        }
    }
}
