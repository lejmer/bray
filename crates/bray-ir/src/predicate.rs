use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{ConstantTermId, PredicateInstanceData, SymbolOrdinal, TypeId};

use crate::MirCallableReference;

/// Exact checked predicate evaluated by an observational generated callable.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirPredicateReference {
    /// One ordinary declaration-owned requirement with its selected specialization.
    Clause {
        /// Callable whose complete checked contract owns the expression.
        callable: MirCallableReference,
        /// Stable contract-expression ordinal, including preceding non-runtime clauses.
        ordinal: SymbolOrdinal,
    },
    /// One checked named predicate application.
    Predicate(PredicateInstanceData),
    /// A requirement retained in an erased callable type.
    Condition {
        /// Checked condition in receiver-first argument order.
        condition: ConstantTermId,
        /// Observed argument types before the helper adds readonly borrows.
        input_types: Arc<[TypeId]>,
    },
}

impl MirPredicateReference {
    /// Creates an erased callable requirement without acquiring or consuming its inputs.
    pub fn condition(
        condition: ConstantTermId,
        input_types: impl IntoIterator<Item = TypeId>,
    ) -> Self {
        Self::Condition {
            condition,
            input_types: shared_slice(input_types),
        }
    }
}

/// Stable predicate unit identity with its compilation-local semantic payload.
///
/// The compiler derives the digest from the checked reference and its specialization.
/// Equality, ordering and hashing
/// use that digest, so semantic interning order cannot affect persistent unit keys.
#[derive(Clone, Debug)]
pub struct MirPredicateKey {
    identity: [u8; 32],
    reference: MirPredicateReference,
}

impl MirPredicateKey {
    /// Associates a compiler-derived structural identity with its checked reference.
    pub const fn new(identity: [u8; 32], reference: MirPredicateReference) -> Self {
        Self {
            identity,
            reference,
        }
    }

    /// Returns the checked reference used to realize this unit in its compilation.
    pub const fn reference(&self) -> &MirPredicateReference {
        &self.reference
    }
}

impl PartialEq for MirPredicateKey {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for MirPredicateKey {}

impl PartialOrd for MirPredicateKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MirPredicateKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.identity.cmp(&other.identity)
    }
}

impl Hash for MirPredicateKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&self.identity, state);
    }
}
