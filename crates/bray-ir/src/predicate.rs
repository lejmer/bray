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
    pub fn condition(condition: ConstantTermId, input_types: impl IntoIterator<Item = TypeId>) -> Self {
        Self::Condition { condition, input_types: shared_slice(input_types) }
    }
}
