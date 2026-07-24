//! Declaration discovery from Bray syntax trees.

#![forbid(unsafe_code)]

mod chunk;
mod diagnostic;
mod discover;
mod id;
mod merge;
mod name;
mod record;
mod result;
mod surface;
mod table;
#[cfg(test)]
mod test_support;

pub use chunk::{DeclarationChunk, DiscoveredDeclaration, DiscoveredModulePart};
pub use discover::discover_source_unit_declarations;
pub use id::{ContainerId, DeclarationId, ModulePartId};
pub use merge::{merge_declaration_chunks, merge_selected_declaration_chunks};
pub use name::{DeclarationName, ImplementationDeclarationName, ModulePath};
pub use record::{
    ContainerKind, ContainerRecord, DeclarationKind, DeclarationRecord, ModulePartRecord,
};
pub use result::{DeclarationChunkResult, DeclarationTableResult};
pub use surface::{DeclarationSurface, SyntaxAnchor};
pub use table::DeclarationTable;
