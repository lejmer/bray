mod audit;
mod availability;
mod error;
mod fact;
mod provider;
mod role;
mod validation;

pub use audit::{
    CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError, CompilerKnownCatalogAuditReport,
    CompilerKnownTargetProfile,
};
pub use availability::AvailableCompilerKnownSymbols;
pub use error::CompilerKnownSymbolBuildError;
pub use fact::{CompilerKnownDeclarationFact, CompilerKnownSymbolFactKey};
pub use provider::{CompilerKnownScopeSymbolId, CompilerKnownSymbolProvider};
pub use role::{
    CompilerKnownIterationProtocol, CompilerKnownOperationContract, CompilerKnownSymbolRoleRegistry,
};

#[cfg(test)]
mod test_support;
