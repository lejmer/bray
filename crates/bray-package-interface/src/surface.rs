mod decoding;
mod encoding;
mod model;
mod reference;

mod validation;

pub use model::{
    DependencyInterfaceId, ExportedLookupEdge, ExportedLookupKind, InterfaceDependency,
    InterfaceProductIdentity, InterfaceProductKind, InterfaceSymbolReference,
    PackageInterfaceIdentity, PackageInterfaceSurface, PackageInterfaceSurfaceBuildError,
    SymbolRelationship, SymbolRelationshipKind,
};

pub(crate) use decoding::decode_surface;
