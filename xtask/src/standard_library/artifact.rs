use std::path::PathBuf;

use bray_emitter::{ArtifactKind, ManagedFilesystemDestination, resolve_published_artifact};
use bray_symbols::ProductIdentity;

use super::command::BuildError;

pub(super) fn resolve_executable(
    destination: impl Into<ManagedFilesystemDestination>,
    product: &ProductIdentity,
    operation: &'static str,
) -> Result<PathBuf, BuildError> {
    resolve_published_artifact(destination, product, ArtifactKind::Executable, 0).map_err(|error| {
        BuildError::conformance(
            operation,
            format!("could not resolve emitted executable: {error:?}"),
        )
    })
}
