//! Compiler-known and recognized standard-library catalog contracts.

#![forbid(unsafe_code)]

mod availability;
mod catalog;
#[cfg(any(test, feature = "generation"))]
mod catalog_digest;
mod implementation;
mod representation;

pub use availability::AvailabilityRule;
#[cfg(any(test, feature = "generation"))]
pub use catalog::generator_input_inventory;
pub use catalog::{
    CATALOG_SOURCE_DIGEST, COMPILER_KNOWN_CATALOG, CatalogDeclarationKind,
    CatalogDeclarationSignature, CatalogDeclarationSurface, CatalogDeclarationSurfaceSyntax,
    CatalogGenericParameter, CatalogGenericParameterKind, CatalogPath, CatalogScopeLocation,
    CatalogSourceAnchor, CatalogSourceId, CatalogSurfaceContext, CatalogSurfaceElement,
    CatalogSurfaceToken, CatalogTokenSpelling, CatalogTypeSurface, CatalogTypeSurfaceSyntax,
    CompilerKnownCatalog, CompilerKnownCatalogRoleRegistry, CompilerKnownDeclarationDescriptor,
    CompilerKnownDeclarationId, CompilerKnownDeclarationKey, CompilerKnownDeclarationOwner,
    CompilerKnownImplementationBinding, CompilerKnownRepresentationBinding,
    CompilerKnownRepresentationTarget, CompilerKnownScopeDescriptor, CompilerKnownScopeId,
    CompilerKnownScopeKey, CompilerKnownValueDescriptor, CompilerKnownValueId,
    CompilerKnownValueKey, RecognizedStandardLibraryDeclarationDescriptor,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationIdentity,
    RecognizedStandardLibraryDeclarationKey, RecognizedStandardLibraryDeclarationOwner,
    RecognizedStandardLibraryScopeDescriptor, RecognizedStandardLibraryScopeId,
    RecognizedStandardLibraryScopeKey,
};
#[cfg(any(test, feature = "generation"))]
pub use catalog::{
    CatalogBuildResult, CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics,
    CatalogEntryKind, CatalogExpectation, CatalogField, CatalogFragmentValidator, CatalogKeyDomain,
    CatalogKind, CatalogMetadataKind, CatalogRelatedKey, CatalogSource, CatalogSourceInventory,
    build_catalog,
};
#[cfg(feature = "generation")]
pub use catalog::{CatalogGenerationError, GeneratedCatalogOutput, generate_catalog_output};
pub use implementation::ImplementationHook;
pub use representation::RepresentationRole;
