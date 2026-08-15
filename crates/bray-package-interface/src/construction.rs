use std::collections::BTreeMap;

use bray_symbols::{
    AnySymbolId, ExternalSymbolKey, ImportedInterfaceId, ImportedLookupEdge,
    ImportedSymbolRelationship, ImportedSymbolSkeleton, ImportedSymbolSkeletonBuildError,
    ImportedSymbolSkeletonInput, InterfaceSymbolId, PackageIdentity, SymbolId,
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
    /// Creates a view over one validated package surface.
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

    /// Returns the loaded-interface handle used by imported record keys.
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
    DuplicatePackage {
        /// Repeated package identity.
        package: PackageIdentity,
        /// Previously loaded interface claiming the identity.
        first: ImportedInterfaceId,
        /// Later loaded interface claiming the identity.
        duplicate: ImportedInterfaceId,
    },
    /// A dependency package required by an interface is not loaded.
    MissingDependency {
        /// Interface requiring the unavailable dependency.
        importing: ImportedInterfaceId,
        /// Required dependency package.
        package: PackageIdentity,
    },
    /// A loaded dependency product does not match the exact selected product.
    DependencyProductMismatch {
        /// Required dependency package.
        package: PackageIdentity,
        /// Interface declaring the dependency requirement.
        importing: ImportedInterfaceId,
        /// Product required by the importing interface.
        expected: InterfaceProductIdentity,
        /// Product supplied by the loaded interface.
        actual: InterfaceProductIdentity,
    },
    /// A loaded dependency's semantic content differs from the exact required content.
    DependencyContentMismatch {
        /// Required dependency package.
        package: PackageIdentity,
        /// Interface declaring the dependency requirement.
        importing: ImportedInterfaceId,
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
        /// Interface exporting the unavailable dependency symbol.
        importing: ImportedInterfaceId,
        /// Defining dependency package.
        package: PackageIdentity,
        /// Missing stable external identity.
        key: ExternalSymbolKey,
    },
    /// An exported lookup attempted to project a compiler-known symbol.
    CompilerKnownExportTarget {
        /// Interface attempting the invalid export.
        importing: ImportedInterfaceId,
        /// Compiler-provided declaration targeted by the export.
        key: bray_symbols::SymbolKey,
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

/// Resolves validated interface references through one immutable imported symbol skeleton.
pub struct ImportedInterfaceSymbolResolver<'surface> {
    current: LoadedInterfaceSurface<'surface>,
    package_index: BTreeMap<PackageIdentity, LoadedInterfaceSurface<'surface>>,
    symbols: &'surface ImportedSymbolSkeleton,
    compiler_known: &'surface BTreeMap<bray_symbols::SymbolKey, AnySymbolId>,
}

impl<'surface> ImportedInterfaceSymbolResolver<'surface> {
    /// Creates a resolver for the loaded surfaces associated with `symbols`.
    pub fn try_new(
        current: LoadedInterfaceSurface<'surface>,
        surfaces: impl IntoIterator<Item = LoadedInterfaceSurface<'surface>>,
        symbols: &'surface ImportedSymbolSkeleton,
        compiler_known: &'surface BTreeMap<bray_symbols::SymbolKey, AnySymbolId>,
    ) -> Result<Self, ImportedSymbolConstructionError> {
        let surfaces: Vec<_> = surfaces.into_iter().collect();
        let package_index = package_index(&surfaces)?;

        validate_dependencies(&surfaces, &package_index)?;

        Ok(Self {
            current,
            package_index,
            symbols,
            compiler_known,
        })
    }

    fn resolve_external_key(
        &self,
        reference: &InterfaceSymbolReference,
    ) -> Result<ExternalSymbolKey, ImportedSymbolConstructionError> {
        lookup_target_key(self.current, &self.package_index, reference)
    }

    fn resolve_compiler_known(
        &self,
        reference: &crate::CompilerKnownSymbolReference,
    ) -> Option<AnySymbolId> {
        self.compiler_known
            .get(reference.key())
            .copied()
            .filter(|symbol| symbol.kind() == reference.kind())
    }
}

impl crate::InterfaceSymbolResolver for ImportedInterfaceSymbolResolver<'_> {
    fn resolve(&self, reference: &InterfaceSymbolReference) -> Option<AnySymbolId> {
        if let InterfaceSymbolReference::CompilerKnown(reference) = reference {
            return self.resolve_compiler_known(reference);
        }

        let key = self.resolve_external_key(reference).ok()?;

        self.symbols.symbol_by_external_key(&key)
    }

    fn symbol_key(&self, reference: &InterfaceSymbolReference) -> Option<bray_symbols::SymbolKey> {
        if let InterfaceSymbolReference::CompilerKnown(reference) = reference {
            // Compiler-known keys are Arc-backed and imported templates retain the identity.
            return self
                .resolve_compiler_known(reference)
                .map(|_| reference.key().clone());
        }

        self.resolve_external_key(reference)
            .ok()
            .map(bray_symbols::SymbolKey::external)
    }
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

        if let Some(first) = packages.insert(package.clone(), *loaded) {
            return Err(ImportedSymbolConstructionError::DuplicatePackage {
                package,
                first: first.interface(),
                duplicate: loaded.interface(),
            });
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
                return Err(ImportedSymbolConstructionError::MissingDependency {
                    importing: loaded.interface(),
                    package: dependency.package().clone(),
                });
            };

            if actual.surface().identity().product() != dependency.product() {
                return Err(ImportedSymbolConstructionError::DependencyProductMismatch {
                    importing: loaded.interface(),
                    package: dependency.package().clone(),
                    expected: dependency.product().clone(),
                    actual: actual.surface().identity().product().clone(),
                });
            }

            if actual.content_hash() != dependency.content_hash() {
                return Err(ImportedSymbolConstructionError::DependencyContentMismatch {
                    importing: loaded.interface(),
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
        let imported = ImportedSymbolRelationship::new(
            relationship.kind(),
            relationship.owner(),
            relationship.member(),
            relationship.ordinal(),
        )
        .with_position(relationship.position());

        if relationship.allows_mutation() {
            imported.with_mutation()
        } else {
            imported
        }
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
) -> Result<ExternalSymbolKey, ImportedSymbolConstructionError> {
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
                return Err(ImportedSymbolConstructionError::MissingDependency {
                    importing: loaded.interface(),
                    package: reference.package().clone(),
                });
            };

            if defining.surface().symbol_by_external_key(key).is_none() {
                return Err(ImportedSymbolConstructionError::MissingDependencySymbol {
                    importing: loaded.interface(),
                    package: reference.package().clone(),
                    key: key.clone(),
                });
            }

            Ok(key.clone())
        }
        InterfaceSymbolReference::CompilerKnown(reference) => {
            Err(ImportedSymbolConstructionError::CompilerKnownExportTarget {
                importing: loaded.interface(),
                key: reference.key().clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{
        AnySymbolId, ExternalSymbolKey, ImportedInterfaceId, ImportedSymbolIdentityInput,
        InterfaceSymbolId, MemberLookupResult, ModulePathKey, PackageIdentity, SymbolId,
        SymbolKind, SymbolName,
    };

    use super::{
        ImportedInterfaceSymbolResolver, ImportedSymbolConstructionError, LoadedInterfaceSurface,
        construct_imported_symbol_skeletons,
    };
    use crate::test_support::package_version;
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
                importing: ImportedInterfaceId::new(2),
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
                importing: ImportedInterfaceId::new(2),
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
                importing: ImportedInterfaceId::new(2),
                package: package("dependency.package"),
                key: missing,
            })
        );
    }

    #[test]
    fn construction_views_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImportedInterfaceSymbolResolver<'static>>();
        assert_send_sync::<LoadedInterfaceSurface<'static>>();
    }

    #[test]
    fn compiler_known_resolution_requires_the_declared_symbol_kind() {
        let surface = dependency_surface("dependency-product");
        let loaded = loaded(1, DEPENDENCY_HASH, &surface);
        let symbols = construct(&surface, &surface, [loaded]);

        let Some(symbol) =
            symbols.symbol_by_external_key(&function_key("dependency.package", "run"))
        else {
            panic!("test function must resolve");
        };

        let Some(key) = CompilerKnownDeclarationKey::try_new("TestFunction") else {
            panic!("test compiler-known key must be valid");
        };

        let Some(function_key) =
            bray_symbols::SymbolKey::compiler_known_declaration(key.clone(), SymbolKind::Function)
        else {
            panic!("test compiler-known function key must be valid");
        };

        let compiler_known = BTreeMap::from([(function_key.clone(), symbol)]);

        let resolver =
            ImportedInterfaceSymbolResolver::try_new(loaded, [loaded], &symbols, &compiler_known)
                .unwrap_or_else(|error| panic!("test resolver must be valid: {error:?}"));

        let compiler_known_reference = InterfaceSymbolReference::CompilerKnown(
            crate::CompilerKnownSymbolReference::try_new(function_key.clone())
                .unwrap_or_else(|| panic!("test compiler-known reference must be valid")),
        );

        assert_eq!(
            crate::InterfaceSymbolResolver::resolve(&resolver, &compiler_known_reference),
            Some(symbol)
        );

        assert_eq!(
            crate::InterfaceSymbolResolver::symbol_key(&resolver, &compiler_known_reference),
            Some(function_key)
        );

        assert_eq!(
            crate::InterfaceSymbolResolver::resolve(
                &resolver,
                &InterfaceSymbolReference::CompilerKnown(
                    crate::CompilerKnownSymbolReference::try_new(
                        bray_symbols::SymbolKey::compiler_known_declaration(
                            key,
                            SymbolKind::Struct,
                        )
                        .unwrap_or_else(|| panic!("test compiler-known struct key must be valid")),
                    )
                    .unwrap_or_else(|| panic!("test compiler-known reference must be valid")),
                ),
            ),
            None
        );
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
            package_version(),
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
