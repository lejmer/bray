use std::{collections::BTreeMap, marker::PhantomData, sync::Arc};

use bray_base::shared_slice;

use crate::{
    ExactSymbolId, ExternalSymbolKey, ExternalSymbolKeyData, ImportedInterfaceId,
    InterfaceSymbolId, PackageIdentity, SymbolKind, SymbolOrigin,
};

/// One imported symbol identity record awaiting whole-surface validation.
///
/// A record becomes trusted semantic input only as part of a validated
/// [`ImportedPackageIdentitySurface`].
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSymbolIdentityInput {
    id: InterfaceSymbolId,
    key: ExternalSymbolKey,
    kind: SymbolKind,
    container: Option<InterfaceSymbolId>,
}

impl ImportedSymbolIdentityInput {
    /// Creates an immutable decoded identity record.
    pub fn new(
        id: InterfaceSymbolId,
        key: ExternalSymbolKey,
        kind: SymbolKind,
        container: Option<InterfaceSymbolId>,
    ) -> Self {
        Self {
            id,
            key,
            kind,
            container,
        }
    }

    /// Returns the artifact-local symbol ID.
    pub const fn id(&self) -> InterfaceSymbolId {
        self.id
    }

    /// Returns the stable external identity.
    pub const fn key(&self) -> &ExternalSymbolKey {
        &self.key
    }

    /// Returns the ordinary Bray symbol kind.
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// Returns the artifact-local semantic container.
    pub const fn container(&self) -> Option<InterfaceSymbolId> {
        self.container
    }
}

/// One symbol identity from a validated imported package surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSymbolIdentity(ImportedSymbolIdentityInput);

impl ImportedSymbolIdentity {
    /// Returns the artifact-local symbol ID.
    pub const fn id(&self) -> InterfaceSymbolId {
        self.0.id()
    }

    /// Returns the stable external identity.
    pub const fn key(&self) -> &ExternalSymbolKey {
        self.0.key()
    }

    /// Returns the ordinary Bray symbol kind.
    pub const fn kind(&self) -> SymbolKind {
        self.0.kind()
    }

    /// Returns the artifact-local semantic container.
    pub const fn container(&self) -> Option<InterfaceSymbolId> {
        self.0.container()
    }

    /// Returns the origin shared by every symbol constructed from this record.
    pub const fn origin(&self) -> SymbolOrigin {
        SymbolOrigin::Imported
    }

    fn from_validated(input: ImportedSymbolIdentityInput) -> Self {
        Self(input)
    }
}

/// Describes why decoded imported identity records cannot form a valid semantic surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImportedIdentitySurfaceError {
    /// The surface contains no package root.
    Empty,
    /// The symbol table is too large for compact interface IDs.
    SymbolCountOverflow,
    /// A record ID does not match its canonical table position.
    NonCanonicalSymbolId {
        /// The table-position ID required by canonical ordering.
        expected: InterfaceSymbolId,
        /// The ID supplied by the decoded record.
        actual: InterfaceSymbolId,
    },
    /// The first identity record is not the package root.
    MissingPackageRoot {
        /// The kind found at the package-root position.
        actual: SymbolKind,
    },
    /// The package root incorrectly names a semantic container.
    PackageRootHasContainer {
        /// The invalid container ID.
        container: InterfaceSymbolId,
    },
    /// A key belongs to a package other than the surface package.
    PackageIdentityMismatch {
        /// The record containing the mismatched key.
        symbol: InterfaceSymbolId,
    },
    /// A decoded record kind disagrees with its structured external key.
    SymbolKindMismatch {
        /// The mismatched record.
        symbol: InterfaceSymbolId,
        /// The record's declared kind.
        declared: SymbolKind,
        /// The kind encoded structurally by the key.
        keyed: SymbolKind,
    },
    /// Two records use the same stable external key.
    DuplicateExternalKey {
        /// The first record using the key.
        first: InterfaceSymbolId,
        /// The later duplicate record.
        duplicate: InterfaceSymbolId,
    },
    /// A non-root record has no semantic container.
    MissingContainer {
        /// The uncontained record.
        symbol: InterfaceSymbolId,
    },
    /// A record refers to a missing, later, or cyclic container.
    InvalidContainer {
        /// The contained record.
        symbol: InterfaceSymbolId,
        /// The invalid container ID.
        container: InterfaceSymbolId,
    },
    /// The record's containment edge disagrees with its external key owner.
    ContainerKeyMismatch {
        /// The contained record.
        symbol: InterfaceSymbolId,
        /// The declared container ID.
        container: InterfaceSymbolId,
    },
    /// A second root or a compiler-known root appears in a package surface.
    UnexpectedRoot {
        /// The invalid root record.
        symbol: InterfaceSymbolId,
        /// The invalid root kind.
        kind: SymbolKind,
    },
}

