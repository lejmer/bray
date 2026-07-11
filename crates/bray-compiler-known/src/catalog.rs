mod descriptor;
#[cfg(any(test, feature = "generation"))]
mod diagnostic;
#[cfg(any(test, feature = "generation"))]
mod entry;
mod generated;
mod graph;
mod id;
mod key;
#[cfg(any(test, feature = "generation"))]
mod loader;
#[cfg(any(test, feature = "generation"))]
mod parser;
mod source;
mod surface;
#[cfg(any(test, feature = "generation"))]
mod validation;

#[cfg(any(test, feature = "generation"))]
mod generation;
#[cfg(any(test, feature = "generation"))]
mod rendering;

pub use descriptor::{
    CatalogDeclarationKind, CatalogScopeLocation, CatalogSurfaceContext,
    CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationOwner,
    CompilerKnownScopeDescriptor, CompilerKnownValueDescriptor,
    RecognizedStandardLibraryDeclarationDescriptor, RecognizedStandardLibraryDeclarationIdentity,
    RecognizedStandardLibraryDeclarationOwner, RecognizedStandardLibraryScopeDescriptor,
};
#[cfg(any(test, feature = "generation"))]
pub use diagnostic::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics, CatalogEntryKind,
    CatalogExpectation, CatalogField, CatalogKeyDomain, CatalogMetadataKind, CatalogRelatedKey,
};
pub use generated::{CATALOG_SOURCE_DIGEST, COMPILER_KNOWN_CATALOG};
pub use graph::CompilerKnownCatalog;
pub use id::{
    CatalogSourceId, CompilerKnownDeclarationId, CompilerKnownScopeId, CompilerKnownValueId,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryScopeId,
};
pub use key::{
    CatalogPath, CompilerKnownDeclarationKey, CompilerKnownScopeKey, CompilerKnownValueKey,
    RecognizedStandardLibraryDeclarationKey, RecognizedStandardLibraryScopeKey,
};
#[cfg(any(test, feature = "generation"))]
pub use loader::{CatalogBuildResult, CatalogFragmentValidator, build_catalog};
#[cfg(any(test, feature = "generation"))]
pub use source::generator_input_inventory;
pub use source::{
    CatalogDeclarationSurface, CatalogSourceAnchor, CatalogTokenSpelling, CatalogTypeSurface,
};
#[cfg(any(test, feature = "generation"))]
pub use source::{CatalogKind, CatalogSource, CatalogSourceInventory};
pub use surface::{
    CatalogDeclarationSurfaceSyntax, CatalogSurfaceElement, CatalogSurfaceToken,
    CatalogTypeSurfaceSyntax,
};

#[cfg(feature = "generation")]
pub use generation::{CatalogGenerationError, GeneratedCatalogOutput, generate_catalog_output};
