use crate::{
    BackendArtifactRequest, CodegenCancellation, CodegenOptions, CodegenTarget, CodegenUnit,
};

/// Borrowed immutable inputs for one complete backend operation.
#[derive(Clone, Copy)]
pub struct CodegenRequest<'request> {
    unit: &'request CodegenUnit,
    target: &'request CodegenTarget,
    options: &'request CodegenOptions,
    artifacts: &'request BackendArtifactRequest,
    cancellation: &'request dyn CodegenCancellation,
}

impl<'request> CodegenRequest<'request> {
    /// Creates a request from validated immutable inputs.
    pub const fn new(
        unit: &'request CodegenUnit,
        target: &'request CodegenTarget,
        options: &'request CodegenOptions,
        artifacts: &'request BackendArtifactRequest,
        cancellation: &'request dyn CodegenCancellation,
    ) -> Self {
        Self {
            unit,
            target,
            options,
            artifacts,
            cancellation,
        }
    }

    /// Returns the validated MIR work item.
    pub const fn unit(self) -> &'request CodegenUnit {
        self.unit
    }

    /// Returns the validated target configuration.
    pub const fn target(self) -> &'request CodegenTarget {
        self.target
    }

    /// Returns backend-neutral generation policy.
    pub const fn options(self) -> &'request CodegenOptions {
        self.options
    }

    /// Returns the exact emitter-derived artifact request.
    pub const fn artifacts(self) -> &'request BackendArtifactRequest {
        self.artifacts
    }

    /// Returns the compilation-owned cancellation observer.
    pub const fn cancellation(self) -> &'request dyn CodegenCancellation {
        self.cancellation
    }
}
