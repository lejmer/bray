use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_symbols::{CallableExecutionGuarantee, SymbolOrdinal};

use crate::{AnyBoundNodeId, BoundBlockId, BoundExpressionId, BoundUnitId, StorageAccessId};

/// The selected source or implicit operation that supplies implementation evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableProofTarget {
    /// A source call whose checked selection identifies its implementation.
    Call(BoundExpressionId),
    /// An implicitly selected callable at the retained source operation.
    Implicit {
        /// Source operation requiring the implicit execution.
        site: AnyBoundNodeId,
        /// Exact selected generic implementation.
        callable: bray_symbols::CallableInstanceData,
    },
}

impl CallableProofTarget {
    /// Resolves the selected callable definition from checked source selections.
    pub fn definition(
        self,
        selections: &crate::CheckedSemanticSelections,
    ) -> Option<bray_symbols::CallableDefinitionId> {
        match self {
            Self::Implicit { callable, .. } => Some(callable.definition()),
            Self::Call(expression) => match selections.expression(expression) {
                Some(crate::SemanticSelection::Call(call)) => match call.target() {
                    crate::BoundCallableTarget::Declaration(instance) => {
                        Some(instance.definition())
                    }
                    _ => None,
                },
                _ => None,
            },
        }
    }

    /// Returns the source operation that requires this evidence.
    pub const fn site(self) -> AnyBoundNodeId {
        match self {
            Self::Call(expression) => AnyBoundNodeId::Expression(expression),
            Self::Implicit { site, .. } => site,
        }
    }
}

/// A selected call whose implementation evidence is required by a body proof.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableProofDependency {
    target: CallableProofTarget,
    obligation: CallableProofObligation,
}

impl CallableProofDependency {
    /// Records the call occurrence and required domain in its selected callable's contract.
    pub const fn new(target: CallableProofTarget, obligation: CallableProofObligation) -> Self {
        Self { target, obligation }
    }

    /// Returns the source or implicit operation selecting the dependent implementation.
    pub const fn target(self) -> CallableProofTarget {
        self.target
    }

    /// Returns the selected callable's required execution property or completion predicate.
    pub const fn obligation(self) -> CallableProofObligation {
        self.obligation
    }
}

/// A body-local proof whose selected implementation dependencies remain to be verified.
///
/// Consumers certify this candidate only after validating every dependency. In particular,
/// mutually dependent declarations cannot establish total execution by promising it to each other.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CallableProofCandidate {
    obligation: CallableProofObligation,
    dependencies: Arc<[CallableProofDependency]>,
}

impl CallableProofCandidate {
    /// Retains one checked body domain and its distinct implementation dependencies.
    pub fn new(
        obligation: CallableProofObligation,
        dependencies: impl IntoIterator<Item = CallableProofDependency>,
    ) -> Self {
        Self {
            obligation,
            dependencies: sorted_unique_shared_slice(dependencies),
        }
    }

    /// Returns the declaration domain checked by this candidate.
    pub const fn obligation(&self) -> CallableProofObligation {
        self.obligation
    }

    /// Returns the selected call domains that must supply compatible implementation evidence.
    pub fn dependencies(&self) -> &[CallableProofDependency] {
        &self.dependencies
    }
}

/// The checked outcome of attempting one body-local contract proof.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CallableProofResult {
    /// Body-local evidence whose selected dependencies still need validation.
    Candidate(CallableProofCandidate),
    /// A proof that was attempted without sufficient evidence.
    Unproven {
        /// The exact declaration or lifecycle obligation that could not be proved.
        obligation: CallableProofObligation,
        /// The most specific source-correlated reason discovered by the checker.
        diagnostic: bray_diagnostics::Diagnostic,
    },
}

impl CallableProofResult {
    /// Returns the obligation checked by this result.
    pub const fn obligation(&self) -> CallableProofObligation {
        match self {
            Self::Candidate(candidate) => candidate.obligation(),
            Self::Unproven { obligation, .. } => *obligation,
        }
    }
}

/// A source declaration promise requiring a checked implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableProofObligation {
    /// An execution property on the specified entry domain.
    Execution(CallableExecutionGuarantee),
    /// A normal-completion predicate identified by its contract clause ordinal.
    Postcondition(SymbolOrdinal),
    /// Successful no-work whole-value finalization for every receiver of a type.
    TypeFinalization {
        /// Operation whose represented cleanup requires this proof.
        site: AnyBoundNodeId,
        /// Element or represented owner type, independent of runtime receiver state.
        ty: bray_symbols::TypeId,
    },
    /// Successful no-work completion of one whole-value graceful step at this scope exit.
    Finalization {
        /// Lexical owner whose cleanup includes the value.
        scope: BoundBlockId,
        /// Source operation ending that ownership scope.
        exit: AnyBoundNodeId,
        /// Complete owned value whose graceful step is discharged.
        access: StorageAccessId,
        /// Checked represented-part ordinal, or the storage root when absent.
        part: Option<SymbolOrdinal>,
    },
    /// Checked synchronous destruction at one cleanup occurrence.
    /// Destruction still executes, including the source destructor's checked remainder.
    SynchronousDestruction {
        /// Lexical owner whose cleanup includes the value.
        scope: BoundBlockId,
        /// Source operation ending that ownership scope.
        exit: AnyBoundNodeId,
        /// Complete owned value whose destruction is certified.
        access: StorageAccessId,
        /// Checked represented-part ordinal, or the storage root when absent.
        part: Option<SymbolOrdinal>,
    },
}

impl CallableProofObligation {
    /// Returns the declaration-level promise, excluding occurrence-specific cleanup certificates.
    pub const fn contract(self) -> Option<bray_symbols::CallableContractObligation> {
        match self {
            Self::Execution(guarantee) => Some(
                bray_symbols::CallableContractObligation::Execution(guarantee),
            ),
            Self::Postcondition(ordinal) => Some(
                bray_symbols::CallableContractObligation::Postcondition(ordinal),
            ),
            Self::Finalization { .. }
            | Self::TypeFinalization { .. }
            | Self::SynchronousDestruction { .. } => None,
        }
    }
}

/// Identifies one implementation promise independently of its callers' generic instantiations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableProofKey<U = BoundUnitId> {
    owner: U,
    obligation: CallableProofObligation,
}

impl<U: Copy> CallableProofKey<U> {
    /// Pairs the implementation identity with its contract obligation.
    pub const fn new(owner: U, obligation: CallableProofObligation) -> Self {
        Self { owner, obligation }
    }

    /// Returns the source body or imported declaration that supplies implementation evidence.
    pub const fn owner(self) -> U {
        self.owner
    }

    /// Returns the execution promise or postcondition being verified.
    pub const fn obligation(self) -> CallableProofObligation {
        self.obligation
    }
}

impl From<bray_symbols::CallableContractObligation> for CallableProofObligation {
    fn from(obligation: bray_symbols::CallableContractObligation) -> Self {
        match obligation {
            bray_symbols::CallableContractObligation::Execution(guarantee) => {
                Self::Execution(guarantee)
            }
            bray_symbols::CallableContractObligation::Postcondition(ordinal) => {
                Self::Postcondition(ordinal)
            }
        }
    }
}
