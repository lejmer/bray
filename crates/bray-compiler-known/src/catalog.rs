mod descriptor;
mod diagnostic;
mod entry;
mod graph;
mod id;
mod key;
mod loader;
mod parser;
mod source;
mod validation;

pub use descriptor::{
    CatalogDeclarationKind, CatalogScopeLocation, CatalogSurfaceContext,
    CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationOwner,
    CompilerKnownScopeDescriptor, CompilerKnownValueDescriptor,
    RecognizedStandardLibraryDeclarationDescriptor, RecognizedStandardLibraryDeclarationOwner,
    RecognizedStandardLibraryScopeDescriptor,
};
pub use diagnostic::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics, CatalogEntryKind,
    CatalogExpectation, CatalogField, CatalogKeyDomain, CatalogMetadataKind, CatalogRelatedKey,
};
pub use graph::CompilerKnownCatalog;
pub use id::{
    CatalogSourceId, CompilerKnownDeclarationId, CompilerKnownScopeId, CompilerKnownValueId,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryScopeId,
};
pub use key::{
    CatalogPath, CompilerKnownDeclarationKey, CompilerKnownScopeKey, CompilerKnownValueKey,
    RecognizedStandardLibraryDeclarationKey, RecognizedStandardLibraryScopeKey,
};
pub use loader::{CatalogBuildResult, CatalogFragmentValidator, build_catalog};
pub use source::{
    CatalogDeclarationSurface, CatalogKind, CatalogSource, CatalogSourceAnchor,
    CatalogSourceInventory, CatalogTokenSpelling, CatalogTypeSurface, embedded_source_inventory,
};
