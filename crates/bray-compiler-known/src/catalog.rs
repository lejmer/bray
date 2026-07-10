mod descriptor;
mod graph;
mod id;
mod key;
mod source;

pub use descriptor::{
    CatalogDeclarationKind, CatalogScopeLocation, CompilerKnownDeclarationDescriptor,
    CompilerKnownDeclarationOwner, CompilerKnownScopeDescriptor, CompilerKnownValueDescriptor,
    RecognizedStandardLibraryDeclarationDescriptor, RecognizedStandardLibraryDeclarationOwner,
    RecognizedStandardLibraryScopeDescriptor,
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
pub use source::{
    CatalogDeclarationSurface, CatalogKind, CatalogSource, CatalogSourceAnchor,
    CatalogSourceInventory, CatalogTokenSpelling, CatalogTypeSurface, embedded_source_inventory,
};
