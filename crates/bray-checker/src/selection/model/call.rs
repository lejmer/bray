use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{BoundArgument, BoundExpressionId, BoundResolvedCall, MemberTarget};
use bray_symbols::{
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, CallableSignature,
    ImplementationSelection, ImplementationSelectionKey, SemanticValueStore, SymbolKey, TypeData,
};

use crate::{CheckerInfrastructureError, ExpressionTypeExpectation};

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
    requirement: ImplementationSelectionKey,
    selection: ImplementationSelection,
}

impl ImplementationSelectionEvidence {
    /// Creates evidence for one exact implementation requirement.
    pub const fn new(
        requirement: ImplementationSelectionKey,
        selection: ImplementationSelection,
    ) -> Self {
        Self {
            requirement,
            selection,
        }
    }

    /// Returns the exact implementation requirement.
    pub const fn requirement(&self) -> ImplementationSelectionKey {
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
    pub implementation_selections: Arc<[ImplementationSelectionEvidence]>,
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

    pub(in crate::selection) fn into_parts(self) -> CallableCandidateParts {
        CallableCandidateParts {
            key: self.key,
            resolution: self.resolution,
            signature: self.signature,
            defaults: self.defaults,
            implementation_selections: self.implementation_selections,
        }
    }
}

/// Inputs for selecting one exact callable and normalizing its argument mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableSelectionRequest {
    pub(in crate::selection) expression: BoundExpressionId,
    pub(in crate::selection) callee_member: Option<MemberTarget>,
    pub(in crate::selection) receiver: Option<ReceiverSelection>,
    pub(in crate::selection) arguments: Arc<[BoundArgument]>,
    pub(in crate::selection) candidates: Vec<CallableCandidate>,
}

impl CallableSelectionRequest {
    /// Creates a callable selection request from binder-enumerated candidates.
    pub fn new(
        expression: BoundExpressionId,
        callee_member: Option<MemberTarget>,
        receiver: Option<ReceiverSelection>,
        arguments: impl IntoIterator<Item = BoundArgument>,
        candidates: impl IntoIterator<Item = CallableCandidate>,
    ) -> Self {
        Self {
            expression,
            callee_member,
            receiver,
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

    /// Returns explicit source arguments in evaluation order.
    pub fn arguments(&self) -> &[BoundArgument] {
        &self.arguments
    }

    /// Returns candidate-agreed expected types for explicit arguments.
    ///
    /// An argument receives context only when every participating candidate maps it to the same
    /// parameter type. Candidate result types never contribute expected context.
    pub fn contextual_type_expectations(
        &self,
        values: &SemanticValueStore,
    ) -> Result<Vec<ExpressionTypeExpectation>, CheckerInfrastructureError> {
        let participating = self
            .candidates
            .iter()
            .filter(|candidate| candidate.state == CallableCandidateState::Available)
            .collect::<Vec<_>>();

        let Some(first) = participating.first() else {
            return Ok(Vec::new());
        };

        let mut agreed = candidate_argument_types(values, &self.arguments, first)?;

        for candidate in participating.into_iter().skip(1) {
            let current = candidate_argument_types(values, &self.arguments, candidate)?;

            for (agreed, current) in agreed.iter_mut().zip(current) {
                if *agreed != current {
                    *agreed = None;
                }
            }
        }

        Ok(self
            .arguments
            .iter()
            .zip(agreed)
            .filter_map(|(argument, ty)| {
                ty.map(|ty| ExpressionTypeExpectation::new(argument.expression(), ty))
            })
            .collect())
    }
}

fn candidate_argument_types(
    values: &SemanticValueStore,
    arguments: &[BoundArgument],
    candidate: &CallableCandidate,
) -> Result<Vec<Option<bray_symbols::TypeId>>, CheckerInfrastructureError> {
    let callable = values
        .type_data(candidate.signature.callable_type())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(callable) = callable.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(indices) = map_explicit_argument_indices(arguments, callable.parameters()) else {
        return Ok(vec![None; arguments.len()]);
    };

    Ok(indices
        .into_iter()
        .map(|index| {
            callable
                .parameters()
                .get(index)
                .map(|parameter| parameter.ty())
        })
        .collect())
}

pub(in crate::selection) fn map_explicit_argument_indices(
    arguments: &[BoundArgument],
    parameters: &[bray_symbols::CallableParameterData],
) -> Option<Vec<usize>> {
    let mut supplied = vec![false; parameters.len()];
    let mut indices = Vec::with_capacity(arguments.len());
    let mut positional_index = 0;
    let mut saw_named = false;

    for argument in arguments {
        let parameter_index = match argument.name() {
            Some(name) => {
                saw_named = true;

                parameters
                    .iter()
                    .position(|parameter| parameter.name().as_str() == name.as_str())
            }
            None if saw_named => return None,
            None => {
                let index = positional_index;
                positional_index += 1;

                parameters.get(index).and_then(|parameter| {
                    (parameter.position() == bray_symbols::CallablePosition::PositionalOrNamed)
                        .then_some(index)
                })
            }
        }?;

        if std::mem::replace(supplied.get_mut(parameter_index)?, true) {
            return None;
        }

        indices.push(parameter_index);
    }

    Some(indices)
}
