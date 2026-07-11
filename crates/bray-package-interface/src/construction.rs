use std::collections::BTreeMap;

use bray_symbols::{
    ImportedInterfaceId, ImportedLookupEdge, ImportedSymbolRelationship, ImportedSymbolSkeleton,
    ImportedSymbolSkeletonBuildError, ImportedSymbolSkeletonInput, InterfaceSymbolId,
    PackageIdentity, SymbolId,
};

use crate::{
    DependencyInterfaceId, InterfaceContentHash, InterfaceProductIdentity,
    InterfaceSymbolReference, PackageInterfaceSurface,
};

/// One loaded validated package surface participating in imported symbol construction.
#[derive(Clone, Copy, Debug)]
pub struct LoadedInterfaceSurface<'surface> {
    interface: ImportedInterfaceId,
    content_hash: InterfaceContentHash,
    surface: &'surface PackageInterfaceSurface,
}

impl<'surface> LoadedInterfaceSurface<'surface> {
    /// Creates a construction view without retaining a codec reader or artifact bytes.
    pub const fn new(
        interface: ImportedInterfaceId,
        content_hash: InterfaceContentHash,
        surface: &'surface PackageInterfaceSurface,
    ) -> Self {
        Self {
            interface,
            content_hash,
            surface,
        }
    }

    /// Returns the loaded-interface handle used by imported fact keys.
    pub const fn interface(self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the exact semantic content hash validated for this interface.
    pub const fn content_hash(self) -> InterfaceContentHash {
        self.content_hash
    }

    /// Returns the decoded validated identity surface.
    pub const fn surface(self) -> &'surface PackageInterfaceSurface {
        self.surface
    }
}

/// Describes why loaded package-interface surfaces cannot construct imported symbols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportedSymbolConstructionError {
    /// More than one loaded interface uses the same package identity.
    DuplicatePackage(PackageIdentity),
    /// A dependency package required by an interface is not loaded.
    MissingDependency(PackageIdentity),
    /// A loaded dependency product does not match the exact selected product.
    DependencyProductMismatch {
        /// Required dependency package.
        package: PackageIdentity,
        /// Product required by the importing interface.
        expected: InterfaceProductIdentity,
        /// Product supplied by the loaded interface.
        actual: InterfaceProductIdentity,
    },
    /// A loaded dependency's semantic content differs from the exact required content.
    DependencyContentMismatch {
        /// Required dependency package.
        package: PackageIdentity,
        /// Content hash required by the importing interface.
        expected: InterfaceContentHash,
        /// Content hash supplied by the loaded interface.
        actual: InterfaceContentHash,
    },
    /// A checked interface-local reference does not address an identity record.
    SymbolOutOfBounds {
        /// Interface containing the invalid reference.
        interface: ImportedInterfaceId,
        /// Missing interface-local symbol.
        symbol: InterfaceSymbolId,
    },
    /// A dependency reference names a missing dependency-table slot.
    DependencyOutOfBounds {
        /// Interface containing the invalid dependency reference.
        interface: ImportedInterfaceId,
        /// Missing dependency-table slot.
        dependency: DependencyInterfaceId,
    },
    /// An exported dependency key is absent from the exact loaded dependency surface.
    MissingDependencySymbol {
        /// Defining dependency package.
        package: PackageIdentity,
        /// Missing stable external identity.
        key: bray_symbols::ExternalSymbolKey,
    },
    /// Origin-neutral semantic construction rejected the translated surfaces.
    Symbols(ImportedSymbolSkeletonBuildError),
}

/// Constructs one deterministic imported symbol provider from validated package surfaces.
pub fn construct_imported_symbol_skeletons<'surface>(
    first_symbol_id: SymbolId,
    surfaces: impl IntoIterator<Item = LoadedInterfaceSurface<'surface>>,
) -> Result<ImportedSymbolSkeleton, ImportedSymbolConstructionError> {
    let surfaces: Vec<_> = surfaces.into_iter().collect();
    let package_index = package_index(&surfaces)?;

    validate_dependencies(&surfaces, &package_index)?;

    let inputs = surfaces
        .iter()
        .map(|loaded| construction_input(*loaded, &package_index))
        .collect::<Result<Vec<_>, _>>()?;

    ImportedSymbolSkeleton::try_new(first_symbol_id, inputs)
        .map_err(ImportedSymbolConstructionError::Symbols)
}

