//! Compiler-known and recognized standard-library catalog contracts.

#![forbid(unsafe_code)]

mod availability;
mod catalog;
mod implementation;
mod representation;

pub use availability::AvailabilityRule;
pub use catalog::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogKind, CatalogPath,
    CatalogScopeLocation, CatalogSource, CatalogSourceAnchor, CatalogSourceId,
    CatalogSourceInventory, CatalogTokenSpelling, CatalogTypeSurface, CompilerKnownCatalog,
    CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId, CompilerKnownDeclarationKey,
    CompilerKnownDeclarationOwner, CompilerKnownScopeDescriptor, CompilerKnownScopeId,
    CompilerKnownScopeKey, CompilerKnownValueDescriptor, CompilerKnownValueId,
    CompilerKnownValueKey, RecognizedStandardLibraryDeclarationDescriptor,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryDeclarationOwner, RecognizedStandardLibraryScopeDescriptor,
    RecognizedStandardLibraryScopeId, RecognizedStandardLibraryScopeKey, embedded_source_inventory,
};
pub use implementation::ImplementationHook;
pub use representation::RepresentationRole;
