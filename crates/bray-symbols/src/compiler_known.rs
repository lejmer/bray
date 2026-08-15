mod audit;
mod availability;
mod error;
mod provider;
mod role;
mod semantic;
mod validation;

pub use audit::{
    CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError, CompilerKnownCatalogAuditReport,
    CompilerKnownTargetProfile,
};
pub use availability::AvailableCompilerKnownSymbols;
pub use error::CompilerKnownSymbolBuildError;
pub use provider::{CompilerKnownScopeSymbolId, CompilerKnownSymbolProvider};
pub use role::{
    CompilerKnownIterationProtocol, CompilerKnownOperationContract,
    CompilerKnownOrderingRepresentation, CompilerKnownResultRepresentation,
    CompilerKnownRunResultRepresentation, CompilerKnownSymbolRoleRegistry,
};
pub use semantic::{CompilerKnownDeclarationSemantics, CompilerKnownSemanticKey};

#[cfg(test)]
mod test_support;