fn package_index<'surface>(
    surfaces: &[LoadedInterfaceSurface<'surface>],
) -> Result<
    BTreeMap<PackageIdentity, LoadedInterfaceSurface<'surface>>,
    ImportedSymbolConstructionError,
> {
    let mut packages = BTreeMap::new();

    for loaded in surfaces {
        let package = loaded.surface().identity().package().clone();

        if packages.insert(package.clone(), *loaded).is_some() {
            return Err(ImportedSymbolConstructionError::DuplicatePackage(package));
        }
    }

    Ok(packages)
}

fn validate_dependencies(
    surfaces: &[LoadedInterfaceSurface<'_>],
    package_index: &BTreeMap<PackageIdentity, LoadedInterfaceSurface<'_>>,
) -> Result<(), ImportedSymbolConstructionError> {
    for loaded in surfaces {
        for dependency in loaded.surface().dependencies() {
            let Some(actual) = package_index.get(dependency.package()).copied() else {
                return Err(ImportedSymbolConstructionError::MissingDependency(
                    dependency.package().clone(),
                ));
            };

            if actual.surface().identity().product() != dependency.product() {
                return Err(ImportedSymbolConstructionError::DependencyProductMismatch {
                    package: dependency.package().clone(),
                    expected: dependency.product().clone(),
                    actual: actual.surface().identity().product().clone(),
                });
            }

            if actual.content_hash() != dependency.content_hash() {
                return Err(ImportedSymbolConstructionError::DependencyContentMismatch {
                    package: dependency.package().clone(),
                    expected: dependency.content_hash(),
                    actual: actual.content_hash(),
                });
            }
        }
    }

    Ok(())
}

fn construction_input(
    loaded: LoadedInterfaceSurface<'_>,
    package_index: &BTreeMap<PackageIdentity, LoadedInterfaceSurface<'_>>,
) -> Result<ImportedSymbolSkeletonInput, ImportedSymbolConstructionError> {
    let relationships = loaded.surface().relationships().iter().map(|relationship| {
        ImportedSymbolRelationship::new(
            relationship.kind(),
            relationship.owner(),
            relationship.member(),
            relationship.ordinal(),
        )
    });
    let lookups = loaded
        .surface()
        .exports()
        .iter()
        .map(|edge| {
            let target = lookup_target_key(loaded, package_index, edge.target())?;

            Ok(ImportedLookupEdge::new(
                edge.owner(),
                edge.name().clone(),
                target,
            ))
        })
        .collect::<Result<Vec<_>, ImportedSymbolConstructionError>>()?;

    Ok(ImportedSymbolSkeletonInput::new(
        loaded.interface(),
        loaded.surface().symbols().clone(),
        relationships,
        lookups,
    ))
}

fn lookup_target_key(
    loaded: LoadedInterfaceSurface<'_>,
    package_index: &BTreeMap<PackageIdentity, LoadedInterfaceSurface<'_>>,
    target: &InterfaceSymbolReference,
) -> Result<bray_symbols::ExternalSymbolKey, ImportedSymbolConstructionError> {
    match target {
        InterfaceSymbolReference::Local(symbol) => loaded
            .surface()
            .symbols()
            .symbol(*symbol)
            .map(|identity| identity.key().clone())
            .ok_or(ImportedSymbolConstructionError::SymbolOutOfBounds {
                interface: loaded.interface(),
                symbol: *symbol,
            }),
        InterfaceSymbolReference::Dependency { dependency, key } => {
            let Some(reference) = dependency
                .to_index()
                .and_then(|index| loaded.surface().dependencies().get(index))
            else {
                return Err(ImportedSymbolConstructionError::DependencyOutOfBounds {
                    interface: loaded.interface(),
                    dependency: *dependency,
                });
            };

            let Some(defining) = package_index.get(reference.package()) else {
                return Err(ImportedSymbolConstructionError::MissingDependency(
                    reference.package().clone(),
                ));
            };

            if defining.surface().symbol_by_external_key(key).is_none() {
                return Err(ImportedSymbolConstructionError::MissingDependencySymbol {
                    package: reference.package().clone(),
                    key: key.clone(),
                });
            }

            Ok(key.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        AnySymbolId, ExternalSymbolKey, ImportedInterfaceId, ImportedSymbolIdentityInput,
        InterfaceSymbolId, MemberLookupResult, ModulePathKey, PackageIdentity, SymbolId,
        SymbolKind, SymbolName,
    };

    use super::{
        ImportedSymbolConstructionError, LoadedInterfaceSurface,
        construct_imported_symbol_skeletons,
    };
    use crate::{
        DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceContentHash,
        InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind,
        InterfaceSymbolReference, PackageInterfaceIdentity, PackageInterfaceSurface,
        SymbolRelationship, SymbolRelationshipKind,
    };

    const DEPENDENCY_HASH: InterfaceContentHash = InterfaceContentHash::from_bytes([1; 32]);
    const CONSUMER_HASH: InterfaceContentHash = InterfaceContentHash::from_bytes([2; 32]);

    #[test]
    fn construction_resolves_dependency_reexports_and_is_input_order_independent() {
        let dependency = dependency_surface("dependency-product");
        let dependency_function = function_key("dependency.package", "run");
        let consumer = consumer_surface(
            "dependency-product",
            DEPENDENCY_HASH,
            dependency_function.clone(),
        );
        let forward = construct(
            &dependency,
            &consumer,
            [
                loaded(9, DEPENDENCY_HASH, &dependency),
                loaded(4, CONSUMER_HASH, &consumer),
            ],
        );
        let reverse = construct(
            &dependency,
            &consumer,
            [
                loaded(4, CONSUMER_HASH, &consumer),
                loaded(9, DEPENDENCY_HASH, &dependency),
            ],
        );

        assert_eq!(forward, reverse);

        let consumer_module_key = module_key("consumer.package");

        let Some(AnySymbolId::Module(consumer_module)) =
            forward.symbol_by_external_key(&consumer_module_key)
        else {
            panic!("consumer module key must resolve");
        };

        let Some(dependency_function) = forward.symbol_by_external_key(&dependency_function) else {
            panic!("dependency function key must resolve");
        };

        assert_eq!(
            forward.lookup(consumer_module.into(), "dep_run"),
            MemberLookupResult::Found(dependency_function)
        );
    }

    #[test]
    fn exact_dependency_identity_and_content_are_required() {
        let dependency = dependency_surface("actual-product");
        let dependency_function = function_key("dependency.package", "run");
        let consumer = consumer_surface("expected-product", DEPENDENCY_HASH, dependency_function);

        assert_eq!(
            construct_imported_symbol_skeletons(
                SymbolId::new(0),
                [
                    loaded(1, DEPENDENCY_HASH, &dependency),
                    loaded(2, CONSUMER_HASH, &consumer),
                ],
            ),
            Err(ImportedSymbolConstructionError::DependencyProductMismatch {
                package: package("dependency.package"),
                expected: product("expected-product"),
                actual: product("actual-product"),
            })
        );

        let matching_dependency = dependency_surface("expected-product");

        assert_eq!(
            construct_imported_symbol_skeletons(
                SymbolId::new(0),
                [
                    loaded(
                        1,
                        InterfaceContentHash::from_bytes([8; 32]),
                        &matching_dependency
                    ),
                    loaded(2, CONSUMER_HASH, &consumer),
                ],
            ),
            Err(ImportedSymbolConstructionError::DependencyContentMismatch {
                package: package("dependency.package"),
                expected: DEPENDENCY_HASH,
                actual: InterfaceContentHash::from_bytes([8; 32]),
            })
        );
    }

    #[test]
    fn unresolved_dependency_external_keys_are_rejected() {
        let dependency = dependency_surface("dependency-product");
        let missing = function_key("dependency.package", "missing");
        let consumer = consumer_surface("dependency-product", DEPENDENCY_HASH, missing.clone());

        assert_eq!(
            construct_imported_symbol_skeletons(
                SymbolId::new(0),
                [
                    loaded(1, DEPENDENCY_HASH, &dependency),
                    loaded(2, CONSUMER_HASH, &consumer),
                ],
            ),
            Err(ImportedSymbolConstructionError::MissingDependencySymbol {
                package: package("dependency.package"),
                key: missing,
            })
        );
    }

    #[test]
    fn construction_views_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<LoadedInterfaceSurface<'static>>();
    }

    fn dependency_surface(product_name: &str) -> PackageInterfaceSurface {
        surface(
            "dependency.package",
            product_name,
            [],
            Some((
                "run",
                InterfaceSymbolReference::Local(InterfaceSymbolId::new(2)),
            )),
        )
    }

    fn consumer_surface(
        dependency_product: &str,
        dependency_hash: InterfaceContentHash,
        target: ExternalSymbolKey,
    ) -> PackageInterfaceSurface {
        surface(
            "consumer.package",
            "consumer-product",
            [InterfaceDependency::new(
                package("dependency.package"),
                product(dependency_product),
                dependency_hash,
            )],
            Some((
                "dep_run",
                InterfaceSymbolReference::Dependency {
                    dependency: DependencyInterfaceId::new(0),
                    key: target,
                },
            )),
        )
    }

    fn surface(
        package_name: &str,
        product_name: &str,
        dependencies: impl IntoIterator<Item = InterfaceDependency>,
        export: Option<(&str, InterfaceSymbolReference)>,
    ) -> PackageInterfaceSurface {
        let package = package(package_name);
        let package_key = ExternalSymbolKey::package(package.clone());
        let module_key = module_key(package_name);
        let mut symbols = vec![
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(0),
                package_key,
                SymbolKind::Package,
                None,
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(1),
                module_key.clone(),
                SymbolKind::Module,
                Some(InterfaceSymbolId::new(0)),
            ),
        ];
        let mut relationships = vec![SymbolRelationship::new(
            SymbolRelationshipKind::PackageModule,
            InterfaceSymbolId::new(0),
            InterfaceSymbolId::new(1),
            0,
        )];

        if package_name == "dependency.package" {
            symbols.push(ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(2),
                function_key(package_name, "run"),
                SymbolKind::Function,
                Some(InterfaceSymbolId::new(1)),
            ));
            relationships.push(SymbolRelationship::new(
                SymbolRelationshipKind::ModuleMember,
                InterfaceSymbolId::new(1),
                InterfaceSymbolId::new(2),
                0,
            ));
        }

        let exports = export.into_iter().map(|(name, target)| {
            ExportedLookupEdge::new(
                InterfaceSymbolId::new(1),
                symbol_name(name),
                if matches!(target, InterfaceSymbolReference::Local(_)) {
                    ExportedLookupKind::Direct
                } else {
                    ExportedLookupKind::ReExport
                },
                target,
            )
        });

        let Some(identity) = PackageInterfaceIdentity::try_new(
            package,
            product(product_name),
            InterfaceProductKind::Library,
            "public-v1",
        ) else {
            panic!("test interface identity must be valid");
        };

        match PackageInterfaceSurface::try_new(
            identity,
            dependencies,
            symbols,
            relationships,
            exports,
        ) {
            Ok(surface) => surface,
            Err(error) => panic!("test package interface surface must be valid: {error:?}"),
        }
    }

    fn construct<'surface, const N: usize>(
        _dependency: &'surface PackageInterfaceSurface,
        _consumer: &'surface PackageInterfaceSurface,
        loaded: [LoadedInterfaceSurface<'surface>; N],
    ) -> bray_symbols::ImportedSymbolSkeleton {
        match construct_imported_symbol_skeletons(SymbolId::new(20), loaded) {
            Ok(skeleton) => skeleton,
            Err(error) => panic!("test imported symbol construction must succeed: {error:?}"),
        }
    }

    const fn loaded(
        interface: u32,
        hash: InterfaceContentHash,
        surface: &PackageInterfaceSurface,
    ) -> LoadedInterfaceSurface<'_> {
        LoadedInterfaceSurface::new(ImportedInterfaceId::new(interface), hash, surface)
    }

    fn package(value: &str) -> PackageIdentity {
        match PackageIdentity::try_new(value) {
            Some(package) => package,
            None => panic!("test package identity must be valid"),
        }
    }

    fn product(value: &str) -> InterfaceProductIdentity {
        match InterfaceProductIdentity::try_new(value) {
            Some(product) => product,
            None => panic!("test product identity must be valid"),
        }
    }

    fn module_key(package_name: &str) -> ExternalSymbolKey {
        let package = ExternalSymbolKey::package(package(package_name));

        let Some(path) = ModulePathKey::try_new(["api"]) else {
            panic!("test module path must be valid");
        };

        match ExternalSymbolKey::module(package, path) {
            Some(module) => module,
            None => panic!("package key must own test module key"),
        }
    }

    fn function_key(package_name: &str, name: &str) -> ExternalSymbolKey {
        match ExternalSymbolKey::named(
            module_key(package_name),
            SymbolKind::Function,
            symbol_name(name),
        ) {
            Some(function) => function,
            None => panic!("module key must own test function key"),
        }
    }

    fn symbol_name(value: &str) -> SymbolName {
        match SymbolName::try_new(value) {
            Some(name) => name,
            None => panic!("test symbol name must be valid"),
        }
    }
}
