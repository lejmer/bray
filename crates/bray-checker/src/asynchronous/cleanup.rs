mod execution;
mod scope;
mod shape;

pub(in crate::asynchronous) use scope::prepare_cleanup;
pub(crate) use scope::{StorageCleanupRequirements, graph_cleanup_requirements};
pub(crate) use shape::{CleanupShape, CleanupShapeResolver, ResolvedCleanupPart};
