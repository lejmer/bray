use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    CheckedConstraint, GenericOwnerId, GenericParameterSymbolId, SymbolOrdinal,
    TraitApplicationTemplate, TypeExpressionTemplate,
};

use super::DeclarationExpressionTemplate;

/// One source or already-resolved generic constraint in declaration order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GenericConstraintTemplate {
    /// A source expression retained for later checking.
    Source {
        /// The constraint's stable declaration-order position.
        ordinal: SymbolOrdinal,
        /// The syntax occurrence that owns the independently bound constraint unit.
        unit: bray_declarations::SyntaxAnchor,
        /// The exact source expression occurrence.
        expression: DeclarationExpressionTemplate,
    },
    /// A source trait-satisfaction requirement retained for semantic checking.
    TraitSatisfaction {
        /// The constraint's stable declaration-order position.
        ordinal: SymbolOrdinal,
        /// The syntax occurrence that owns the constraint.
        unit: bray_declarations::SyntaxAnchor,
        /// The implementation-eligible subject type.
        subject: TypeExpressionTemplate,
        /// The exact required trait application.
        application: TraitApplicationTemplate,
    },
    /// A source type-equality requirement retained for semantic checking.
    TypeEquality {
        /// The constraint's stable declaration-order position.
        ordinal: SymbolOrdinal,
        /// The syntax occurrence that owns the constraint.
        unit: bray_declarations::SyntaxAnchor,
        /// The left type expression.
        left: TypeExpressionTemplate,
        /// The right type expression.
        right: TypeExpressionTemplate,
    },
    /// A source-independent imported constraint.
    Resolved(CheckedConstraint),
}

impl GenericConstraintTemplate {
    /// Creates one unevaluated generic constraint.
    pub const fn new(
        ordinal: SymbolOrdinal,
        unit: bray_declarations::SyntaxAnchor,
        expression: DeclarationExpressionTemplate,
    ) -> Self {
        Self::Source {
            ordinal,
            unit,
            expression,
        }
    }

    /// Creates one unevaluated trait-satisfaction constraint.
    pub const fn trait_satisfaction(
        ordinal: SymbolOrdinal,
        unit: bray_declarations::SyntaxAnchor,
        subject: TypeExpressionTemplate,
        application: TraitApplicationTemplate,
    ) -> Self {
        Self::TraitSatisfaction {
            ordinal,
            unit,
            subject,
            application,
        }
    }

    /// Creates one unevaluated type-equality constraint.
    pub const fn type_equality(
        ordinal: SymbolOrdinal,
        unit: bray_declarations::SyntaxAnchor,
        left: TypeExpressionTemplate,
        right: TypeExpressionTemplate,
    ) -> Self {
        Self::TypeEquality {
            ordinal,
            unit,
            left,
            right,
        }
    }

    /// Returns the constraint's stable declaration-order position.
    pub const fn ordinal(&self) -> SymbolOrdinal {
        match self {
            Self::Source { ordinal, .. }
            | Self::TraitSatisfaction { ordinal, .. }
            | Self::TypeEquality { ordinal, .. } => *ordinal,
            Self::Resolved(constraint) => constraint.ordinal(),
        }
    }

    /// Returns the exact source expression retained for checking.
    pub const fn expression(&self) -> Option<DeclarationExpressionTemplate> {
        match self {
            Self::Source { expression, .. } => Some(*expression),
            Self::TraitSatisfaction { .. } | Self::TypeEquality { .. } | Self::Resolved(_) => None,
        }
    }

    /// Returns the source syntax that owns the independently bound constraint unit.
    pub const fn unit_syntax(&self) -> Option<bray_declarations::SyntaxAnchor> {
        match self {
            Self::Source { unit, .. }
            | Self::TraitSatisfaction { unit, .. }
            | Self::TypeEquality { unit, .. } => Some(*unit),
            Self::Resolved(_) => None,
        }
    }

    /// Returns the retained subject and trait application for a trait-satisfaction constraint.
    pub const fn trait_satisfaction_templates(
        &self,
    ) -> Option<(&TypeExpressionTemplate, &TraitApplicationTemplate)> {
        match self {
            Self::TraitSatisfaction {
                subject,
                application,
                ..
            } => Some((subject, application)),
            Self::Source { .. } | Self::TypeEquality { .. } | Self::Resolved(_) => None,
        }
    }

    /// Returns both retained type templates for a type-equality constraint.
    pub const fn type_equality_templates(
        &self,
    ) -> Option<(&TypeExpressionTemplate, &TypeExpressionTemplate)> {
        match self {
            Self::TypeEquality { left, right, .. } => Some((left, right)),
            Self::Source { .. } | Self::TraitSatisfaction { .. } | Self::Resolved(_) => None,
        }
    }

    /// Returns the checked constraint when supplied by a compiled interface.
    pub const fn resolved(&self) -> Option<CheckedConstraint> {
        match self {
            Self::Source { .. } | Self::TraitSatisfaction { .. } | Self::TypeEquality { .. } => {
                None
            }
            Self::Resolved(constraint) => Some(*constraint),
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
