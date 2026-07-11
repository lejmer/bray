use crate::{
    ExternalSymbolKey, ImportedInterfaceId, ImportedLookupEdge, ImportedPackageIdentitySurface,
    ImportedSymbolIdentityInput, ImportedSymbolRelationship, ImportedSymbolSkeletonInput,
    InterfaceSymbolId, ModulePathKey, PackageIdentity, SymbolKind, SymbolName,
    SymbolRelationshipKind,
};

pub(super) struct InterfaceFixture {
    pub(super) input: ImportedSymbolSkeletonInput,
    pub(super) package_key: ExternalSymbolKey,
    pub(super) module_key: ExternalSymbolKey,
    pub(super) function_key: ExternalSymbolKey,
}

pub(super) fn interface_fixture(
    interface: u32,
    package_name: &str,
    function_name: &str,
) -> InterfaceFixture {
    let package = package_identity(package_name);
    let package_key = ExternalSymbolKey::package(package.clone());
    let Some(module_path) = ModulePathKey::try_new(["api"]) else {
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
        symbol_name(function_name),
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

pub(super) fn package_identity(value: &str) -> PackageIdentity {
    match PackageIdentity::try_new(value) {
        Some(identity) => identity,
        None => panic!("test package identity must be valid"),
    }
}

pub(super) fn symbol_name(value: &str) -> SymbolName {
    match SymbolName::try_new(value) {
        Some(name) => name,
        None => panic!("test symbol name must be valid"),
    }
}
