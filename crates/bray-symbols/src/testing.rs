//! Shared fixtures for crates testing semantic compiler boundaries.

use std::sync::{Arc, OnceLock};

use crate::{AvailableCompilerKnownSymbols, CompilerKnownSymbolProvider};

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
