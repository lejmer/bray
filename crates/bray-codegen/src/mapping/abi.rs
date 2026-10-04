use std::num::NonZeroU16;
use std::sync::Arc;

use bray_base::shared_slice;

/// One register representation selected by aggregate ABI classification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenAbiScalar {
    /// Integer bits, including a partial register at the end of an aggregate.
    Integer(NonZeroU16),
    /// One single-precision value.
    Float32,
    /// One double-precision value.
    Float64,
    /// Two single-precision values packed into one vector register.
    Float32Pair,
}

/// One physical register value and its byte position in semantic aggregate storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenAbiPiece {
    /// Offset of the represented bytes in the semantic value.
    pub offset_bytes: u64,
    /// Exact register representation.
    pub scalar: CodegenAbiScalar,
}

/// Target-selected register pieces for one directly passed aggregate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenAggregateCoercion(Arc<[CodegenAbiPiece]>);

impl CodegenAggregateCoercion {
    /// Creates a completed, nonempty aggregate coercion.
    pub fn new(pieces: impl IntoIterator<Item = CodegenAbiPiece>) -> Self {
        let pieces = shared_slice(pieces);

        assert!(
            !pieces.is_empty(),
            "aggregate coercion requires register pieces"
        );

        Self(pieces)
    }

    /// Returns physical values in machine signature order.
    pub fn pieces(&self) -> &[CodegenAbiPiece] {
        &self.0
    }
}
