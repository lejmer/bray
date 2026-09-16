use bray_base::Cancellation;
use bray_symbols::ProductKind;

use crate::{
    BackendArtifactRequest, BackendCapabilityRevision, BackendIdentity, CodegenMappings,
    CodegenOptions, CodegenTarget, CodegenUnit,
};

/// Borrowed immutable inputs for one complete backend operation.
#[derive(Clone, Copy)]
pub struct CodegenRequest<'request> {
    unit: &'request CodegenUnit,
    backend: &'request BackendIdentity,
    capability_revision: BackendCapabilityRevision,
    product: ProductKind,
    target: &'request CodegenTarget,
    mappings: &'request CodegenMappings,
    options: &'request CodegenOptions,
    artifacts: &'request BackendArtifactRequest,
    cancellation: &'request dyn Cancellation,
}

impl<'request> CodegenRequest<'request> {
    /// Creates a request from authoritative immutable inputs.
    pub const fn new(
        unit: &'request CodegenUnit,
        backend: &'request BackendIdentity,
        capability_revision: BackendCapabilityRevision,
        product: ProductKind,
        target: &'request CodegenTarget,
        mappings: &'request CodegenMappings,
        options: &'request CodegenOptions,
        artifacts: &'request BackendArtifactRequest,
        cancellation: &'request dyn Cancellation,
    ) -> Self {
        Self {
            unit,
            backend,
            capability_revision,
            product,
            target,
            mappings,
            options,
            artifacts,
            cancellation,
        }
    }

    /// Returns exact backend-neutral realization mappings for this unit.
    pub const fn mappings(self) -> &'request CodegenMappings {
        self.mappings
    }

    /// Returns the validated MIR work item.
    pub const fn unit(self) -> &'request CodegenUnit {
        self.unit
    }

    /// Returns the authoritative selected backend identity.
    pub const fn backend(self) -> &'request BackendIdentity {
        self.backend
    }

    /// Returns the capability declaration revision required by this request.
    pub const fn capability_revision(self) -> BackendCapabilityRevision {
        self.capability_revision
    }

    /// Returns the selected product kind.
    pub const fn product(self) -> ProductKind {
        self.product
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
    pub const fn cancellation(self) -> &'request dyn Cancellation {
        self.cancellation
    }
}
