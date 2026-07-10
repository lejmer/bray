//! Compiler-known and recognized standard-library catalog contracts.

#![forbid(unsafe_code)]

mod availability;
mod catalog;
mod implementation;
mod representation;

pub use availability::AvailabilityRule;
pub use catalog::{
    CatalogBuildResult, CatalogDeclarationKind, CatalogDeclarationSurface, CatalogDiagnostic,
    CatalogDiagnosticKind, CatalogDiagnostics, CatalogEntryKind, CatalogExpectation, CatalogField,
    CatalogFragmentValidator, CatalogKeyDomain, CatalogKind, CatalogMetadataKind, CatalogPath,
    CatalogRelatedKey, CatalogScopeLocation, CatalogSource, CatalogSourceAnchor, CatalogSourceId,
    CatalogSourceInventory, CatalogSurfaceContext, CatalogTokenSpelling, CatalogTypeSurface,
    CompilerKnownCatalog, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
    CompilerKnownDeclarationKey, CompilerKnownDeclarationOwner, CompilerKnownScopeDescriptor,
    CompilerKnownScopeId, CompilerKnownScopeKey, CompilerKnownValueDescriptor,
    CompilerKnownValueId, CompilerKnownValueKey, RecognizedStandardLibraryDeclarationDescriptor,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryDeclarationOwner, RecognizedStandardLibraryScopeDescriptor,
    RecognizedStandardLibraryScopeId, RecognizedStandardLibraryScopeKey, build_catalog,
    embedded_source_inventory,
};
pub use implementation::ImplementationHook;
pub use representation::RepresentationRole;
