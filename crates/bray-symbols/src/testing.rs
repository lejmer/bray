//! Shared fixtures for crates testing semantic compiler boundaries.

use std::sync::{Arc, OnceLock};

use bray_declarations::DeclarationId;

use crate::{
    AnySymbolId, AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider, GenericArgument,
    GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    GenericTypeParameterSymbolId, ImplementationInstanceData, ImplementationInstanceId,
    ImplementationRequirementKey, ImportedSymbolSkeleton, ModulePathKey,
    NamedTraitImplementationSymbolId, PackageIdentity, SemanticValueStore, SymbolId, SymbolKey,
    SymbolKind, SymbolRootKey, TraitApplicationData, TraitSymbolId, TypeData, TypeId,
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
    imported_lookup_fixture_at_module(
        package_name,
        [module_name],
        declaration_kind,
        declaration_name,
    )
}

/// Builds one imported package with an exported logical module path and declaration.
pub fn imported_lookup_fixture_at_module<I, S>(
    package_name: &str,
    module_path: I,
    declaration_kind: SymbolKind,
    declaration_name: &str,
) -> ImportedLookupFixture
where
    I: IntoIterator<Item = S>,
    S: Into<Arc<str>>,
{
    let module_path = module_path.into_iter().map(Into::into).collect::<Vec<_>>();

    imported_lookup_fixture_from_paths(
        package_name,
        &module_path,
        &module_path,
        declaration_kind,
        declaration_name,
        crate::imported::test_support::ModuleRouteShape::Exact,
    )
}

/// Builds one imported declaration under a logical module path with declared prefixes.
pub fn imported_lookup_fixture_with_declared_prefixes<I, S>(
    package_name: &str,
    module_path: I,
    declaration_kind: SymbolKind,
    declaration_name: &str,
) -> ImportedLookupFixture
where
    I: IntoIterator<Item = S>,
    S: Into<Arc<str>>,
{
    let module_path = module_path.into_iter().map(Into::into).collect::<Vec<_>>();

    imported_lookup_fixture_from_paths(
        package_name,
        &module_path,
        &module_path,
        declaration_kind,
        declaration_name,
        crate::imported::test_support::ModuleRouteShape::DeclaredPrefixes,
    )
}

/// Builds one imported declaration re-exported through another module.
pub fn imported_reexport_lookup_fixture(
    package_name: &str,
    defining_module_name: &str,
    exporting_module_name: &str,
    declaration_kind: SymbolKind,
    declaration_name: &str,
) -> ImportedLookupFixture {
    imported_lookup_fixture_from_paths(
        package_name,
        &[Arc::from(defining_module_name)],
        &[Arc::from(exporting_module_name)],
        declaration_kind,
        declaration_name,
        crate::imported::test_support::ModuleRouteShape::Exact,
    )
}

fn imported_lookup_fixture_from_paths(
    package_name: &str,
    defining_module_path: &[Arc<str>],
    exporting_module_path: &[Arc<str>],
    declaration_kind: SymbolKind,
    declaration_name: &str,
    route_shape: crate::imported::test_support::ModuleRouteShape,
) -> ImportedLookupFixture {
    let fixture = crate::imported::test_support::lookup_input_fixture_with_routes(
        0,
        crate::imported::test_support::LookupInputSpec {
            package_name,
            defining_module_path,
            exporting_module_path,
            declaration_kind,
            declaration_name,
            lookup_name: declaration_name,
            route_shape,
        },
    );

    let symbols = ImportedSymbolSkeleton::try_new(SymbolId::new(10_000), [fixture.input])
        .unwrap_or_else(|error| panic!("test imported skeleton must build: {error:?}"));

    let declaration = symbols
        .symbol_by_external_key(&fixture.declaration_key)
        .unwrap_or_else(|| panic!("test imported declaration must be present"));

    ImportedLookupFixture {
        symbols,
        declaration,
    }
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

/// Interns one valid semantic type for tests.
pub fn intern_type(values: &SemanticValueStore, data: TypeData) -> TypeId {
    match values.intern_type(data) {
        Ok(ty) => ty,
        Err(error) => panic!("test type must be valid: {error:?}"),
    }
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
