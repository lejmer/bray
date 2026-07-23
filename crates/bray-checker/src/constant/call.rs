use std::sync::Arc;

use bray_base::shared_slice;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{CallableInstanceData, ConstantValueId, ImplementationInstanceId, TypeId};

use crate::CheckerFactResult;

use super::ConstantEvaluationLimits;

/// One selected constant call after its argument expressions have evaluated.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantCallRequest {
    callable: CallableInstanceData,
    selected_implementation: Option<ImplementationInstanceId>,
    arguments: Arc<[ConstantValueId]>,
    result_type: TypeId,
    limits: ConstantEvaluationLimits,
}

impl ConstantCallRequest {
    /// Creates a request for one exact selected callable and closed argument list.
    pub fn new(
        callable: CallableInstanceData,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: impl IntoIterator<Item = ConstantValueId>,
        result_type: TypeId,
        limits: ConstantEvaluationLimits,
    ) -> Self {
        Self {
            callable,
            selected_implementation,
            arguments: shared_slice(arguments),
            result_type,
            limits,
        }
    }

    /// Returns the exact substituted callable selected for evaluation.
    pub const fn callable(&self) -> CallableInstanceData {
        self.callable
    }

    /// Returns the selected implementation when the callable is a trait fulfillment.
    pub const fn selected_implementation(&self) -> Option<ImplementationInstanceId> {
        self.selected_implementation
    }

    /// Returns closed arguments in callable parameter order.
    pub fn arguments(&self) -> &[ConstantValueId] {
        &self.arguments
    }

    /// Returns the selected call result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns the deterministic limits available to the nested evaluation.
    pub const fn limits(&self) -> ConstantEvaluationLimits {
        self.limits
    }
}

/// The caller-owned result of requesting one selected constant call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConstantCallResolution {
    /// The callable evaluated and owns the accompanying nested diagnostics.
    Evaluated(DiagnosticResult<ConstantValueId>),
    /// The compilation fact graph found a recursive constant-call cycle.
    Cycle,
    /// The selected callable cannot execute in constant context.
    Ineligible,
}

/// Resolves selected constant calls through the caller's demand-driven fact graph.
pub trait ConstantCallResolver: Sync {
    /// Returns whether the selected callable may execute in a constant context.
    fn is_constant_callable(&self, callable: CallableInstanceData) -> CheckerFactResult<bool>;

    /// Evaluates one exact call without forcing unrelated semantic facts.
    fn resolve(&self, request: &ConstantCallRequest) -> CheckerFactResult<ConstantCallResolution>;
}
