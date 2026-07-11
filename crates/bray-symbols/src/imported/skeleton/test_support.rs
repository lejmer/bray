use crate::{
    ExternalSymbolKey, ImportedInterfaceId, ImportedLookupEdge, ImportedPackageIdentitySurface,
    ImportedSymbolIdentityInput, ImportedSymbolRelationship, ImportedSymbolSkeleton,
    ImportedSymbolSkeletonInput, InterfaceSymbolId, ModulePathKey, PackageIdentity, SymbolId,
    SymbolKind, SymbolName, SymbolRelationshipKind,
};

pub(crate) struct InterfaceFixture {
    pub(crate) input: ImportedSymbolSkeletonInput,
    pub(crate) package_key: ExternalSymbolKey,
    pub(crate) module_key: ExternalSymbolKey,
    pub(crate) function_key: ExternalSymbolKey,
}

pub(crate) fn interface_fixture(
    interface: u32,
    package_name: &str,
    function_name: &str,
) -> InterfaceFixture {
    interface_fixture_at_module(interface, package_name, ["api"], function_name)
}

pub(crate) fn interface_fixture_at_module<I, S>(
    interface: u32,
    package_name: &str,
    module_path: I,
    function_name: &str,
) -> InterfaceFixture
where
    I: IntoIterator<Item = S>,
    S: Into<std::sync::Arc<str>>,
{
    interface_fixture_with_lookup(
        interface,
        package_name,
        module_path,
        function_name,
        function_name,
    )
}

pub(crate) fn interface_fixture_with_lookup<I, S>(
    interface: u32,
    package_name: &str,
    module_path: I,
    function_name: &str,
    lookup_name: &str,
) -> InterfaceFixture
where
    I: IntoIterator<Item = S>,
    S: Into<std::sync::Arc<str>>,
{
    let package = package_identity(package_name);
    let package_key = ExternalSymbolKey::package(package.clone());

    let Some(module_path) = ModulePathKey::try_new(module_path) else {
        panic!("test module path must be valid");
    };

    let Some(module_key) = ExternalSymbolKey::module(package_key.clone(), module_path) else {
        panic!("package key must own test module key");
    };

    let Some(function_key) = ExternalSymbolKey::named(
        module_key.clone(),
        SymbolKind::Function,
        symbol_name(function_name),
    ) else {
        panic!("module key must own test function key");
    };

    let symbols = match ImportedPackageIdentitySurface::try_new(
        package,
        [
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(0),
                package_key.clone(),
                SymbolKind::Package,
                None,
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(1),
                module_key.clone(),
                SymbolKind::Module,
                Some(InterfaceSymbolId::new(0)),
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(2),
                function_key.clone(),
                SymbolKind::Function,
                Some(InterfaceSymbolId::new(1)),
            ),
        ],
    ) {
        Ok(symbols) => symbols,
        Err(error) => panic!("test identity surface must be valid: {error:?}"),
    };
    let relationships = [
        ImportedSymbolRelationship::new(
            SymbolRelationshipKind::PackageModule,
            InterfaceSymbolId::new(0),
            InterfaceSymbolId::new(1),
            0,
        ),
        ImportedSymbolRelationship::new(
            SymbolRelationshipKind::ModuleMember,
            InterfaceSymbolId::new(1),
            InterfaceSymbolId::new(2),
            0,
        ),
    ];
    let lookups = [ImportedLookupEdge::new(
        InterfaceSymbolId::new(1),
        symbol_name(lookup_name),
        function_key.clone(),
    )];
    let input = ImportedSymbolSkeletonInput::new(
        ImportedInterfaceId::new(interface),
        symbols,
        relationships,
        lookups,
    );

    InterfaceFixture {
        input,
        package_key,
        module_key,
        function_key,
    }
}

pub(crate) fn package_identity(value: &str) -> PackageIdentity {
    match PackageIdentity::try_new(value) {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}

pub(crate) fn symbol_name(value: &str) -> SymbolName {
    match SymbolName::try_new(value) {
        Some(name) => name,
        None => panic!("test symbol name must be valid"),
    }
}

pub(crate) fn build_skeleton(
    inputs: impl IntoIterator<Item = ImportedSymbolSkeletonInput>,
) -> ImportedSymbolSkeleton {
    match ImportedSymbolSkeleton::try_new(SymbolId::new(10), inputs) {
        Ok(skeleton) => skeleton,
        Err(error) => panic!("test imported skeleton must build: {error:?}"),
    }
}
