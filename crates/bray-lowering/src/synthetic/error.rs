use bray_ir::MirCapacityError;
use bray_symbols::{SemanticValueStoreError, TypeId};

/// An operational failure encountered while lowering a compiler-generated body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntheticLoweringError {
    /// Semantic value construction or lookup failed.
    SemanticValue(SemanticValueStoreError),
    /// A generated MIR identity table exceeded its compact representation.
    Capacity(MirCapacityError),
    /// A represented member or element ordinal exceeds the MIR index range.
    LayoutOverflow(TypeId),
    /// An exact integer expression needs more bits than the backend representation permits.
    ExactIntegerCapacity {
        /// A lower bound on the required representation width.
        minimum_bits: u64,
    },
}

impl<C: super::SyntheticLoweringContext + ?Sized> super::SyntheticLowerer<'_, C> {
    pub(crate) fn capacity_error(&self, cause: MirCapacityError) -> C::Error {
        SyntheticLoweringError::Capacity(cause).into()
    }
}
