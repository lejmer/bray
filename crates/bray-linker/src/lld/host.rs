use std::ffi::OsString;

use bray_base::Cancellation;

use super::LldFlavor;
use crate::{ExternalToolFailure, ExternalToolOutput};

/// Packaging-provided boundary to an LLD library embedded in the compiler process.
pub trait EmbeddedLldHost: Send + Sync {
    /// Invokes one LLD flavor with an explicit deterministic argument vector.
    fn run(
        &self,
        flavor: LldFlavor,
        arguments: &[OsString],
        cancellation: &dyn Cancellation,
    ) -> Result<ExternalToolOutput, ExternalToolFailure>;
}
