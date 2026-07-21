use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{
    BoundArgument, BoundExpressionId, BoundGenericArgument, BoundResolvedCall, MemberTarget,
};
use bray_symbols::{
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, CallableSignature,
    ImplementationRequirementKey, ImplementationSelection, SymbolKey,
};

/// Receiver authority relevant to method candidate applicability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReceiverCapability {
    /// Shared observation only.
    Shared,
    /// Exclusive mutation without ownership transfer.
    Mutable,
    /// Ownership without mutable local authority.
    Owned,
    /// Ownership with mutable local authority.
    OwnedMutable,
}

/// The receiver occurrence and authority supplied by method-call syntax.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReceiverSelection {
    expression: BoundExpressionId,
    capability: ReceiverCapability,
}

impl ReceiverSelection {
    /// Creates a method receiver selection input.
    pub const fn new(expression: BoundExpressionId, capability: ReceiverCapability) -> Self {
        Self {
            expression,
            capability,
        }
    }

    /// Returns the receiver expression occurrence.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the receiver authority available to candidate selection.
    pub const fn capability(self) -> ReceiverCapability {
        self.capability
    }
}

/// Non-type participation state for one callable candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableCandidateState {
    /// The candidate is visible, target-available, and satisfies static constraints.
    Available,
    /// The candidate exists but is not visible from the requesting context.
    Inaccessible,
    /// Target availability or static constraints exclude the candidate.
    Unavailable,
    /// The candidate signature or declaration contains prior recovery.
    Recovered,
}

/// One typed implementation-selection fact supplied with a candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationSelectionEvidence {
    requirement: ImplementationRequirementKey,
    selection: ImplementationSelection,
}

impl ImplementationSelectionEvidence {
    /// Creates evidence for one exact implementation requirement.
    pub const fn new(
        requirement: ImplementationRequirementKey,
        selection: ImplementationSelection,
    ) -> Self {
        Self {
            requirement,
            selection,
        }
    }

    /// Returns the exact implementation requirement.
    pub const fn requirement(&self) -> ImplementationRequirementKey {
        self.requirement
    }

    /// Returns the typed implementation-selection result.
    pub const fn selection(&self) -> &ImplementationSelection {
        &self.selection
    }
}

/// One callable target considered by exact argument and receiver applicability checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableCandidate {
    key: SymbolKey,
    resolution: BoundResolvedCall,
    signature: CallableSignature,
    defaults: Arc<
        [(
            CallableParameterSymbolId,
            CallableParameterDefaultProviderSymbolId,
        )],
    >,
    implementation_selections: Arc<[ImplementationSelectionEvidence]>,
    state: CallableCandidateState,
}

impl CallableCandidate {
    /// Creates a callable candidate and its available runtime defaults.
    pub fn new(
        key: SymbolKey,
        resolution: BoundResolvedCall,
        signature: CallableSignature,
        defaults: impl IntoIterator<
            Item = (
                CallableParameterSymbolId,
                CallableParameterDefaultProviderSymbolId,
            ),
        >,
        state: CallableCandidateState,
    ) -> Self {
        let mut defaults = defaults.into_iter().collect::<Vec<_>>();

        defaults.sort_unstable_by_key(|(parameter, _)| *parameter);

        Self {
            key,
            resolution,
            signature,
            defaults: defaults.into(),
            implementation_selections: Arc::new([]),
            state,
        }
    }

    /// Supplies typed implementation-selection facts used by the callable target.
    pub fn with_implementation_selections(
        mut self,
        selections: impl IntoIterator<Item = ImplementationSelectionEvidence>,
    ) -> Self {
        self.implementation_selections = shared_slice(selections);

        self
    }

    /// Returns the stable semantic key used for deterministic ordering.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the selected target and result this candidate would commit.
    pub const fn resolution(&self) -> &BoundResolvedCall {
        &self.resolution
    }

    /// Returns the checked callable signature.
    pub const fn signature(&self) -> &CallableSignature {
        &self.signature
    }

    /// Returns available parameter defaults in parameter-ID order.
    pub fn defaults(
        &self,
    ) -> &[(
        CallableParameterSymbolId,
        CallableParameterDefaultProviderSymbolId,
    )] {
        &self.defaults
    }

    /// Returns this candidate's non-type participation state.
    pub const fn state(&self) -> CallableCandidateState {
        self.state
    }

    pub(in crate::selection) fn implementation_selections(
        &self,
    ) -> &[ImplementationSelectionEvidence] {
        &self.implementation_selections
    }
}

/// Inputs for selecting one exact callable and normalizing its argument mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableSelectionRequest {
    pub(in crate::selection) expression: BoundExpressionId,
    pub(in crate::selection) callee_member: Option<MemberTarget>,
    pub(in crate::selection) receiver: Option<ReceiverSelection>,
    pub(in crate::selection) generic_arguments: Arc<[BoundGenericArgument]>,
    pub(in crate::selection) arguments: Arc<[BoundArgument]>,
    pub(in crate::selection) candidates: Vec<CallableCandidate>,
}

impl CallableSelectionRequest {
    /// Creates a callable selection request from binder-enumerated candidates.
    pub fn new(
        expression: BoundExpressionId,
        callee_member: Option<MemberTarget>,
        receiver: Option<ReceiverSelection>,
        generic_arguments: impl IntoIterator<Item = BoundGenericArgument>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        candidates: impl IntoIterator<Item = CallableCandidate>,
    ) -> Self {
        Self {
            expression,
            callee_member,
            receiver,
            generic_arguments: shared_slice(generic_arguments),
            arguments: shared_slice(arguments),
            candidates: candidates.into_iter().collect(),
        }
    }

    /// Returns the call expression occurrence that owns this selection.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the exact selected member when the callee is a member access.
    pub const fn callee_member(&self) -> Option<&MemberTarget> {
        self.callee_member.as_ref()
    }

    /// Returns the method receiver when the call has one.
    pub const fn receiver(&self) -> Option<ReceiverSelection> {
        self.receiver
    }

    /// Returns explicit generic arguments in source order.
    pub fn generic_arguments(&self) -> &[BoundGenericArgument] {
        &self.generic_arguments
    }

    /// Returns explicit source arguments in evaluation order.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }
}
