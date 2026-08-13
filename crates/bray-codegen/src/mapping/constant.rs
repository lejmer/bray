use bray_symbols::{ConstantTermId, ConstantValueData, ConstantValueId, TypeId};

use crate::CodegenInstanceKey;

/// Materialized data for one constant value demanded by MIR.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenConstantMapping {
    value: ConstantValueId,
    semantic_type: TypeId,
    data: ConstantValueData,
}

impl CodegenConstantMapping {
    /// Creates a mapping from one semantic constant identity to its immutable value data.
    pub const fn new(value: ConstantValueId, data: ConstantValueData) -> Self {
        Self {
            value,
            semantic_type: data.ty(),
            data,
        }
    }

    /// Creates a mapping materialized with the representation required by one MIR use.
    pub fn with_representation(
        value: ConstantValueId,
        data: ConstantValueData,
        representation: TypeId,
    ) -> Self {
        let semantic_type = data.ty();
        let data = ConstantValueData::new(representation, data.kind().clone());

        Self {
            value,
            semantic_type,
            data,
        }
    }

    /// Returns the demanded constant identity.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }

    /// Returns the semantic type that establishes the constant's identity.
    pub const fn semantic_type(&self) -> TypeId {
        self.semantic_type
    }

    /// Returns the materialized representation required by this use.
    pub const fn representation(&self) -> TypeId {
        self.data.ty()
    }

    /// Returns the complete materializable constant data.
    pub const fn data(&self) -> &ConstantValueData {
        &self.data
    }
}

/// Resolves one closed constant term retained by a MIR predicate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenConstantTermMapping {
    owner: CodegenInstanceKey,
    term: ConstantTermId,
    value: ConstantValueId,
}

impl CodegenConstantTermMapping {
    /// Creates a mapping from one closed term to its materialized value.
    pub const fn new(
        owner: CodegenInstanceKey,
        term: ConstantTermId,
        value: ConstantValueId,
    ) -> Self {
        Self { owner, term, value }
    }

    /// Returns the concrete definition containing the term occurrence.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
    }

    /// Returns the demanded constant term.
    pub const fn term(&self) -> ConstantTermId {
        self.term
    }

    /// Returns the materialized value selected for the term.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }
}
