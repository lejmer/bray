use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;

use crate::{ExecutionProperty, SymbolOrdinal};

/// Maximum condition traversal shared by proof normalization and portable validation.
pub const EXECUTION_CONDITION_WORK_LIMIT: usize = 128;

/// An exact execution promise identified in its owning callable's contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableExecutionObligation<C> {
    /// A property on every valid input, or on one identified entry domain.
    Property(ExecutionProperty, Option<C>),
    /// A normal-completion predicate.
    Postcondition(C),
}

impl<C: Copy> CallableExecutionObligation<C> {
    /// Whether a proof cycle would assume its own completion.
    pub const fn requires_acyclic_proof(self) -> bool {
        !matches!(self, Self::Property(ExecutionProperty::Pure, _))
    }

    /// Returns the property when this is an execution obligation.
    pub const fn property(self) -> Option<ExecutionProperty> {
        match self {
            Self::Property(property, _) => Some(property),
            Self::Postcondition(_) => None,
        }
    }
}

/// Guarantees sharing one conjunction of execution-entry conditions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableExecutionDomain<T> {
    /// Stable position in the callable's execution domains.
    pub ordinal: SymbolOrdinal,
    /// Common requirements and enclosing guards, observed at entry.
    pub entry: Arc<[T]>,
    /// Independent promises on the entry domain.
    pub properties: Arc<[ExecutionProperty]>,
    /// Completion predicates with ordinals unique within the callable.
    pub postconditions: Arc<[(SymbolOrdinal, T)]>,
}

/// The authority supplying a declared execution promise.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableExecutionOrigin {
    /// The provider checked a Bray body and retained its selected dependencies.
    CheckedBody,
    /// An extern trusted declaration in a trusted module asserts the promise.
    /// Its ABI and caller capabilities remain in the owning callable contract.
    ForeignAssertion,
    /// A trait requirement must be established by its selected implementation.
    Requirement,
    /// A compiler intrinsic whose identity and supported properties are checked by the consumer.
    CompilerIntrinsic,
}

/// A selected proof target retaining either a callable instance or its opaque type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableExecutionTarget<C, T> {
    /// An exact declaration and generic substitution.
    Callable(C),
    /// An opaque callable contract, which can support purity but not a new completion proof.
    Indirect(T),
}

/// One promise and the exact implementation evidence on which it depends.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableExecutionEvidence<T> {
    /// The authority that establishes this promise.
    pub origin: CallableExecutionOrigin,
    /// The promise established by this record.
    pub obligation: CallableExecutionObligation<SymbolOrdinal>,
    /// Selected implementation promises, in stable semantic order.
    pub dependencies: Arc<[(T, CallableExecutionObligation<SymbolOrdinal>)]>,
}

impl<T: Ord> CallableExecutionEvidence<T> {
    /// Records one promise with a normalized dependency set.
    pub fn new(
        origin: CallableExecutionOrigin,
        obligation: CallableExecutionObligation<SymbolOrdinal>,
        dependencies: impl IntoIterator<Item = (T, CallableExecutionObligation<SymbolOrdinal>)>,
    ) -> Self {
        Self {
            origin,
            obligation,
            dependencies: sorted_unique_shared_slice(dependencies),
        }
    }
}

/// Portable declaration domains and their separately validated implementation evidence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableExecutionContract<T, D> {
    /// Declared input domains, including their completion predicates.
    pub domains: Arc<[CallableExecutionDomain<T>]>,
    /// Evidence supplied for the declared promises.
    pub evidence: Arc<[CallableExecutionEvidence<D>]>,
}

impl<T, D> Default for CallableExecutionContract<T, D> {
    fn default() -> Self {
        Self {
            domains: Arc::from([]),
            evidence: Arc::from([]),
        }
    }
}

impl<T: Copy, D: Copy> CallableExecutionContract<T, D> {
    /// Converts semantic identities without losing domains, obligations, or proof authority.
    pub fn try_map<C, U, E: Ord, X>(
        &self,
        context: &mut C,
        mut term: impl FnMut(&mut C, T) -> Result<U, X>,
        mut target: impl FnMut(&mut C, D) -> Result<E, X>,
    ) -> Result<CallableExecutionContract<U, E>, X> {
        let domains = self
            .domains
            .iter()
            .map(|domain| {
                Ok(CallableExecutionDomain {
                    ordinal: domain.ordinal,
                    entry: domain
                        .entry
                        .iter()
                        .copied()
                        .map(|value| term(context, value))
                        .collect::<Result<Vec<_>, X>>()?
                        .into(),
                    properties: Arc::clone(&domain.properties),
                    postconditions: domain
                        .postconditions
                        .iter()
                        .map(|(ordinal, value)| {
                            term(context, *value).map(|value| (*ordinal, value))
                        })
                        .collect::<Result<Vec<_>, X>>()?
                        .into(),
                })
            })
            .collect::<Result<Vec<_>, X>>()?;

        let evidence = self
            .evidence
            .iter()
            .map(|proof| {
                let dependencies = proof
                    .dependencies
                    .iter()
                    .map(|(dependency, obligation)| {
                        target(context, *dependency).map(|dependency| (dependency, *obligation))
                    })
                    .collect::<Result<Vec<_>, X>>()?;

                Ok(CallableExecutionEvidence::new(
                    proof.origin,
                    proof.obligation,
                    dependencies,
                ))
            })
            .collect::<Result<Vec<_>, X>>()?;

        Ok(CallableExecutionContract {
            domains: domains.into(),
            evidence: evidence.into(),
        })
    }
}

/// Execution evidence expressed in interned semantic identities.
pub type ResolvedCallableExecutionContract = CallableExecutionContract<
    crate::ConstantTermId,
    CallableExecutionTarget<crate::CallableInstanceId, crate::TypeId>,
>;
