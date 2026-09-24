use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::{
    CallableCapabilityRequirement, CallableEffectRequirement, CallableExecutionRequirement,
    CurrentRunCancellation, LifecycleObligationKind, StaticSymbolId, TrustedCapabilitySymbolId,
};

use crate::{
    BoundCallableTarget, BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKind,
    DefaultValueProvider,
};

/// Whether one selected callable contributes invocation or deferred body behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BodyBehaviorPhase {
    /// Behavior evaluated while invoking the callable.
    Invocation,
    /// Behavior evaluated when a lazy callable body is driven.
    DeferredExecution,
}

/// One selected callable whose checked behavior contributes to a body summary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BodyBehaviorCall {
    target: BoundCallableTarget,
    phase: BodyBehaviorPhase,
    source: Option<BoundSourceAnchor>,
    anonymous_unit: Option<BoundUnitKey>,
}

impl BodyBehaviorCall {
    /// Creates one exact callable-phase contribution.
    pub const fn new(target: BoundCallableTarget, phase: BodyBehaviorPhase) -> Self {
        Self {
            target,
            phase,
            source: None,
            anonymous_unit: None,
        }
    }

    /// Retains the source expression that selected this callable contribution.
    pub const fn with_source(mut self, source: BoundSourceAnchor) -> Self {
        self.source = Some(source);

        self
    }

    /// Retains the separately bound body selected for an anonymous callable.
    pub fn with_anonymous_unit(mut self, unit: BoundUnitKey) -> Self {
        self.anonymous_unit = Some(unit);

        self
    }

    /// Returns the selected callable target.
    pub const fn target(&self) -> BoundCallableTarget {
        self.target
    }

    /// Returns the evaluated callable phase.
    pub const fn phase(&self) -> BodyBehaviorPhase {
        self.phase
    }

    /// Returns the source expression that selected this contribution, when source-backed.
    pub const fn source(&self) -> Option<BoundSourceAnchor> {
        self.source
    }

    /// Returns the selected anonymous body when the target is body-local.
    pub const fn anonymous_unit(&self) -> Option<&BoundUnitKey> {
        self.anonymous_unit.as_ref()
    }
}

/// Direct semantic contributions discovered in one checked bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BodyBehaviorContributions {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    calls: Arc<[BodyBehaviorCall]>,
    defaults: Arc<[DefaultValueProvider]>,
    static_dependencies: Arc<[StaticSymbolId]>,
    current_run_cancellation: CurrentRunCancellation,
    is_recovered: bool,
}

/// One trusted capability reached by source-backed calls in a checked body.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TrustedCapabilityUse {
    capability: TrustedCapabilitySymbolId,
    sources: Arc<[BoundSourceAnchor]>,
}

impl TrustedCapabilityUse {
    /// Creates one use summary with canonical, duplicate-free source origins.
    pub fn new(
        capability: TrustedCapabilitySymbolId,
        sources: impl IntoIterator<Item = BoundSourceAnchor>,
    ) -> Self {
        Self {
            capability,
            sources: sorted_unique_shared_slice(sources),
        }
    }

    /// Returns the trusted capability required by the selected calls.
    pub const fn capability(&self) -> TrustedCapabilitySymbolId {
        self.capability
    }

    /// Returns every retained source call that requires this capability.
    pub fn sources(&self) -> &[BoundSourceAnchor] {
        &self.sources
    }
}

impl BodyBehaviorContributions {
    /// Creates direct contributions in deterministic semantic order.
    pub fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        calls: impl IntoIterator<Item = BodyBehaviorCall>,
        defaults: impl IntoIterator<Item = DefaultValueProvider>,
        static_dependencies: impl IntoIterator<Item = StaticSymbolId>,
        current_run_cancellation: CurrentRunCancellation,
        is_recovered: bool,
    ) -> Self {
        Self {
            unit,
            kind,
            calls: shared_slice(calls),
            defaults: shared_slice(defaults),
            static_dependencies: sorted_unique_shared_slice(static_dependencies),
            current_run_cancellation,
            is_recovered,
        }
    }

    /// Returns the checked semantic unit described by these contributions.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the checked unit category.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns selected callable phases in deterministic evaluation order.
    pub fn calls(&self) -> &[BodyBehaviorCall] {
        &self.calls
    }

    /// Returns evaluated runtime defaults in deterministic evaluation order.
    pub fn defaults(&self) -> &[DefaultValueProvider] {
        &self.defaults
    }

    /// Returns static declarations selected directly in this unit.
    pub fn static_dependencies(&self) -> &[StaticSymbolId] {
        &self.static_dependencies
    }

    /// Returns whether the current run can enter cancellation directly in this unit.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }

    /// Returns whether recovery prevented complete contribution discovery.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// Normalized effects and obligations of one checked semantic body.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedBodyBehavior {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    effects: Arc<[CallableEffectRequirement]>,
    capabilities: Arc<[CallableCapabilityRequirement]>,
    trusted_capabilities: Arc<[TrustedCapabilitySymbolId]>,
    trusted_capability_uses: Arc<[TrustedCapabilityUse]>,
    execution_requirements: Arc<[CallableExecutionRequirement]>,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    static_dependencies: Arc<[StaticSymbolId]>,
    current_run_cancellation: CurrentRunCancellation,
    is_recovered: bool,
}

