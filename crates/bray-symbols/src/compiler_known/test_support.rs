use bray_compiler_known::{CompilerKnownDeclarationKey, CompilerKnownScopeKey};

use super::CompilerKnownSymbolProvider;
use crate::StructSymbolId;

pub(crate) fn build_provider() -> CompilerKnownSymbolProvider {
    match CompilerKnownSymbolProvider::build() {
        Ok(provider) => provider,
        Err(error) => panic!("generated compiler-known catalog must build: {error:?}"),
    }
}

pub(crate) fn declaration_key(value: &str) -> CompilerKnownDeclarationKey {
    match CompilerKnownDeclarationKey::try_new(value) {
        Some(key) => key,
        None => panic!("test declaration key must be valid"),
    }
}

pub(crate) fn scope_key(value: &str) -> CompilerKnownScopeKey {
    match CompilerKnownScopeKey::try_new(value) {
        Some(key) => key,
        None => panic!("test scope key must be valid"),
    }
}

pub(crate) fn struct_id(
    provider: &CompilerKnownSymbolProvider,
    key: &CompilerKnownDeclarationKey,
) -> StructSymbolId {
    let Some(id) = provider.declaration_symbol::<StructSymbolId>(key) else {
        panic!("test struct key must resolve");
    };

    id
}
