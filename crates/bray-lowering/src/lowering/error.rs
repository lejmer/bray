use bray_ir::MirCapacityError;
use bray_symbols::SemanticValueStoreError;

/// An operational failure encountered while lowering a checked unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoweringError {
    /// The semantic value store rejected a required read or intern operation.
    SemanticValue(SemanticValueStoreError),
    /// A MIR identity table exceeded its compact representation.
    MirCapacity(MirCapacityError),
}

impl From<MirCapacityError> for LoweringError {
    fn from(error: MirCapacityError) -> Self {
        Self::MirCapacity(error)
    }
}

impl From<SemanticValueStoreError> for LoweringError {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::SemanticValue(error)
    }
}
