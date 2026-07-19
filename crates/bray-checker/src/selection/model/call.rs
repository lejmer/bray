use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{BoundArgument, BoundCallableTarget, BoundExpressionId, BoundResolvedCall};
use bray_symbols::{
    CallableAbi, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    CallableSignature, SymbolKey,
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

/// Whether a call selects one direct target or one explicit overload arm.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableSelectionMode {
    /// A direct call can fill omitted defaulted parameters after target selection.
    Direct,
    /// An overload arm is applicable only when every parameter is supplied explicitly.
    Overload,
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
    state: CallableCandidateState,
}

pub(in crate::selection) struct CallableCandidateParts {
    pub key: SymbolKey,
    pub resolution: BoundResolvedCall,
    pub signature: CallableSignature,
    pub defaults: Arc<
        [(
            CallableParameterSymbolId,
            CallableParameterDefaultProviderSymbolId,
        )],
    >,
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
            state,
        }
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

    pub(in crate::selection) fn into_parts(self) -> CallableCandidateParts {
        CallableCandidateParts {
            key: self.key,
            resolution: self.resolution,
            signature: self.signature,
            defaults: self.defaults,
        }
    }
}

/// One explicit or defaulted value in call evaluation order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectedArgument {
    /// A source argument mapped to its exact parameter.
    Explicit {
        /// The argument expression occurrence.
        expression: BoundExpressionId,
        /// The exact selected parameter.
        parameter: CallableParameterSymbolId,
    },
    /// An omitted parameter supplied by its declaration-owned default provider.
    Default {
        /// The exact omitted parameter.
        parameter: CallableParameterSymbolId,
        /// The declaration-owned default provider evaluated by the call.
        provider: CallableParameterDefaultProviderSymbolId,
    },
}

/// One exact callable, ABI, implementation-witness set, and normalized argument mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedCall {
    resolution: BoundResolvedCall,
    abi: CallableAbi,
    arguments: Arc<[SelectedArgument]>,
}

impl SelectedCall {
    pub(in crate::selection) fn new(
        resolution: BoundResolvedCall,
        abi: CallableAbi,
        arguments: impl IntoIterator<Item = SelectedArgument>,
    ) -> Self {
        Self {
            resolution,
            abi,
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the exact selected callable target and result behavior.
    pub const fn resolution(&self) -> &BoundResolvedCall {
        &self.resolution
    }

    /// Returns the callable ABI participating in the selected call contract.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns explicit arguments in source order followed by defaults in parameter order.
    pub fn arguments(&self) -> &[SelectedArgument] {
        &self.arguments
    }

    /// Returns the exact declared, anonymous, or indirect call target.
    pub const fn target(&self) -> BoundCallableTarget {
        self.resolution.target()
    }
}

/// Inputs for selecting one exact callable and normalizing its argument mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableSelectionRequest {
    pub(in crate::selection) expression: BoundExpressionId,
    pub(in crate::selection) mode: CallableSelectionMode,
    pub(in crate::selection) receiver: Option<ReceiverSelection>,
    pub(in crate::selection) arguments: Arc<[BoundArgument]>,
    pub(in crate::selection) candidates: Vec<CallableCandidate>,
}

impl CallableSelectionRequest {
    /// Creates a callable selection request from binder-enumerated candidates.
    pub fn new(
        expression: BoundExpressionId,
        mode: CallableSelectionMode,
        receiver: Option<ReceiverSelection>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        candidates: impl IntoIterator<Item = CallableCandidate>,
    ) -> Self {
        Self {
            expression,
            mode,
            receiver,
            arguments: shared_slice(arguments),
            candidates: candidates.into_iter().collect(),
        }
    }

    /// Returns the call expression occurrence that owns this selection.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns whether the request selects a direct target or an overload arm.
    pub const fn mode(&self) -> CallableSelectionMode {
        self.mode
    }

    /// Returns the method receiver when the call has one.
    pub const fn receiver(&self) -> Option<ReceiverSelection> {
        self.receiver
    }

    /// Returns explicit source arguments in evaluation order.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }
}
