use bray_declarations::SyntaxAnchor;
use bray_symbols::{TypeId, UnionPayloadFieldSymbolId, UnionVariantSymbolId};

use super::SelectedConversion;

/// The exact checked boundary receiving one propagated value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SelectedPropagationBoundary {
    /// The enclosing callable result boundary.
    Callable,
    /// One enclosing single-yield region.
    YieldRegion(SyntaxAnchor),
}

/// The exact checked behavior of one propagation expression.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SelectedPropagation {
    /// Propagate absence to one nullable boundary.
    Nullable {
        /// The selected lexical boundary.
        boundary: SelectedPropagationBoundary,
        /// The complete nullable type accepted by the boundary.
        result_type: TypeId,
    },
    /// Propagate one recoverable error to a compatible `Result` boundary.
    Result {
        /// The selected lexical boundary.
        boundary: SelectedPropagationBoundary,
        /// The complete `Result` type accepted by the boundary.
        result_type: TypeId,
        /// The checked conversion from the operand error to the boundary error.
        error_conversion: SelectedConversion,
        /// The propagated error variant and its retained payload.
        error: (UnionVariantSymbolId, UnionPayloadFieldSymbolId),
        /// The success variant and its retained payload.
        success: (UnionVariantSymbolId, UnionPayloadFieldSymbolId),
    },
    /// Forward `RunResult` panic or cancellation within the current run.
    CurrentRun {
        /// The completed variant and its retained payload.
        success: (UnionVariantSymbolId, UnionPayloadFieldSymbolId),
    },
}

impl SelectedPropagation {
    /// Returns the selected successful union case, when propagation unwraps a union.
    pub const fn success(&self) -> Option<(UnionVariantSymbolId, UnionPayloadFieldSymbolId)> {
        match self {
            Self::Nullable { .. } => None,
            Self::Result { success, .. } | Self::CurrentRun { success } => Some(*success),
        }
    }

    /// Returns the retained payload on normal continuation.
    pub fn success_projection(&self) -> crate::StorageProjection {
        match self.success() {
            Some((variant, field)) => {
                crate::StorageProjection::ActiveUnionPayloadField { variant, field }
            }
            None => crate::StorageProjection::NullableValue,
        }
    }

    /// Returns the selected lexical result boundary, when propagation targets one.
    pub const fn boundary(&self) -> Option<SelectedPropagationBoundary> {
        match self {
            Self::Nullable { boundary, .. } | Self::Result { boundary, .. } => Some(*boundary),
            Self::CurrentRun { .. } => None,
        }
    }

    /// Returns the complete type accepted by the selected result boundary.
    pub const fn result_type(&self) -> Option<TypeId> {
        match self {
            Self::Nullable { result_type, .. } | Self::Result { result_type, .. } => {
                Some(*result_type)
            }
            Self::CurrentRun { .. } => None,
        }
    }

    /// Returns the checked propagated-error conversion, when required.
    pub const fn error_conversion(&self) -> Option<&SelectedConversion> {
        match self {
            Self::Result {
                error_conversion, ..
            } => Some(error_conversion),
            Self::Nullable { .. } | Self::CurrentRun { .. } => None,
        }
    }
}