impl CheckedBodyBehavior {
    /// Creates a checked body summary without declaration-backed requirements.
    pub fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        current_run_cancellation: CurrentRunCancellation,
        is_recovered: bool,
    ) -> Self {
        Self {
            unit,
            kind,
            effects: Arc::from([]),
            capabilities: Arc::from([]),
            trusted_capabilities: Arc::from([]),
            trusted_capability_uses: Arc::from([]),
            execution_requirements: Arc::from([]),
            lifecycle_obligations: Arc::from([]),
            static_dependencies: Arc::from([]),
            current_run_cancellation,
            is_recovered,
        }
    }

    /// Returns checked behavior with normalized declaration-backed requirements.
    pub fn with_requirements(
        mut self,
        effects: impl IntoIterator<Item = CallableEffectRequirement>,
        capabilities: impl IntoIterator<Item = CallableCapabilityRequirement>,
        trusted_capabilities: impl IntoIterator<Item = TrustedCapabilitySymbolId>,
        execution_requirements: impl IntoIterator<Item = CallableExecutionRequirement>,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
    ) -> Self {
        self.effects = sorted_unique_shared_slice(effects);
        self.capabilities = sorted_unique_shared_slice(capabilities);
        self.trusted_capabilities = sorted_unique_shared_slice(trusted_capabilities);
        self.execution_requirements = sorted_unique_shared_slice(execution_requirements);
        self.lifecycle_obligations = sorted_unique_shared_slice(lifecycle_obligations);

        self
    }

    /// Returns checked behavior with source origins for direct trusted-capability use.
    pub fn with_trusted_capability_uses(
        mut self,
        uses: impl IntoIterator<Item = TrustedCapabilityUse>,
    ) -> Self {
        self.trusted_capability_uses = uses.into_iter().collect();

        self
    }

    /// Retains static storage needed by this body and its reachable calls.
    pub fn with_static_dependencies(
        mut self,
        dependencies: impl IntoIterator<Item = StaticSymbolId>,
    ) -> Self {
        self.static_dependencies = sorted_unique_shared_slice(dependencies);

        self
    }

    /// Returns the checked semantic unit described by this summary.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the checked unit category.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns accumulated effects in canonical semantic order.
    pub fn effects(&self) -> &[CallableEffectRequirement] {
        &self.effects
    }

    /// Returns accumulated capability requirements in canonical semantic order.
    pub fn capabilities(&self) -> &[CallableCapabilityRequirement] {
        &self.capabilities
    }

    /// Returns trusted implementation capabilities in canonical semantic order.
    pub fn trusted_capabilities(&self) -> &[TrustedCapabilitySymbolId] {
        &self.trusted_capabilities
    }

    /// Returns source-correlated trusted-capability uses in capability order.
    pub fn trusted_capability_uses(&self) -> &[TrustedCapabilityUse] {
        &self.trusted_capability_uses
    }

    /// Returns execution-context requirements in canonical semantic order.
    pub fn execution_requirements(&self) -> &[CallableExecutionRequirement] {
        &self.execution_requirements
    }

    /// Returns retained lifecycle obligations in canonical semantic order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns every static declaration needed while this body executes.
    pub fn static_dependencies(&self) -> &[StaticSymbolId] {
        &self.static_dependencies
    }

    /// Returns whether execution can enter cancellation for its current run.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }

    /// Returns whether recovery prevented a complete summary.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        AnySymbolId, CallableCapabilityRequirement, CallableEffectRequirement,
        CallableExecutionRequirement, CurrentRunCancellation, FunctionSymbolId,
        LifecycleObligationKind, SymbolId, TrustedCapabilitySymbolId,
    };

    use super::CheckedBodyBehavior;
    use crate::{BoundUnitId, BoundUnitKind};

    #[test]
    fn checked_body_behavior_normalizes_semantic_sets() {
        let declaration = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(4)));
        let trusted_capability = TrustedCapabilitySymbolId::from_symbol_id(SymbolId::new(5));

        let behavior = CheckedBodyBehavior::new(
            BoundUnitId::new(2),
            BoundUnitKind::CallableBody,
            CurrentRunCancellation::MayEnter,
            false,
        )
        .with_requirements(
            [
                CallableEffectRequirement::new(declaration),
                CallableEffectRequirement::new(declaration),
            ],
            [
                CallableCapabilityRequirement::new(declaration),
                CallableCapabilityRequirement::new(declaration),
            ],
            [trusted_capability, trusted_capability],
            [
                CallableExecutionRequirement::new(declaration),
                CallableExecutionRequirement::new(declaration),
            ],
            [
                LifecycleObligationKind::Joining,
                LifecycleObligationKind::Joining,
            ],
        );

        assert_eq!(behavior.effects().len(), 1);
        assert_eq!(behavior.capabilities().len(), 1);
        assert_eq!(behavior.trusted_capabilities().len(), 1);
        assert_eq!(behavior.execution_requirements().len(), 1);
        assert_eq!(behavior.lifecycle_obligations().len(), 1);

        assert_eq!(
            behavior.current_run_cancellation(),
            CurrentRunCancellation::MayEnter
        );
    }
}
