use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;

use crate::{
    AnySymbolId, CallableDependencyContracts, CallableExecution, DependencyContractTemplateId,
    LifecycleObligationKind,
};

use super::TrustedCapabilityRequirement;

/// Whether checked execution can enter cancellation propagation for its current run.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CurrentRunCancellation {
    /// No checked path enters current-run cancellation.
    NotEntered,
    /// At least one checked path can enter current-run cancellation.
    MayEnter,
}

macro_rules! define_callable_requirement {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(AnySymbolId);

        impl $name {
            /// Creates a requirement for an exact semantic declaration.
            pub const fn new(declaration: AnySymbolId) -> Self {
                Self(declaration)
            }

            /// Returns the exact declaration that defines the requirement.
            pub const fn declaration(self) -> AnySymbolId {
                self.0
            }
        }
    };
}

define_callable_requirement!(
    CallableEffectRequirement,
    "One checked effect produced while executing a callable contract phase."
);
define_callable_requirement!(
    CallableCapabilityRequirement,
    "One checked capability required while executing a callable contract phase."
);
define_callable_requirement!(
    CallableExecutionRequirement,
    "One checked execution-lane predicate required by a callable contract phase."
);

/// Complete checked behavior of one callable contract phase.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallablePhaseBehavior {
    effects: Arc<[CallableEffectRequirement]>,
    capabilities: Arc<[CallableCapabilityRequirement]>,
    trusted_capabilities: Arc<[TrustedCapabilityRequirement]>,
    execution_requirements: Arc<[CallableExecutionRequirement]>,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    dependency_contract: DependencyContractTemplateId,
    current_run_cancellation: CurrentRunCancellation,
}

impl CallablePhaseBehavior {
    /// Creates phase behavior as canonical semantic sets.
    pub fn new(
        effects: impl IntoIterator<Item = CallableEffectRequirement>,
        capabilities: impl IntoIterator<Item = CallableCapabilityRequirement>,
        trusted_capabilities: impl IntoIterator<Item = TrustedCapabilityRequirement>,
        execution_requirements: impl IntoIterator<Item = CallableExecutionRequirement>,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        dependency_contract: DependencyContractTemplateId,
        current_run_cancellation: CurrentRunCancellation,
    ) -> Self {
        Self {
            effects: sorted_unique_shared_slice(effects),
            capabilities: sorted_unique_shared_slice(capabilities),
            trusted_capabilities: sorted_unique_shared_slice(trusted_capabilities),
            execution_requirements: sorted_unique_shared_slice(execution_requirements),
            lifecycle_obligations: sorted_unique_shared_slice(lifecycle_obligations),
            dependency_contract,
            current_run_cancellation,
        }
    }

    /// Creates behavior with no requirements other than its dependency template.
    pub fn empty(dependency_contract: DependencyContractTemplateId) -> Self {
        Self::new(
            [],
            [],
            [],
            [],
            [],
            dependency_contract,
            CurrentRunCancellation::NotEntered,
        )
    }

    /// Returns checked effects in canonical semantic order.
    pub fn effects(&self) -> &[CallableEffectRequirement] {
        &self.effects
    }

    /// Returns checked capabilities in canonical semantic order.
    pub fn capabilities(&self) -> &[CallableCapabilityRequirement] {
        &self.capabilities
    }

    /// Returns trusted capability requirements in canonical semantic order.
    pub fn trusted_capabilities(&self) -> &[TrustedCapabilityRequirement] {
        &self.trusted_capabilities
    }

    /// Returns execution-lane requirements in canonical semantic order.
    pub fn execution_requirements(&self) -> &[CallableExecutionRequirement] {
        &self.execution_requirements
    }

    /// Returns whether every execution lane required here is also required by `other`.
    pub fn execution_requirements_are_subset_of(&self, other: &Self) -> bool {
        self.execution_requirements.iter().all(|requirement| {
            other
                .execution_requirements
                .binary_search(requirement)
                .is_ok()
        })
    }

    /// Returns lifecycle obligations in canonical semantic order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns the portable dependencies retained by this phase.
    pub const fn dependency_contract(&self) -> DependencyContractTemplateId {
        self.dependency_contract
    }

    /// Returns whether execution can enter cancellation for its current run.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }

    /// Returns this phase behavior with a replacement portable dependency template.
    pub fn with_dependency_contract(
        mut self,
        dependency_contract: DependencyContractTemplateId,
    ) -> Self {
        self.dependency_contract = dependency_contract;

        self
    }
}

/// Caller-visible behavior split across callable invocation and deferred execution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallablePhaseBehaviors {
    invocation: Arc<CallablePhaseBehavior>,
    deferred_execution: Option<Arc<CallablePhaseBehavior>>,
}

impl CallablePhaseBehaviors {
    /// Creates behavior for an immediately executing callable.
    pub fn synchronous(invocation: CallablePhaseBehavior) -> Self {
        Self {
            invocation: Arc::new(invocation),
            deferred_execution: None,
        }
    }

    /// Creates behavior for lazy async invocation and deferred body execution.
    pub fn asynchronous(
        invocation: CallablePhaseBehavior,
        deferred_execution: CallablePhaseBehavior,
    ) -> Self {
        Self {
            invocation: Arc::new(invocation),
            deferred_execution: Some(Arc::new(deferred_execution)),
        }
    }

    /// Creates empty phase behavior with the supplied dependency templates.
    pub fn empty(dependencies: CallableDependencyContracts) -> Self {
        let invocation = CallablePhaseBehavior::empty(dependencies.invocation());

        match dependencies.deferred_execution() {
            Some(deferred) => {
                Self::asynchronous(invocation, CallablePhaseBehavior::empty(deferred))
            }
            None => Self::synchronous(invocation),
        }
    }

    /// Returns behavior incurred while invoking the callable.
    pub fn invocation(&self) -> &CallablePhaseBehavior {
        &self.invocation
    }

    /// Returns behavior retained until deferred async execution.
    pub fn deferred_execution(&self) -> Option<&CallablePhaseBehavior> {
        self.deferred_execution.as_deref()
    }

    /// Returns the callable execution mode implied by the phase shape.
    pub const fn execution(&self) -> CallableExecution {
        match self.deferred_execution {
            Some(_) => CallableExecution::Asynchronous,
            None => CallableExecution::Synchronous,
        }
    }

    /// Returns portable dependency templates for both callable phases.
    pub fn dependency_contracts(&self) -> CallableDependencyContracts {
        match self.deferred_execution() {
            Some(deferred) => CallableDependencyContracts::asynchronous(
                self.invocation().dependency_contract(),
                deferred.dependency_contract(),
            ),
            None => {
                CallableDependencyContracts::synchronous(self.invocation().dependency_contract())
            }
        }
    }

    /// Replaces both phase dependency templates when the execution shape matches.
    pub fn try_with_dependency_contracts(
        &self,
        dependencies: CallableDependencyContracts,
    ) -> Option<Self> {
        // The updated value owns its Arc-backed phase snapshots independently of `self`.
        match (self.deferred_execution(), dependencies.deferred_execution()) {
            (Some(deferred), Some(dependency)) => Some(Self::asynchronous(
                self.invocation()
                    .clone()
                    .with_dependency_contract(dependencies.invocation()),
                deferred.clone().with_dependency_contract(dependency),
            )),
            (None, None) => Some(Self::synchronous(
                self.invocation()
                    .clone()
                    .with_dependency_contract(dependencies.invocation()),
            )),
            (Some(_), None) | (None, Some(_)) => None,
        }
    }
}
