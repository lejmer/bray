mod error;
mod fact;
mod provider;

pub use error::CompilerKnownSymbolBuildError;
pub use fact::{CompilerKnownDeclarationFact, CompilerKnownSymbolFactKey};
pub use provider::{CompilerKnownScopeSymbolId, CompilerKnownSymbolProvider};

#[cfg(test)]
mod test_support;
