//! Shared fixtures for crates testing semantic compiler boundaries.

use std::sync::{Arc, OnceLock};

use bray_declarations::DeclarationId;

use crate::{
    AnySymbolId, AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider, ExternalSymbolKey,
    GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    GenericTypeParameterSymbolId, ImplementationInstanceData, ImplementationInstanceId,
    ImplementationRequirementKey, ImportedInterfaceId, ImportedLookupEdge,
    ImportedPackageIdentitySurface, ImportedSymbolIdentityInput, ImportedSymbolRelationship,
    ImportedSymbolSkeleton, ImportedSymbolSkeletonInput, InterfaceSymbolId, ModulePathKey,
    NamedTraitImplementationSymbolId, PackageIdentity, SemanticValueStore, SymbolId, SymbolKey,
    SymbolKind, SymbolName, SymbolRelationshipKind, SymbolRootKey, TraitApplicationData,
    TraitSymbolId, TypeId,
};

/// Imported package, module, and declaration identities used by cross-crate lookup tests.
pub struct ImportedLookupFixture {
    /// Immutable imported identity and lookup provider.
    pub symbols: ImportedSymbolSkeleton,
    /// Imported declaration identity.
    pub declaration: AnySymbolId,
}

/// Builds one imported package with an exported module and declaration.
pub fn imported_lookup_fixture(
    package_name: &str,
    module_name: &str,
    declaration_kind: SymbolKind,
    declaration_name: &str,
) -> ImportedLookupFixture {
    let package_identity = PackageIdentity::try_new(package_name)
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let package_key = ExternalSymbolKey::package(package_identity.clone());

    let module_key = imported_module_key(&package_key, module_name);

    let declaration_name = SymbolName::try_new(declaration_name)
        .unwrap_or_else(|| panic!("test declaration name must be valid"));

    let declaration_key = ExternalSymbolKey::named(
        module_key.clone(),
        declaration_kind,
        declaration_name.clone(),
    )
    .unwrap_or_else(|| panic!("test declaration key must be valid"));

    let identities = ImportedPackageIdentitySurface::try_new(
        package_identity,
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
                declaration_key.clone(),
                declaration_kind,
                Some(InterfaceSymbolId::new(1)),
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("test imported identities must be valid: {error:?}"));

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

    let lookups = [
        ImportedLookupEdge::new(
            InterfaceSymbolId::new(0),
            SymbolName::try_new(module_name)
                .unwrap_or_else(|| panic!("test module lookup name must be valid")),
            module_key.clone(),
        ),
        ImportedLookupEdge::new(
            InterfaceSymbolId::new(1),
            declaration_name,
            declaration_key.clone(),
        ),
    ];

    let input = ImportedSymbolSkeletonInput::new(
        ImportedInterfaceId::new(0),
        identities,
        relationships,
        lookups,
    );

    let symbols = ImportedSymbolSkeleton::try_new(SymbolId::new(10_000), [input])
        .unwrap_or_else(|error| panic!("test imported skeleton must build: {error:?}"));

    let declaration = symbols
        .symbol_by_external_key(&declaration_key)
        .unwrap_or_else(|| panic!("test imported declaration must be present"));

    ImportedLookupFixture {
        symbols,
        declaration,
    }
}

