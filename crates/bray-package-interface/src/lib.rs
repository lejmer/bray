//! Deterministic wire encoding and bounded validation for compiled package interfaces.

#![forbid(unsafe_code)]

mod construction;
mod diagnostic;
mod hash;
mod header;
mod limits;
mod section;
mod surface;
mod validation;
mod wire;

pub use construction::{
    ImportedSymbolConstructionError, LoadedInterfaceSurface, construct_imported_symbol_skeletons,
};
pub use diagnostic::InterfaceValidationError;
pub use hash::{InterfaceArtifactHash, InterfaceContentHash, InterfaceSectionHash};
pub use header::{
    CURRENT_FORMAT_REVISION, InterfaceFormatRevision, InterfaceHeader, InterfaceLanguageRevision,
    InterfaceRequiredFlags,
};
pub use limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
pub use section::{InterfaceSectionTag, ValidatedInterfaceSection};
pub use surface::{
    DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceDependency,
    InterfaceProductIdentity, InterfaceProductKind, InterfaceSymbolReference,
    PackageInterfaceIdentity, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    SymbolRelationship, SymbolRelationshipKind,
};
pub use validation::ValidatedPackageInterface;