/// An immutable, semantically validated imported package identity skeleton input.
///
/// Records use dense canonical interface IDs. Every non-root record follows its semantic
/// container, which makes containment acyclic and permits deterministic single-pass mapping to
/// compilation-local typed symbol IDs.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedPackageIdentitySurface {
    package: PackageIdentity,
    symbols: Arc<[ImportedSymbolIdentity]>,
}

impl ImportedPackageIdentitySurface {
    /// Validates and publishes decoded identity records as an immutable semantic surface.
    pub fn try_new(
        package: PackageIdentity,
        symbols: impl IntoIterator<Item = ImportedSymbolIdentityInput>,
    ) -> Result<Self, ImportedIdentitySurfaceError> {
        let inputs: Vec<ImportedSymbolIdentityInput> = symbols.into_iter().collect();

        validate_surface(&package, &inputs)?;

        let symbols: Vec<ImportedSymbolIdentity> = inputs
            .into_iter()
            .map(ImportedSymbolIdentity::from_validated)
            .collect();

        Ok(Self {
            package,
            symbols: shared_slice(symbols),
        })
    }

    /// Returns the package identity declared by this surface.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the canonical package-root interface ID.
    pub const fn package_symbol_id(&self) -> InterfaceSymbolId {
        InterfaceSymbolId::new(0)
    }

    /// Returns every validated identity record in canonical interface order.
    pub fn symbols(&self) -> &[ImportedSymbolIdentity] {
        &self.symbols
    }

    /// Returns a validated identity record by its checked interface-local ID.
    pub fn symbol(&self, id: InterfaceSymbolId) -> Option<&ImportedSymbolIdentity> {
        id.to_index().and_then(|index| self.symbols.get(index))
    }

    /// Creates a category-typed imported fact key when the symbol has the requested exact kind.
    pub fn symbol_fact_key<I: ExactSymbolId>(
        &self,
        interface: ImportedInterfaceId,
        symbol: InterfaceSymbolId,
    ) -> Option<ImportedSymbolFactKey<I>> {
        let identity = self.symbol(symbol)?;

        if identity.kind() != I::KIND {
            return None;
        }

        Some(ImportedSymbolFactKey::from_validated(interface, symbol))
    }
}

/// A category-typed route to one imported symbol's semantic facts.
///
/// `I` is the exact ordinary symbol ID type used in the consuming compilation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSymbolFactKey<I: ExactSymbolId> {
    interface: ImportedInterfaceId,
    symbol: InterfaceSymbolId,
    marker: PhantomData<fn() -> I>,
}

/// The compiled-interface address backing one imported symbol.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSymbolFactAddress {
    interface: ImportedInterfaceId,
    symbol: InterfaceSymbolId,
}

