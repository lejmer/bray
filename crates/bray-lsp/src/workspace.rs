mod error;
mod products;
mod sources;
mod state;

pub(crate) use error::WorkspaceError;
pub(crate) use sources::offset_for_position;
pub(crate) use state::{CompilationRevision, DocumentSnapshot, Workspace, WorkspaceUpdate};

#[cfg(test)]
mod tests;
