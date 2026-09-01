mod decoding;
mod encoding;
mod model;
mod presentation;
mod reference;

mod validation;

pub use model::{
    CompilerKnownSymbolReference, DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind,
    InterfaceDependency, InterfaceProductIdentity, InterfaceProductKind, InterfaceSymbolReference,
    PackageInterfaceIdentity, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    SymbolRelationship, SymbolRelationshipKind,
};

pub(crate) use decoding::{
    decode_dependencies, decode_exports, decode_metadata, decode_relationships, decode_strings,
    decode_surface, decode_symbols,
};
pub(crate) use encoding::{EncodedSurfaceSection, encode_surface};
pub use presentation::diagnostic_surface_problem;
