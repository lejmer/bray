use std::sync::Arc;

use bray_compiler_known::{CompilerKnownDeclarationKey, CompilerKnownScopeKey, ImplementationHook};

use super::CompilerKnownSymbolProvider;
use crate::{
    CompilerKnownScopeSymbolId, FunctionSymbolId, StructSymbolId, SymbolKind, SymbolOrigin,
    SymbolProvider,
};

#[test]
fn generated_catalog_builds_deterministic_typed_symbols() {
    let first = build_provider();
    let second = build_provider();

    assert_eq!(first, second);
    assert_eq!(first.environment().id().symbol_id().raw(), 0);
    assert_eq!(first.modules().len(), 2);

    let bool_key = declaration_key("Bool");
    let Some(bool_id) = first.declaration_symbol::<StructSymbolId>(&bool_key) else {
        panic!("Bool must have a compiler-known symbol");
    };

    assert_eq!(bool_id.kind(), SymbolKind::Struct);
    assert_eq!(bool_id.symbol_id().raw(), 3);
}

#[test]
fn stable_scope_and_declaration_keys_recover_only_exact_categories() {
    let provider = build_provider();
    let ambient = scope_key("Ambient");

    assert_eq!(
        provider.scope_symbol(&ambient),
        Some(CompilerKnownScopeSymbolId::Environment(
            provider.environment().id()
        ))
    );
    assert_eq!(provider.scope_symbol(&scope_key("Missing")), None);
    assert_eq!(provider.scope_symbols().len(), 3);

    let bool_key = declaration_key("Bool");

    assert!(provider.fact_key::<StructSymbolId>(&bool_key).is_some());
    assert_eq!(provider.fact_key::<FunctionSymbolId>(&bool_key), None);
    assert_eq!(
        provider.declaration_symbol::<StructSymbolId>(&declaration_key("Missing")),
        None
    );
    assert_eq!(provider.declaration_symbols().len(), 12);
}

#[test]
fn ordinary_records_retain_catalog_origin_and_typed_containment() {
    let provider = build_provider();
    let bool_key = declaration_key("Bool");
    let raw_pointer_key = declaration_key("RawPointer");
    let memory_copy_key = declaration_key("MemoryCopy");

    let bool_id = struct_id(&provider, &bool_key);
    let raw_pointer_id = struct_id(&provider, &raw_pointer_key);

    let Some(bool_symbol) = SymbolProvider::<StructSymbolId>::symbol(&provider, bool_id) else {
        panic!("Bool struct record must resolve");
    };

    let Some(raw_pointer) = SymbolProvider::<StructSymbolId>::symbol(&provider, raw_pointer_id)
    else {
        panic!("RawPointer struct record must resolve");
    };

    assert_eq!(bool_symbol.origin(), SymbolOrigin::CompilerKnown);
    assert_eq!(raw_pointer.fields().len(), 1);
    assert_eq!(bool_symbol.declaration(), None);
    assert!(bool_symbol.compiler_known_declaration().is_some());
    assert!(bool_symbol.compiler_known_surface().is_some());
    assert!(provider.environment().structures().contains(&bool_id));
    assert_eq!(
        provider.environment().modules(),
        [provider.modules()[0].id(), provider.modules()[1].id()]
    );

    let Some(function_id) = provider.declaration_symbol::<FunctionSymbolId>(&memory_copy_key)
    else {
        panic!("MemoryCopy must have a symbol");
    };

    let Some(memory_copy) = SymbolProvider::<FunctionSymbolId>::symbol(&provider, function_id)
    else {
        panic!("MemoryCopy function record must resolve");
    };

    assert_eq!(memory_copy.origin(), SymbolOrigin::CompilerProvided);
}

#[test]
fn declaration_facts_are_lazy_static_generated_surfaces() {
    let provider = build_provider();
    let key = declaration_key("MemoryCopy");
    let Some(fact_key) = provider.fact_key::<FunctionSymbolId>(&key) else {
        panic!("MemoryCopy fact key must be typed as a function");
    };

    let Some(fact) = provider.declaration_fact(fact_key) else {
        panic!("generated MemoryCopy facts must resolve");
    };

    assert_eq!(fact.descriptor().key(), &key);
    assert_eq!(
        fact.descriptor().implementation_hook(),
        Some(ImplementationHook::MemoryCopy)
    );
    assert_eq!(fact.surface().kind(), fact.descriptor().kind());
    assert!(!fact.surface().elements().is_empty());
}

#[test]
fn concurrent_provider_construction_and_fact_reads_are_deterministic() {
    let expected = Arc::new(build_provider());
    let workers = (0..4)
        .map(|_| {
            let expected = Arc::clone(&expected);

            std::thread::spawn(move || {
                let actual = build_provider();

                assert_eq!(actual, *expected);

                let key = declaration_key("Bool");
                let Some(fact_key) = actual.fact_key::<StructSymbolId>(&key) else {
                    panic!("Bool fact key must resolve concurrently");
                };

                let Some(fact) = actual.declaration_fact(fact_key) else {
                    panic!("Bool facts must resolve concurrently");
                };

                assert_eq!(fact.descriptor().key(), &key);
            })
        })
        .collect::<Vec<_>>();

    for worker in workers {
        if let Err(error) = worker.join() {
            panic!("parallel compiler-known provider failed: {error:?}");
        }
    }
}

fn build_provider() -> CompilerKnownSymbolProvider {
    match CompilerKnownSymbolProvider::build() {
        Ok(provider) => provider,
        Err(error) => panic!("generated compiler-known catalog must build: {error:?}"),
    }
}

fn declaration_key(value: &str) -> CompilerKnownDeclarationKey {
    match CompilerKnownDeclarationKey::try_new(value) {
        Some(key) => key,
        None => panic!("test declaration key must be valid"),
    }
}

fn scope_key(value: &str) -> CompilerKnownScopeKey {
    match CompilerKnownScopeKey::try_new(value) {
        Some(key) => key,
        None => panic!("test scope key must be valid"),
    }
}

fn struct_id(
    provider: &CompilerKnownSymbolProvider,
    key: &CompilerKnownDeclarationKey,
) -> StructSymbolId {
    let Some(id) = provider.declaration_symbol::<StructSymbolId>(key) else {
        panic!("test struct key must resolve");
    };

    id
}
