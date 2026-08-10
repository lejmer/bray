use std::sync::Arc;

use bray_codegen::{CodegenFailure, CodegenTarget, OptimizationLevel};

use crate::machine::validate_target_configuration;

/// Immutable LLVM configuration reused by compatible generation tasks.
#[derive(Debug)]
pub(crate) struct LlvmBackendSession {
    target: CodegenTarget,
    optimization: OptimizationLevel,
    features: Arc<str>,
}

impl LlvmBackendSession {
    pub(crate) fn try_new(
        target: &CodegenTarget,
        optimization: OptimizationLevel,
    ) -> Result<Self, CodegenFailure> {
        validate_target_configuration(target)?;

        let features = target
            .features()
            .map(|feature| format!("+{feature}"))
            .collect::<Vec<_>>()
            .join(",")
            .into();

        Ok(Self {
            // The immutable session owns target policy beyond the request borrow.
            target: target.clone(),
            optimization,
            features,
        })
    }

    pub(crate) const fn target(&self) -> &CodegenTarget {
        &self.target
    }

    pub(crate) const fn optimization(&self) -> OptimizationLevel {
        self.optimization
    }

    pub(crate) fn features(&self) -> &str {
        &self.features
    }
}
