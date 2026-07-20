use std::sync::Arc;

use bray_base::shared_slice;

use crate::{CheckedConstraint, GenericOwnerId, GenericParameterSymbolId, SymbolOrdinal};

use super::DeclarationExpressionTemplate;

/// One source or already-resolved generic constraint in declaration order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GenericConstraintTemplate {
    /// A source expression retained for later checking.
    Source {
        /// The constraint's stable declaration-order position.
        ordinal: SymbolOrdinal,
        /// The exact source expression occurrence.
        expression: DeclarationExpressionTemplate,
    },
    /// A source-independent imported constraint.
    Resolved(CheckedConstraint),
}

impl GenericConstraintTemplate {
    /// Creates one unevaluated generic constraint.
    pub const fn new(ordinal: SymbolOrdinal, expression: DeclarationExpressionTemplate) -> Self {
        Self::Source {
            ordinal,
            expression,
        }
    }

    /// Returns the constraint's stable declaration-order position.
    pub const fn ordinal(self) -> SymbolOrdinal {
        match self {
            Self::Source { ordinal, .. } => ordinal,
            Self::Resolved(constraint) => constraint.ordinal(),
        }
    }

    /// Returns the exact source expression retained for checking.
    pub const fn expression(self) -> Option<DeclarationExpressionTemplate> {
        match self {
            Self::Source { expression, .. } => Some(expression),
            Self::Resolved(_) => None,
        }
    }

    /// Returns the checked constraint when supplied by a compiled interface.
    pub const fn resolved(self) -> Option<CheckedConstraint> {
        match self {
            Self::Source { .. } => None,
            Self::Resolved(constraint) => Some(constraint),
        }
    }
}

/// The ordered generic surface of one declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenericDeclarationTemplate {
    owner: GenericOwnerId,
    parameters: Arc<[GenericParameterSymbolId]>,
    constraints: Arc<[GenericConstraintTemplate]>,
}

impl GenericDeclarationTemplate {
    /// Creates a generic declaration template in source order.
    pub fn new(
        owner: GenericOwnerId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        constraints: impl IntoIterator<Item = GenericConstraintTemplate>,
    ) -> Self {
        Self {
            owner,
            parameters: shared_slice(parameters),
            constraints: shared_slice(constraints),
        }
    }

    /// Returns the declaration that owns this generic surface.
    pub const fn owner(&self) -> GenericOwnerId {
        self.owner
    }

    /// Returns generic parameters in declaration order.
    pub fn parameters(&self) -> &[GenericParameterSymbolId] {
        &self.parameters
    }

    /// Returns constraint templates in declaration order.
    pub fn constraints(&self) -> &[GenericConstraintTemplate] {
        &self.constraints
    }
}
