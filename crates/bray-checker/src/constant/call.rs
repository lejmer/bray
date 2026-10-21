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

/// The checked implementation identity of a constant invocation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantCallTarget {
    /// A named callable with its exact generic application.
    Declaration(CallableInstanceData),
    /// A capture-free anonymous callable in its enclosing generic application.
    Anonymous(bray_bound_tree::BoundUnitKey, Option<bray_symbols::GenericSubstitutionId>),
    /// A declaration-owned default provider in its selected generic application.
    Default(bray_bound_tree::DefaultValueProvider, bray_symbols::GenericSubstitutionId),
    /// A predicate body evaluated internally while checking a constant invocation.
    Predicate(bray_symbols::PredicateInstanceData),
}

/// The exact logical signature and caller requirements of a constant invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstantCallContract {
    inputs: Arc<[TypeId]>,
    result: TypeId,
    requirements: Arc<[bray_symbols::PredicateSemanticSummary]>,
}

impl ConstantCallContract {
    /// Retains the selected signature after substitution and checked contract construction.
    pub fn new(inputs: impl IntoIterator<Item = TypeId>, result: TypeId,
        requirements: impl IntoIterator<Item = bray_symbols::PredicateSemanticSummary>) -> Self {
        Self { inputs: shared_slice(inputs), result, requirements: shared_slice(requirements) }
    }

    /// Returns receiver-first logical input types.
    pub fn inputs(&self) -> &[TypeId] { &self.inputs }

    /// Returns the selected result type.
    pub const fn result(&self) -> TypeId { self.result }

    /// Returns requirements in their declaration order, retaining their checking domain.
    pub fn requirements(&self) -> &[bray_symbols::PredicateSemanticSummary] { &self.requirements }
}

/// One selected constant call after its argument expressions have evaluated.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantCallRequest {
    target: ConstantCallTarget,
    selected_implementation: Option<ImplementationInstanceId>,
    arguments: Arc<[ConstantValueId]>,
    result_type: TypeId,
    limits: ConstantEvaluationLimits,
}

impl ConstantCallRequest {
    /// Creates a request for one exact selected callable and closed argument list.
    pub fn new(
        target: ConstantCallTarget,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: impl IntoIterator<Item = ConstantValueId>,
        result_type: TypeId,
        limits: ConstantEvaluationLimits,
    ) -> Self {
        Self {
            target,
            selected_implementation,
            arguments: shared_slice(arguments),
            result_type,
            limits,
        }
    }

    /// Returns the exact substituted callable selected for evaluation.
    pub const fn callable(&self) -> CallableInstanceData {
        match &self.target {
            ConstantCallTarget::Declaration(callable) => *callable,
            _ => panic!("named constant-call query requires a declaration target"),
        }
    }

    /// Returns the exact implementation identity selected for this invocation.
    pub const fn target(&self) -> &ConstantCallTarget { &self.target }

    /// Returns the selected generic application for every invocation kind.
    pub const fn substitution(&self) -> bray_symbols::GenericSubstitutionId {
        match &self.target {
            ConstantCallTarget::Declaration(callable) => callable.substitution(),
            ConstantCallTarget::Predicate(predicate) => predicate.substitution(),
            ConstantCallTarget::Anonymous(_, Some(substitution)) | ConstantCallTarget::Default(_, substitution) => *substitution,
            ConstantCallTarget::Anonymous(_, None) => panic!("anonymous invocation resolves its lexical generic context before template evaluation"),
        }
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

    /// Reads the already selected logical signature without evaluating a body.
    fn contract(&self, target: &ConstantCallTarget) -> CheckerQueryResult<DiagnosticResult<Option<ConstantCallContract>>, Self::UpstreamError>;

    /// Retrieves source context only after a particular requirement has failed.
    fn requirement_origin(&self, target: &ConstantCallTarget, index: usize) -> CheckerQueryResult<Option<bray_source::SourceSpan>, Self::UpstreamError>;

    /// Evaluates one exact call without evaluating unrelated semantic queries.
    fn resolve(
        &self,
        request: &ConstantCallRequest,
    ) -> CheckerQueryResult<ConstantCallResolution, Self::UpstreamError>;
}

/// Resolves stable references used by an imported const-callable body template.
pub trait ConstantTemplateResolver: ConstantCallResolver {
    /// Returns the complete checked expression graph of one ordinary caller requirement.
    fn requirement_template(&self, target: &ConstantCallTarget, index: usize)
        -> CheckerQueryResult<DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>, Self::UpstreamError>;

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
