use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{
    BoundArgument, BoundExpressionId, BoundGenericArgument, BoundResolvedCall,
    DeclaredValueTypeTerm, MemberTarget,
};
use bray_symbols::{
    CallableContractTemplate, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    CallableSignature, GenericConstraintTemplate, GenericSubstitutionId,
    ImplementationRequirementKey, ImplementationSelection, TypeId,
};

use super::SelectionCandidateKey;

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
    key: SelectionCandidateKey,
    resolution: BoundResolvedCall,
    callable_type: TypeId,
    declaration_signature: Option<CallableSignature>,
    contract: Option<Arc<CallableContractTemplate>>,
    result: TypeId,
    defaults: Arc<
        [(
            CallableParameterSymbolId,
            CallableParameterDefaultProviderSymbolId,
        )],
    >,
    generic_constraints: Arc<[GenericConstraintTemplate]>,
    generic_substitution: Option<GenericSubstitutionId>,
    implementation_selections: Arc<[ImplementationSelectionEvidence]>,
    state: CallableCandidateState,
}

impl CallableCandidate {
    /// Creates a callable candidate and its available runtime defaults.
    pub fn new(
        key: impl Into<SelectionCandidateKey>,
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

        let substitution = match resolution.target() {
            bray_bound_tree::BoundCallableTarget::Declaration(instance) => {
                Some(instance.substitution())
            }
            _ => None,
        };

        defaults.sort_unstable_by_key(|(parameter, _)| *parameter);

        Self {
            key: key.into(),
            resolution,
            callable_type: signature.callable_type(),
            result: signature.result(),
            declaration_signature: Some(signature),
            contract: None,
            defaults: defaults.into(),
            generic_constraints: Arc::new([]),
            generic_substitution: substitution,
            implementation_selections: Arc::new([]),
            state,
        }
    }

    /// Creates a candidate from a value whose converged type is callable.
    pub(crate) fn value(
        key: DeclaredValueTypeTerm,
        resolution: BoundResolvedCall,
        callable_type: TypeId,
        result: TypeId,
        state: CallableCandidateState,
    ) -> Self {
        Self {
            key: SelectionCandidateKey::Value(key),
            resolution,
            callable_type,
            declaration_signature: None,
            contract: None,
            result,
            defaults: Arc::new([]),
            generic_constraints: Arc::new([]),
            generic_substitution: None,
            implementation_selections: Arc::new([]),
            state,
        }
    }

    /// Retains the declaration facts available for a directly selected callable member.
    pub(crate) fn with_declaration(
        mut self,
        signature: CallableSignature,
        defaults: impl IntoIterator<
            Item = (
                CallableParameterSymbolId,
                CallableParameterDefaultProviderSymbolId,
            ),
        >,
    ) -> Self {
        let substitution = match self.resolution.target() {
            bray_bound_tree::BoundCallableTarget::Declaration(instance) => instance.substitution(),
            _ => unreachable!("declaration facts require a declaration target"),
        };

        let mut defaults = defaults.into_iter().collect::<Vec<_>>();
        defaults.sort_unstable_by_key(|(parameter, _)| *parameter);

        self.declaration_signature = Some(signature);
        self.defaults = defaults.into();
        self.generic_substitution = Some(substitution);

        self
    }

    /// Creates a compile-time predicate candidate with a concrete callable surface.
    pub(crate) fn predicate(
        key: impl Into<SelectionCandidateKey>,
        resolution: BoundResolvedCall,
        callable_type: TypeId,
        result: TypeId,
        substitution: GenericSubstitutionId,
        state: CallableCandidateState,
    ) -> Self {
        Self {
            key: key.into(),
            resolution,
            callable_type,
            declaration_signature: None,
            contract: None,
            result,
            defaults: Arc::new([]),
            generic_constraints: Arc::new([]),
            generic_substitution: Some(substitution),
            implementation_selections: Arc::new([]),
            state,
        }
    }

    /// Supplies declaration constraints retained with this candidate's substitution.
    pub(crate) fn with_generic_constraints(
        mut self,
        constraints: impl IntoIterator<Item = GenericConstraintTemplate>,
    ) -> Self {
        self.generic_constraints = shared_slice(constraints);

        self
    }

    pub(crate) const fn with_generic_substitution(
        mut self,
        substitution: GenericSubstitutionId,
    ) -> Self {
        self.generic_substitution = Some(substitution);

        self
    }

    /// Supplies source or imported contract clauses for a declaration candidate.
    pub(crate) fn with_contract(mut self, contract: CallableContractTemplate) -> Self {
        self.contract = Some(Arc::new(contract));

        self
    }

    /// Supplies typed implementation-selection facts used by the callable target.
    pub fn with_implementation_selections(
        mut self,
        selections: impl IntoIterator<Item = ImplementationSelectionEvidence>,
    ) -> Self {
        let mut selections = selections.into_iter().collect::<Vec<_>>();

        let witnesses = self
            .resolution
            .implementation_witnesses()
            .iter()
            .copied()
            .chain(
                selections
                    .iter()
                    .filter_map(|selection| match selection.selection() {
                        ImplementationSelection::Selected(witness) => Some(*witness),
                        ImplementationSelection::Deferred
                        | ImplementationSelection::Unavailable
                        | ImplementationSelection::Ambiguous(_) => None,
                    }),
            )
            .collect::<Vec<_>>();

        selections.sort_unstable_by_key(ImplementationSelectionEvidence::requirement);

        self.resolution = self
            .resolution
            .clone()
            .with_implementation_witnesses(witnesses);

        self.implementation_selections = selections.into();

        self
    }

    /// Returns the stable semantic key used for deterministic ordering.
    pub const fn key(&self) -> &SelectionCandidateKey {
        &self.key
    }

    /// Returns the selected target and result this candidate would commit.
    pub const fn resolution(&self) -> &BoundResolvedCall {
        &self.resolution
    }

    /// Returns the canonical callable type.
    pub const fn callable_type(&self) -> TypeId {
        self.callable_type
    }

    /// Returns the declaration signature when the candidate names a declaration.
    pub(crate) const fn declaration_signature(&self) -> Option<&CallableSignature> {
        self.declaration_signature.as_ref()
    }

    pub(crate) fn contract(&self) -> Option<&CallableContractTemplate> {
        match &self.contract {
            Some(contract) => Some(contract.as_ref()),
            None => None,
        }
    }

    /// Returns the callable's checked result type before asynchronous wrapping.
    pub const fn result(&self) -> TypeId {
        self.result
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

    pub(crate) fn generic_constraints(&self) -> &[GenericConstraintTemplate] {
        &self.generic_constraints
    }

    pub(crate) const fn generic_substitution(&self) -> Option<GenericSubstitutionId> {
        self.generic_substitution
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
        let candidates = Self::canonical_candidates(candidates);

        Self {
            expression,
            callee_member,
            receiver,
            generic_arguments: shared_slice(generic_arguments),
            arguments: shared_slice(arguments),
            candidates,
        }
    }

    pub(crate) fn canonical_candidates(
        candidates: impl IntoIterator<Item = CallableCandidate>,
    ) -> Vec<CallableCandidate> {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();

        super::super::order::sort_by_key(&mut candidates, CallableCandidate::key);

        candidates
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