impl ImportedSymbolFactAddress {
    /// Returns the loaded interface containing the symbol.
    pub const fn interface(self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the symbol's interface-local identity.
    pub const fn symbol(self) -> InterfaceSymbolId {
        self.symbol
    }

    pub(crate) const fn from_validated(
        interface: ImportedInterfaceId,
        symbol: InterfaceSymbolId,
    ) -> Self {
        Self { interface, symbol }
    }
}

impl<I: ExactSymbolId> ImportedSymbolFactKey<I> {
    /// Returns the loaded interface containing the symbol.
    pub const fn interface(self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the symbol's interface-local identity.
    pub const fn symbol(self) -> InterfaceSymbolId {
        self.symbol
    }

    /// Returns the exact ordinary symbol kind expected from the decoded fact.
    pub const fn kind(self) -> SymbolKind {
        I::KIND
    }

    pub(crate) const fn from_validated(
        interface: ImportedInterfaceId,
        symbol: InterfaceSymbolId,
    ) -> Self {
        Self {
            interface,
            symbol,
            marker: PhantomData,
        }
    }
}

impl<I: ExactSymbolId> From<ImportedSymbolFactKey<I>> for ImportedSymbolFactAddress {
    fn from(key: ImportedSymbolFactKey<I>) -> Self {
        Self::from_validated(key.interface(), key.symbol())
    }
}

fn validate_surface(
    package: &PackageIdentity,
    symbols: &[ImportedSymbolIdentityInput],
) -> Result<(), ImportedIdentitySurfaceError> {
    let Some(root) = symbols.first() else {
        return Err(ImportedIdentitySurfaceError::Empty);
    };

    validate_package_root(package, root)?;

    let mut keys = BTreeMap::new();

    for (index, symbol) in symbols.iter().enumerate() {
        validate_symbol_position(index, symbol)?;
        validate_symbol_package(package, symbol)?;
        validate_symbol_kind(symbol)?;
        validate_unique_key(&mut keys, symbol)?;

        if index > 0 {
            validate_containment(index, symbol, symbols)?;
        }
    }

    Ok(())
}

fn validate_package_root(
    package: &PackageIdentity,
    root: &ImportedSymbolIdentityInput,
) -> Result<(), ImportedIdentitySurfaceError> {
    if root.kind() != SymbolKind::Package {
        return Err(ImportedIdentitySurfaceError::MissingPackageRoot {
            actual: root.kind(),
        });
    }

    if let Some(container) = root.container() {
        return Err(ImportedIdentitySurfaceError::PackageRootHasContainer { container });
    }

    if root.key().package_identity() != package
        || !matches!(root.key().data(), ExternalSymbolKeyData::Package(_))
    {
        return Err(ImportedIdentitySurfaceError::PackageIdentityMismatch { symbol: root.id() });
    }

    Ok(())
}

fn validate_symbol_position(
    index: usize,
    symbol: &ImportedSymbolIdentityInput,
) -> Result<(), ImportedIdentitySurfaceError> {
    let Some(expected) = InterfaceSymbolId::try_from_index(index) else {
        return Err(ImportedIdentitySurfaceError::SymbolCountOverflow);
    };

    if symbol.id() != expected {
        return Err(ImportedIdentitySurfaceError::NonCanonicalSymbolId {
            expected,
            actual: symbol.id(),
        });
    }

    Ok(())
}

fn validate_symbol_package(
    package: &PackageIdentity,
    symbol: &ImportedSymbolIdentityInput,
) -> Result<(), ImportedIdentitySurfaceError> {
    if symbol.key().package_identity() != package {
        return Err(ImportedIdentitySurfaceError::PackageIdentityMismatch {
            symbol: symbol.id(),
        });
    }

    Ok(())
}

fn validate_symbol_kind(
    symbol: &ImportedSymbolIdentityInput,
) -> Result<(), ImportedIdentitySurfaceError> {
    let keyed = symbol.key().kind();

    if symbol.kind() != keyed {
        return Err(ImportedIdentitySurfaceError::SymbolKindMismatch {
            symbol: symbol.id(),
            declared: symbol.kind(),
            keyed,
        });
    }

    if symbol.id().raw() > 0
        && matches!(
            symbol.kind(),
            SymbolKind::Package | SymbolKind::CompilerKnownEnvironment
        )
    {
        return Err(ImportedIdentitySurfaceError::UnexpectedRoot {
            symbol: symbol.id(),
            kind: symbol.kind(),
        });
    }

    Ok(())
}

fn validate_unique_key<'a>(
    keys: &mut BTreeMap<&'a ExternalSymbolKey, InterfaceSymbolId>,
    symbol: &'a ImportedSymbolIdentityInput,
) -> Result<(), ImportedIdentitySurfaceError> {
    if let Some(first) = keys.insert(symbol.key(), symbol.id()) {
        return Err(ImportedIdentitySurfaceError::DuplicateExternalKey {
            first,
            duplicate: symbol.id(),
        });
    }

    Ok(())
}

fn validate_containment(
    index: usize,
    symbol: &ImportedSymbolIdentityInput,
    symbols: &[ImportedSymbolIdentityInput],
) -> Result<(), ImportedIdentitySurfaceError> {
    let Some(container) = symbol.container() else {
        return Err(ImportedIdentitySurfaceError::MissingContainer {
            symbol: symbol.id(),
        });
    };

    let Some(container_index) = container.to_index() else {
        return Err(ImportedIdentitySurfaceError::InvalidContainer {
            symbol: symbol.id(),
            container,
        });
    };

    if container_index >= index {
        return Err(ImportedIdentitySurfaceError::InvalidContainer {
            symbol: symbol.id(),
            container,
        });
    }

    let Some(container_symbol) = symbols.get(container_index) else {
        return Err(ImportedIdentitySurfaceError::InvalidContainer {
            symbol: symbol.id(),
            container,
        });
    };

    if symbol.key().owner() != Some(container_symbol.key()) {
        return Err(ImportedIdentitySurfaceError::ContainerKeyMismatch {
            symbol: symbol.id(),
            container,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        ExternalSymbolKey, FunctionSymbolId, ImportedInterfaceId, InterfaceSymbolId, ModulePathKey,
        PackageIdentity, PredicateSymbolId, SymbolKind, SymbolName, SymbolOrigin,
    };

    use super::{
        ImportedIdentitySurfaceError, ImportedPackageIdentitySurface, ImportedSymbolIdentityInput,
    };

    fn package_identity(value: &str) -> PackageIdentity {
        match PackageIdentity::try_new(value) {
            Some(identity) => identity,
            None => panic!("test package identity must be valid"),
        }
    }

    fn symbol_name(value: &str) -> SymbolName {
        match SymbolName::try_new(value) {
            Some(name) => name,
            None => panic!("test symbol name must be valid"),
        }
    }

    fn valid_records() -> (PackageIdentity, Vec<ImportedSymbolIdentityInput>) {
        let package = package_identity("example.package");
        let package_key = ExternalSymbolKey::package(package_identity("example.package"));

        let Some(path) = ModulePathKey::try_new(["example"]) else {
            panic!("test module path must be valid");
        };

        let Some(module_key) = ExternalSymbolKey::module(package_key.clone(), path) else {
            panic!("package key must own a module");
        };

        let Some(function_key) =
            ExternalSymbolKey::named(module_key.clone(), SymbolKind::Function, symbol_name("run"))
        else {
            panic!("function must support named external identity");
        };

        let records = vec![
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(0),
                package_key,
                SymbolKind::Package,
                None,
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(1),
                module_key,
                SymbolKind::Module,
                Some(InterfaceSymbolId::new(0)),
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(2),
                function_key,
                SymbolKind::Function,
                Some(InterfaceSymbolId::new(1)),
            ),
        ];

        (package, records)
    }

    fn valid_surface() -> ImportedPackageIdentitySurface {
        let (package, records) = valid_records();

        match ImportedPackageIdentitySurface::try_new(package, records) {
            Ok(surface) => surface,
            Err(error) => panic!("valid test surface was rejected: {error:?}"),
        }
    }

    #[test]
    fn valid_surfaces_are_immutable_and_deterministic() {
        let first = valid_surface();
        let second = valid_surface();

        assert_eq!(first, second);
        assert_eq!(first.symbols().len(), 3);

        let Some(function) = first.symbol(InterfaceSymbolId::new(2)) else {
            panic!("function identity must exist");
        };

        assert_eq!(function.kind(), SymbolKind::Function);
        assert_eq!(function.origin(), SymbolOrigin::Imported);
    }

    #[test]
    fn fact_keys_are_validated_and_category_typed() {
        let surface = valid_surface();
        let interface = ImportedInterfaceId::new(4);

        let function =
            surface.symbol_fact_key::<FunctionSymbolId>(interface, InterfaceSymbolId::new(2));

        let Some(function) = function else {
            panic!("function identity must produce a function fact key");
        };

        assert_eq!(function.interface(), interface);
        assert_eq!(function.symbol(), InterfaceSymbolId::new(2));
        assert_eq!(function.kind(), SymbolKind::Function);

        assert_eq!(
            surface.symbol_fact_key::<PredicateSymbolId>(interface, InterfaceSymbolId::new(2)),
            None
        );
    }

    #[test]
    fn noncanonical_ids_are_rejected() {
        let (package, mut records) = valid_records();

        let Some(function) = records.pop() else {
            panic!("valid test records must contain a function");
        };

        records.push(ImportedSymbolIdentityInput::new(
            InterfaceSymbolId::new(8),
            function.key().clone(),
            SymbolKind::Function,
            Some(InterfaceSymbolId::new(1)),
        ));

        assert_eq!(
            ImportedPackageIdentitySurface::try_new(package, records),
            Err(ImportedIdentitySurfaceError::NonCanonicalSymbolId {
                expected: InterfaceSymbolId::new(2),
                actual: InterfaceSymbolId::new(8),
            })
        );
    }

    #[test]
    fn kind_and_key_mismatches_are_rejected() {
        let (package, mut records) = valid_records();

        let Some(function) = records.pop() else {
            panic!("valid test records must contain a function");
        };

        records.push(ImportedSymbolIdentityInput::new(
            InterfaceSymbolId::new(2),
            function.key().clone(),
            SymbolKind::Predicate,
            Some(InterfaceSymbolId::new(1)),
        ));

        assert_eq!(
            ImportedPackageIdentitySurface::try_new(package, records),
            Err(ImportedIdentitySurfaceError::SymbolKindMismatch {
                symbol: InterfaceSymbolId::new(2),
                declared: SymbolKind::Predicate,
                keyed: SymbolKind::Function,
            })
        );
    }

    #[test]
    fn duplicate_keys_are_rejected_deterministically() {
        let (package, mut records) = valid_records();

        let Some(function) = records.last() else {
            panic!("valid test records must contain a function");
        };

        let function_key = function.key().clone();

        records.push(ImportedSymbolIdentityInput::new(
            InterfaceSymbolId::new(3),
            function_key,
            SymbolKind::Function,
            Some(InterfaceSymbolId::new(1)),
        ));

        assert_eq!(
            ImportedPackageIdentitySurface::try_new(package, records),
            Err(ImportedIdentitySurfaceError::DuplicateExternalKey {
                first: InterfaceSymbolId::new(2),
                duplicate: InterfaceSymbolId::new(3),
            })
        );
    }

    #[test]
    fn invalid_containment_is_rejected_without_indexing_panics() {
        let (package, mut records) = valid_records();

        let Some(function) = records.pop() else {
            panic!("valid test records must contain a function");
        };

        records.push(ImportedSymbolIdentityInput::new(
            InterfaceSymbolId::new(2),
            function.key().clone(),
            SymbolKind::Function,
            Some(InterfaceSymbolId::new(u32::MAX)),
        ));

        assert_eq!(
            ImportedPackageIdentitySurface::try_new(package, records),
            Err(ImportedIdentitySurfaceError::InvalidContainer {
                symbol: InterfaceSymbolId::new(2),
                container: InterfaceSymbolId::new(u32::MAX),
            })
        );
    }

    #[test]
    fn package_mismatches_are_rejected() {
        let (_, records) = valid_records();

        assert_eq!(
            ImportedPackageIdentitySurface::try_new(package_identity("other.package"), records),
            Err(ImportedIdentitySurfaceError::PackageIdentityMismatch {
                symbol: InterfaceSymbolId::new(0),
            })
        );
    }

    #[test]
    fn imported_surfaces_and_fact_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImportedPackageIdentitySurface>();
        assert_send_sync::<super::ImportedSymbolFactAddress>();
        assert_send_sync::<super::ImportedSymbolFactKey<FunctionSymbolId>>();
    }
}
