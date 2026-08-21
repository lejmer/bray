use std::sync::Arc;

use bray_base::Cancellation;
use bray_base::NonEmptySharedStr;
use bray_target::{CodeModel, RelocationModel, TargetIdentity};

use crate::{ArtifactContent, CodegenFailure, CodegenTarget};

/// Exact target contract required to combine serialized backend bitcode.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendBitcodeTargetContract {
    target: TargetIdentity,
    triple: NonEmptySharedStr,
    data_layout: NonEmptySharedStr,
    relocation_model: RelocationModel,
    code_model: CodeModel,
}

impl BackendBitcodeTargetContract {
    /// Creates the contract from one validated target and backend-native data layout.
    pub fn try_new(target: &CodegenTarget, data_layout: impl Into<Arc<str>>) -> Option<Self> {
        Some(Self {
            target: target.identity().clone(),
            triple: NonEmptySharedStr::try_new(Arc::<str>::from(target.triple()))?,
            data_layout: NonEmptySharedStr::try_new(data_layout)?,
            relocation_model: target.relocation_model(),
            code_model: target.code_model(),
        })
    }

    /// Returns the stable target identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the canonical backend target triple.
    pub fn triple(&self) -> &str {
        self.triple.as_str()
    }

    /// Returns the exact backend-native data-layout string.
    pub fn data_layout(&self) -> &str {
        self.data_layout.as_str()
    }

    /// Returns the selected relocation model.
    pub const fn relocation_model(&self) -> RelocationModel {
        self.relocation_model
    }

    /// Returns the selected code model.
    pub const fn code_model(&self) -> CodeModel {
        self.code_model
    }
}

/// Result of one compiler-host bitcode optimization operation.
pub enum BackendBitcodeOptimizationOutcome {
    /// Summary-bearing immutable bitcode was formed successfully.
    Complete(ArtifactContent),
    /// Cancellation ended the operation without publishing content.
    Cancelled,
}

/// Compiler-host boundary for operations unavailable through the safe backend API.
pub trait BackendBitcodeOptimizer: Send + Sync {
    /// Adds the module summary required for LLVM ThinLTO import planning.
    fn add_thin_lto_summary(
        &self,
        bitcode: &ArtifactContent,
        cancellation: &dyn Cancellation,
    ) -> Result<BackendBitcodeOptimizationOutcome, CodegenFailure>;
}