/// Builds one imported declaration re-exported through another module.
pub fn imported_reexport_lookup_fixture(
    package_name: &str,
    defining_module_name: &str,
    exporting_module_name: &str,
    declaration_kind: SymbolKind,
    declaration_name: &str,
) -> ImportedLookupFixture {
    let package_identity = PackageIdentity::try_new(package_name)
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let package_key = ExternalSymbolKey::package(package_identity.clone());

    let defining_module_key = imported_module_key(&package_key, defining_module_name);
    let exporting_module_key = imported_module_key(&package_key, exporting_module_name);

    let declaration_name = SymbolName::try_new(declaration_name)
        .unwrap_or_else(|| panic!("test declaration name must be valid"));

    let declaration_key = ExternalSymbolKey::named(
        defining_module_key.clone(),
        declaration_kind,
        declaration_name.clone(),
    )
    .unwrap_or_else(|| panic!("test declaration key must be valid"));

    let identities = ImportedPackageIdentitySurface::try_new(
        package_identity,
        [
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(0),
                package_key.clone(),
                SymbolKind::Package,
                None,
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(1),
                defining_module_key,
                SymbolKind::Module,
                Some(InterfaceSymbolId::new(0)),
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(2),
                exporting_module_key.clone(),
                SymbolKind::Module,
                Some(InterfaceSymbolId::new(0)),
            ),
            ImportedSymbolIdentityInput::new(
                InterfaceSymbolId::new(3),
                declaration_key.clone(),
                declaration_kind,
                Some(InterfaceSymbolId::new(1)),
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("test imported identities must be valid: {error:?}"));

    let relationships = [
        ImportedSymbolRelationship::new(
            SymbolRelationshipKind::PackageModule,
            InterfaceSymbolId::new(0),
            InterfaceSymbolId::new(1),
            0,
        ),
        ImportedSymbolRelationship::new(
            SymbolRelationshipKind::PackageModule,
            InterfaceSymbolId::new(0),
            InterfaceSymbolId::new(2),
            1,
        ),
        ImportedSymbolRelationship::new(
            SymbolRelationshipKind::ModuleMember,
            InterfaceSymbolId::new(1),
            InterfaceSymbolId::new(3),
            0,
        ),
    ];

    let exporting_module_name = SymbolName::try_new(exporting_module_name)
        .unwrap_or_else(|| panic!("test exporting module name must be valid"));

    let lookups = [
        ImportedLookupEdge::new(
            InterfaceSymbolId::new(0),
            exporting_module_name,
            exporting_module_key.clone(),
        ),
        ImportedLookupEdge::new(
            InterfaceSymbolId::new(2),
            declaration_name,
            declaration_key.clone(),
        ),
    ];

    let input = ImportedSymbolSkeletonInput::new(
        ImportedInterfaceId::new(0),
        identities,
        relationships,
        lookups,
    );

    let symbols = ImportedSymbolSkeleton::try_new(SymbolId::new(10_000), [input])
        .unwrap_or_else(|error| panic!("test imported skeleton must build: {error:?}"));

    let declaration = symbols
        .symbol_by_external_key(&declaration_key)
        .unwrap_or_else(|| panic!("test imported declaration must be present"));

    ImportedLookupFixture {
        symbols,
        declaration,
    }
}

fn imported_module_key(package: &ExternalSymbolKey, module_name: &str) -> ExternalSymbolKey {
    let module_path = ModulePathKey::try_new([module_name])
        .unwrap_or_else(|| panic!("test module path must be valid"));

    ExternalSymbolKey::module(package.clone(), module_path)
        .unwrap_or_else(|| panic!("test module key must be valid"))
}

/// Returns a process-wide compiler-known symbol view with every target role available.
pub fn available_compiler_known_symbols() -> &'static AvailableCompilerKnownSymbols {
    static SYMBOLS: OnceLock<AvailableCompilerKnownSymbols> = OnceLock::new();

    SYMBOLS.get_or_init(|| {
        let provider = match CompilerKnownSymbolProvider::build() {
            Ok(provider) => provider,
            Err(error) => panic!("test compiler-known provider must build: {error:?}"),
        };

        Arc::new(provider).available_symbols(|_| true)
    })
}

/// Returns the canonical source function key used by cross-crate semantic fixtures.
pub fn source_function_key() -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be valid");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(function) =
        SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
    else {
        panic!("function symbols must be source-declared");
    };

    function
}

/// Creates and interns one implementation-selection requirement for tests.
pub fn implementation_requirement(
    values: &SemanticValueStore,
    trait_definition: TraitSymbolId,
    subject: TypeId,
    argument: TypeId,
) -> ImplementationRequirementKey {
    let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(50));
    let owner = generic_owner(trait_definition.into());

    let substitution = match GenericSubstitutionData::try_new(
        owner,
        [GenericParameterSymbolId::Type(parameter)],
        [GenericArgument::Type(argument)],
    ) {
        Ok(substitution) => substitution,
        Err(error) => panic!("trait substitution must validate: {error:?}"),
    };

    let substitution = match values.intern_generic_substitution(substitution) {
        Ok(substitution) => substitution,
        Err(error) => panic!("trait substitution must be interned: {error:?}"),
    };

    let application = TraitApplicationData::new(trait_definition, substitution);

    let application = match values.intern_trait_application(application) {
        Ok(application) => application,
        Err(error) => panic!("trait application must be interned: {error:?}"),
    };

    ImplementationRequirementKey::new(subject, application)
}

/// Creates and interns one implementation witness for tests.
pub fn implementation_instance(
    values: &SemanticValueStore,
    symbol: u32,
) -> ImplementationInstanceId {
    let definition = NamedTraitImplementationSymbolId::from_symbol_id(SymbolId::new(symbol));

    let substitution = empty_substitution(values, definition.into());

    let instance = ImplementationInstanceData::new(definition.into(), substitution);

    match values.intern_implementation_instance(instance) {
        Ok(instance) => instance,
        Err(error) => panic!("implementation instance must be interned: {error:?}"),
    }
}

fn empty_substitution(
    values: &SemanticValueStore,
    owner: AnySymbolId,
) -> crate::GenericSubstitutionId {
    let owner = generic_owner(owner);

    let substitution = match GenericSubstitutionData::try_new(owner, [], []) {
        Ok(substitution) => substitution,
        Err(error) => panic!("empty substitution must validate: {error:?}"),
    };

    match values.intern_generic_substitution(substitution) {
        Ok(substitution) => substitution,
        Err(error) => panic!("empty substitution must be interned: {error:?}"),
    }
}

fn generic_owner(owner: AnySymbolId) -> GenericOwnerId {
    let Some(owner) = GenericOwnerId::try_new(owner) else {
        panic!("test symbol must support generic substitution");
    };

    owner
}
