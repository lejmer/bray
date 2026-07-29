use bray_symbols::{ConstantTermId, ConstantValueData, ConstantValueId};

/// Materialized data for one constant value demanded by MIR.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenConstantMapping {
    value: ConstantValueId,
    data: ConstantValueData,
}

impl CodegenConstantMapping {
    /// Creates a mapping from one semantic constant identity to its immutable value data.
    pub const fn new(value: ConstantValueId, data: ConstantValueData) -> Self {
        Self { value, data }
    }

    /// Returns the demanded constant identity.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }

    /// Returns the complete materializable constant data.
    pub const fn data(&self) -> &ConstantValueData {
        &self.data
    }
}

/// Resolves one closed constant term retained by a MIR predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenConstantTermMapping {
    term: ConstantTermId,
    value: ConstantValueId,
}

impl CodegenConstantTermMapping {
    /// Creates a mapping from one closed term to its materialized value.
    pub const fn new(term: ConstantTermId, value: ConstantValueId) -> Self {
        Self { term, value }
    }

    /// Returns the demanded constant term.
    pub const fn term(self) -> ConstantTermId {
        self.term
    }

    /// Returns the materialized value selected for the term.
    pub const fn value(self) -> ConstantValueId {
        self.value
    }
}
