//! Shared fixtures for crates testing semantic compiler boundaries.

use std::sync::{Arc, OnceLock};

use bray_declarations::DeclarationId;

use crate::{
    AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider, ModulePathKey, PackageIdentity,
    SymbolKey, SymbolKind, SymbolRootKey,
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
