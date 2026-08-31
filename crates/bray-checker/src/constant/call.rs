use std::sync::Arc;

use bray_base::shared_slice;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableInstanceData, ConstantInstanceKey, ConstantValueId,
    ImplementationInstanceId, SymbolKey, TypeId,
};

use crate::CheckerQueryResult;

use super::{ConstantEvaluationLimits, ConstantEvaluationUsage, ConstantReferenceResolution};

/// One evaluated constant call and its transitive deterministic work usage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EvaluatedConstantCall {
    value: ConstantValueId,
    usage: ConstantEvaluationUsage,
}

impl EvaluatedConstantCall {
    /// Creates a completed call result with its transitive work usage.
    pub const fn new(value: ConstantValueId, usage: ConstantEvaluationUsage) -> Self {
        Self { value, usage }
    }

    /// Returns the closed call result.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }

    /// Returns work consumed by the complete nested evaluation.
    pub const fn usage(self) -> ConstantEvaluationUsage {
        self.usage
    }
}

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
    Evaluated(DiagnosticResult<EvaluatedConstantCall>),
    /// The compilation query graph found a recursive constant-call cycle.
    Cycle,
    /// The selected callable cannot execute in constant context.
    Ineligible(DiagnosticBag),
}

/// Resolves selected constant calls through the caller's demand-driven query graph.
pub trait ConstantCallResolver: Sync {
    /// Exact failures owned by the coordinating query layer.
    type UpstreamError;

    /// Returns whether the selected callable may execute in a constant context.
    fn is_constant_callable(
        &self,
        callable: CallableInstanceData,
    ) -> CheckerQueryResult<bool, Self::UpstreamError>;

    /// Evaluates one exact call without evaluating unrelated semantic queries.
    fn resolve(
        &self,
        request: &ConstantCallRequest,
    ) -> CheckerQueryResult<ConstantCallResolution, Self::UpstreamError>;
}

/// Resolves stable references used by an imported const-callable body template.
pub trait ConstantTemplateResolver: ConstantCallResolver {
    /// Resolves one stable declaration identity into the consuming compilation.
    fn symbol(&self, key: &SymbolKey) -> Option<AnySymbolId>;

    /// Evaluates one constant declaration retained by a checked body template.
    fn resolve_constant(
        &self,
        instance: ConstantInstanceKey,
        limits: ConstantEvaluationLimits,
    ) -> CheckerQueryResult<DiagnosticResult<ConstantReferenceResolution>, Self::UpstreamError>;

    /// Resolves one exact static reference retained by a checked initializer template.
    fn resolve_static(
        &self,
        declaration: bray_symbols::StaticSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> CheckerQueryResult<
        DiagnosticResult<bray_symbols::StaticReferenceSelection>,
        Self::UpstreamError,
    >;
}
