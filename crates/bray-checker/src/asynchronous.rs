mod check;
mod cleanup;
mod diagnostic;
mod execution;
mod parts;
mod replacement;

pub(crate) use check::check_async_analysis_with_graph;
pub(crate) use diagnostic::task_start_dependency_diagnostic;
pub(crate) use cleanup::{
    CleanupShape, CleanupShapeResolver, ResolvedCleanupPart, StorageCleanupRequirements, graph_cleanup_requirements,
};

pub(crate) use execution::{ExecutionCleanupMode, execution_cleanup_dependencies, lifecycle_execution_dependency};
pub(crate) use parts::full_cleanup_parts;
