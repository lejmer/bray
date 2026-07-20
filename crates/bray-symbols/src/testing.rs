//! Shared fixtures for crates testing semantic compiler boundaries.

use std::sync::{Arc, OnceLock};

use bray_declarations::DeclarationId;

use crate::{
    AnySymbolId, AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider, GenericArgument,
    GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    GenericTypeParameterSymbolId, ImplementationInstanceData, ImplementationInstanceId,
    ImplementationRequirementKey, ModulePathKey, NamedTraitImplementationSymbolId, PackageIdentity,
    SemanticValueStore, SymbolId, SymbolKey, SymbolKind, SymbolRootKey, TraitApplicationData,
    TraitSymbolId, TypeId,
};

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
