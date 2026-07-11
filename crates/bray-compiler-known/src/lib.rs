//! Compiler-known and recognized standard-library catalog contracts.

#![forbid(unsafe_code)]

mod availability;
mod catalog;
#[cfg(feature = "generation")]
mod catalog_digest;
mod implementation;
mod representation;

pub use availability::AvailabilityRule;
#[cfg(any(test, feature = "generation"))]
pub use catalog::generator_input_inventory;
pub use catalog::{
    CATALOG_SOURCE_DIGEST, COMPILER_KNOWN_CATALOG, CatalogBuildResult, CatalogDeclarationKind,
    CatalogDeclarationSurface, CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics,
    CatalogEntryKind, CatalogExpectation, CatalogField, CatalogFragmentValidator, CatalogKeyDomain,
    CatalogKind, CatalogMetadataKind, CatalogPath, CatalogRelatedKey, CatalogScopeLocation,
    CatalogSource, CatalogSourceAnchor, CatalogSourceId, CatalogSourceInventory,
    CatalogSurfaceContext, CatalogTokenSpelling, CatalogTypeSurface, CompilerKnownCatalog,
    CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId, CompilerKnownDeclarationKey,
    CompilerKnownDeclarationOwner, CompilerKnownScopeDescriptor, CompilerKnownScopeId,
    CompilerKnownScopeKey, CompilerKnownValueDescriptor, CompilerKnownValueId,
    CompilerKnownValueKey, RecognizedStandardLibraryDeclarationDescriptor,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryDeclarationOwner, RecognizedStandardLibraryScopeDescriptor,
    RecognizedStandardLibraryScopeId, RecognizedStandardLibraryScopeKey, build_catalog,
};
#[cfg(feature = "generation")]
pub use catalog::{CatalogGenerationError, GeneratedCatalogOutput, generate_catalog_output};
pub use implementation::ImplementationHook;
pub use representation::RepresentationRole;
