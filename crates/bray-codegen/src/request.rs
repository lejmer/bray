use bray_base::Cancellation;
use bray_symbols::ProductKind;

use crate::{
    BackendArtifactRequest, BackendCapabilityRevision, BackendIdentity, CodegenMappings,
    CodegenOptions, CodegenTarget, CodegenUnit, DebugInformationMode, DebugInformationOutputMode,
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
    /// Validates and creates a request from authoritative immutable inputs.
    pub fn try_new(
        unit: &'request CodegenUnit,
        backend: &'request BackendIdentity,
        capability_revision: BackendCapabilityRevision,
        product: ProductKind,
        target: &'request CodegenTarget,
        mappings: &'request CodegenMappings,
        options: &'request CodegenOptions,
        artifacts: &'request BackendArtifactRequest,
        cancellation: &'request dyn Cancellation,
    ) -> Result<Self, CodegenRequestBuildError> {
        if artifacts.unit() != unit.key() {
            return Err(CodegenRequestBuildError::ArtifactUnitMismatch);
        }

        if mappings.unit() != unit.key() {
            return Err(CodegenRequestBuildError::MappingUnitMismatch);
        }

        if mappings.target() != target {
            return Err(CodegenRequestBuildError::MappingTargetMismatch);
        }

        if !target.matches_mir_target(unit.target()) {
            return Err(CodegenRequestBuildError::TargetMismatch);
        }

        if unit.mir_units().any(|mir| {
            let bray_ir::MirUnitKind::ExecutableHost(host) = mir.kind() else {
                return false;
            };

            host.target() != target.identity() || host.panic_abi() != target.panic_abi()
        }) {
            return Err(CodegenRequestBuildError::RuntimeContractMismatch);
        }

        let debug_information = options.debug_information();
        let debug_output = artifacts.debug_information();

        if !debug_contract_matches(debug_information, debug_output) {
            return Err(CodegenRequestBuildError::DebugInformationMismatch);
        }

        if debug_information != DebugInformationMode::None && !mappings.covers_debug_sources(unit) {
            return Err(CodegenRequestBuildError::DebugMappingCoverageMismatch);
        }

        Ok(Self {
            unit,
            backend,
            capability_revision,
            product,
            target,
            mappings,
            options,
            artifacts,
            cancellation,
        })
    }

    /// Returns exact backend-neutral realization facts for this unit.
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

/// A contract violation that prevents creation of a backend request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodegenRequestBuildError {
    /// The artifact request belongs to another codegen unit.
    ArtifactUnitMismatch,
    /// Realization mappings belong to another code generation unit.
    MappingUnitMismatch,
    /// Realization mappings were computed for another code generation target.
    MappingTargetMismatch,
    /// Generation and serialization disagree about whether debug information exists.
    DebugInformationMismatch,
    /// Requested debug information lacks one or more MIR source mappings.
    DebugMappingCoverageMismatch,
    /// MIR lowering facts do not match the selected codegen target.
    TargetMismatch,
    /// Executable-host runtime facts do not match target code generation.
    RuntimeContractMismatch,
}

const fn debug_contract_matches(
    information: DebugInformationMode,
    output: DebugInformationOutputMode,
) -> bool {
    matches!(
        (information, output),
        (DebugInformationMode::None, DebugInformationOutputMode::Omit)
            | (
                DebugInformationMode::LineTables | DebugInformationMode::Full,
                DebugInformationOutputMode::Embedded | DebugInformationOutputMode::Separate
            )
    )
}
