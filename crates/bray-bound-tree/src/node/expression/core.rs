use bray_symbols::TypeId;

use crate::{BoundBlockId, BoundExpressionId, BoundNodeOrigin, BoundPatternId};

use super::{
    BoundAnonymousCallableExpression, BoundAssignmentExpression, BoundAwaitExpression,
    BoundBinaryExpression, BoundCallExpression, BoundControlTransferExpression,
    BoundConversionExpression, BoundErrorCallExpression, BoundErrorConversionExpression,
    BoundForExpression, BoundGeneratorExpression, BoundLeadingDotVariantExpression,
    BoundLiteralExpression, BoundMatchExpression, BoundMemberAccessExpression, BoundNameExpression,
    BoundPatternReferenceExpression, BoundStructConstructionExpression, BoundStructuredExpression,
    BoundStructuredExpressionKind, BoundTraitQualifiedMemberExpression, BoundUnaryExpression,
    BoundUnqualifiedVariantExpression, BoundUnresolvedReferenceExpression, IterationSourceMode,
};

/// A checked expression retaining its exact semantic category.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum BoundExpression {
    /// A source-shaped block used as an expression.
    Block(BoundBlockExpression),
    /// A source literal with an exact literal category.
    Literal(BoundLiteralExpression),
    /// A resolved value name.
    Name(BoundNameExpression),
    /// A value name whose pattern-local interpretation depends on the checked subject type.
    PatternReference(BoundPatternReferenceExpression),
    /// A source reference retained after lookup recovery.
    UnresolvedReference(BoundUnresolvedReferenceExpression),
    /// A prefix operator expression.
    Unary(BoundUnaryExpression),
    /// A binary operator expression.
    Binary(BoundBinaryExpression),
    /// An assignment expression.
    Assignment(BoundAssignmentExpression),
    /// A call with source-ordered argument inputs.
    Call(BoundCallExpression),
    /// A call retained after semantic recovery.
    ErrorCall(BoundErrorCallExpression),
    /// An explicit conversion expression.
    Conversion(BoundConversionExpression),
    /// A conversion retained after semantic recovery.
    ErrorConversion(BoundErrorConversionExpression),
    /// A reference to a separately checked anonymous callable unit.
    AnonymousCallable(BoundAnonymousCallableExpression),
    /// A direct await composing a future into the current run.
    Await(BoundAwaitExpression),
    /// A source-shaped aggregate, control-flow, or effect expression.
    Structured(BoundStructuredExpression),
    /// A struct construction preserving named field associations.
    StructConstruction(BoundStructConstructionExpression),
    /// A receiver member selection.
    MemberAccess(BoundMemberAccessExpression),
    /// A contextually typed leading-dot variant reference.
    LeadingDotVariant(BoundLeadingDotVariantExpression),
    /// An unqualified variant reference considered only after ordinary lookup finds no name.
    UnqualifiedVariant(BoundUnqualifiedVariantExpression),
    /// A trait-qualified receiver member selection.
    TraitQualifiedMember(BoundTraitQualifiedMemberExpression),
    /// A return, yield, break, or continue transfer.
    ControlTransfer(BoundControlTransferExpression),
    /// A source for loop with its exact pattern and branches.
    For(BoundForExpression),
    /// A source match with exact arm associations.
    Match(BoundMatchExpression),
    /// A generator iteration and its yield-capable region.
    Generator(BoundGeneratorExpression),
    /// An expression that could not be checked successfully.
    Error(BoundErrorExpression),
}

impl BoundExpression {
    /// Returns this expression's stable machine-readable category name.
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Block(_) => "block",
            Self::Literal(_) => "literal",
            Self::Name(_) => "name",
            Self::PatternReference(_) => "pattern_reference",
            Self::UnresolvedReference(_) => "unresolved_reference",
            Self::Unary(_) => "unary",
            Self::Binary(_) => "binary",
            Self::Assignment(_) => "assignment",
            Self::Call(_) => "call",
            Self::ErrorCall(_) => "error_call",
            Self::Conversion(_) => "conversion",
            Self::ErrorConversion(_) => "error_conversion",
            Self::AnonymousCallable(_) => "anonymous_callable",
            Self::Await(_) => "await",
            Self::Structured(expression) => expression.kind().as_str(),
            Self::StructConstruction(_) => "struct_construction",
            Self::MemberAccess(_) => "member_access",
            Self::LeadingDotVariant(_) => "leading_dot_variant",
            Self::UnqualifiedVariant(_) => "unqualified_variant",
            Self::TraitQualifiedMember(_) => "trait_qualified_member",
            Self::ControlTransfer(_) => "control_transfer",
            Self::For(_) => "for",
            Self::Match(_) => "match",
            Self::Generator(_) => "generator",
            Self::Error(_) => "error",
        }
    }

    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(&self) -> BoundNodeOrigin {
        match self {
            Self::Block(expression) => expression.origin(),
            Self::Literal(expression) => expression.origin(),
            Self::Name(expression) => expression.origin(),
            Self::PatternReference(expression) => expression.origin(),
            Self::UnresolvedReference(expression) => expression.origin(),
            Self::Unary(expression) => expression.origin(),
            Self::Binary(expression) => expression.origin(),
            Self::Assignment(expression) => expression.origin(),
            Self::Call(expression) => expression.origin(),
            Self::ErrorCall(expression) => expression.origin(),
            Self::Conversion(expression) => expression.origin(),
            Self::ErrorConversion(expression) => expression.origin(),
            Self::AnonymousCallable(expression) => expression.origin(),
            Self::Await(expression) => expression.origin(),
            Self::Structured(expression) => expression.origin(),
            Self::StructConstruction(expression) => expression.origin(),
            Self::MemberAccess(expression) => expression.origin(),
            Self::LeadingDotVariant(expression) => expression.origin(),
            Self::UnqualifiedVariant(expression) => expression.origin(),
            Self::TraitQualifiedMember(expression) => expression.origin(),
            Self::ControlTransfer(expression) => expression.origin(),
            Self::For(expression) => expression.origin(),
            Self::Match(expression) => expression.origin(),
            Self::Generator(expression) => expression.origin(),
            Self::Error(expression) => expression.origin(),
        }
    }

    /// Returns the checked or recovery type of this expression.
    pub const fn ty(&self) -> Option<TypeId> {
        match self {
            Self::Block(expression) => expression.ty(),
            Self::Literal(expression) => expression.ty(),
            Self::Name(expression) => expression.ty(),
            Self::PatternReference(_) => None,
            Self::UnresolvedReference(expression) => Some(expression.ty()),
            Self::Unary(expression) => expression.ty(),
            Self::Binary(expression) => expression.ty(),
            Self::Assignment(expression) => expression.ty(),
            Self::Call(expression) => expression.ty(),
            Self::ErrorCall(expression) => Some(expression.ty()),
            Self::Conversion(expression) => expression.ty(),
            Self::ErrorConversion(expression) => Some(expression.ty()),
            Self::AnonymousCallable(expression) => expression.ty(),
            Self::Await(expression) => expression.ty(),
            Self::Structured(expression) => expression.ty(),
            Self::StructConstruction(expression) => expression.ty(),
            Self::MemberAccess(expression) => expression.ty(),
            Self::LeadingDotVariant(expression) => expression.ty(),
            Self::UnqualifiedVariant(expression) => expression.ty(),
            Self::TraitQualifiedMember(expression) => expression.ty(),
            Self::ControlTransfer(expression) => expression.ty(),
            Self::For(expression) => expression.ty(),
            Self::Match(expression) => expression.ty(),
            Self::Generator(expression) => expression.ty(),
            Self::Error(expression) => Some(expression.ty()),
        }
    }

    /// Returns whether syntax or semantic recovery contributed to this expression.
    pub const fn is_recovered(&self) -> bool {
        match self {
            Self::Block(expression) => expression.is_recovered(),
            Self::Literal(expression) => expression.is_recovered(),
            Self::Name(expression) => expression.is_recovered(),
            Self::PatternReference(expression) => expression.is_recovered(),
            Self::UnresolvedReference(_) => true,
            Self::Unary(expression) => expression.is_recovered(),
            Self::Binary(expression) => expression.is_recovered(),
            Self::Assignment(expression) => expression.is_recovered(),
            Self::Call(expression) => expression.is_recovered(),
            Self::ErrorCall(_) => true,
            Self::Conversion(expression) => expression.is_recovered(),
            Self::ErrorConversion(_) => true,
            Self::AnonymousCallable(expression) => expression.is_recovered(),
            Self::Await(expression) => expression.is_recovered(),
            Self::Structured(expression) => expression.is_recovered(),
            Self::StructConstruction(expression) => expression.is_recovered(),
            Self::MemberAccess(expression) => expression.is_recovered(),
            Self::LeadingDotVariant(expression) => expression.is_recovered(),
            Self::UnqualifiedVariant(expression) => expression.is_recovered(),
            Self::TraitQualifiedMember(expression) => expression.is_recovered(),
            Self::ControlTransfer(expression) => expression.is_recovered(),
            Self::For(expression) => expression.is_recovered(),
            Self::Match(expression) => expression.is_recovered(),
            Self::Generator(expression) => expression.is_recovered(),
            Self::Error(_) => true,
        }
    }

    /// Returns direct expression children in semantic evaluation order.
    pub fn child_expressions(&self) -> impl Iterator<Item = BoundExpressionId> + '_ {
        let children = match self {
            Self::Unary(expression) => expression.operands(),
            Self::Binary(expression) => expression.operands(),
            Self::Assignment(expression) => expression.operands(),
            Self::Call(expression) => expression.operands(),
            Self::ErrorCall(expression) => expression.operands(),
            Self::Conversion(expression) => expression.operands(),
            Self::ErrorConversion(expression) => expression.operands(),
            Self::Await(expression) => expression.operands(),
            Self::Structured(expression) => expression.operands(),
            Self::StructConstruction(expression) => expression.operands(),
            Self::MemberAccess(expression) => expression.operands(),
            Self::TraitQualifiedMember(expression) => expression.operands(),
            Self::ControlTransfer(expression) => expression.operands(),
            Self::For(expression) => expression.operands(),
            Self::Match(expression) => expression.operands(),
            Self::Generator(expression) => expression.operands(),
            Self::Block(_)
            | Self::Literal(_)
            | Self::Name(_)
            | Self::PatternReference(_)
            | Self::UnresolvedReference(_)
            | Self::LeadingDotVariant(_)
            | Self::UnqualifiedVariant(_)
            | Self::AnonymousCallable(_)
            | Self::Error(_) => &[],
        };

        children.iter().copied()
    }

    /// Returns direct block children in source-semantic order.
    pub fn child_blocks(&self) -> impl Iterator<Item = BoundBlockId> + '_ {
        let block = match self {
            Self::Block(expression) => Some(expression.block()),
            Self::Structured(expression) => expression.blocks().first().copied(),
            Self::For(expression) => expression.blocks().first().copied(),
            Self::Match(expression) => expression.blocks().first().copied(),
            Self::Generator(expression) => expression.blocks().first().copied(),
            _ => None,
        };

        let remaining = match self {
            Self::Structured(expression) => expression.blocks().get(1..).unwrap_or_default(),
            Self::For(expression) => expression.blocks().get(1..).unwrap_or_default(),
            Self::Match(expression) => expression.blocks().get(1..).unwrap_or_default(),
            Self::Generator(expression) => expression.blocks().get(1..).unwrap_or_default(),
            _ => &[],
        };

        block.into_iter().chain(remaining.iter().copied())
    }

    /// Returns direct pattern children in source-semantic order.
    pub fn child_patterns(&self) -> impl Iterator<Item = BoundPatternId> + '_ {
        match self {
            Self::Structured(expression) => expression.patterns(),
            Self::For(expression) => expression.patterns(),
            Self::Match(expression) => expression.patterns(),
            Self::Generator(expression) => expression.patterns(),
            _ => &[],
        }
        .iter()
        .copied()
    }

    /// Returns the iterated expression and its access mode when this expression performs iteration.
    pub fn iteration_source(&self) -> Option<(BoundExpressionId, IterationSourceMode)> {
        match self {
            Self::For(expression) => Some((expression.source(), expression.source_mode())),
            Self::Generator(expression) => Some((expression.source(), expression.source_mode())),
            Self::Structured(expression)
                if matches!(
                    expression.kind(),
                    BoundStructuredExpressionKind::BooleanAllFold
                        | BoundStructuredExpressionKind::BooleanAnyFold
                ) =>
            {
                let [source] = expression.operands() else {
                    return None;
                };

                Some((*source, IterationSourceMode::Shared))
            }
            _ => None,
        }
    }
}

/// A source-shaped block expression and its checked result type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundBlockExpression {
    origin: BoundNodeOrigin,
    block: BoundBlockId,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundBlockExpression {
    /// Creates a checked block expression.
    pub const fn new(
        origin: BoundNodeOrigin,
        block: BoundBlockId,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            block,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the block evaluated by this expression.
    pub const fn block(self) -> BoundBlockId {
        self.block
    }

    /// Returns the checked result type.
    pub const fn ty(self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this expression or its block.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// An expression preserved after semantic recovery.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundErrorExpression {
    origin: BoundNodeOrigin,
    ty: TypeId,
}

impl BoundErrorExpression {
    /// Creates an error expression with the most useful type known after recovery.
    pub const fn new(origin: BoundNodeOrigin, ty: TypeId) -> Self {
        Self { origin, ty }
    }

    /// Returns the source or synthesized origin of this expression.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the useful recovery type or canonical error type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{error_type, source_anchor};
    use crate::{BoundErrorExpression, BoundExpression, BoundNodeOrigin};

    #[test]
    fn error_expressions_retain_source_and_recovery_type() {
        let source = source_anchor();
        let ty = error_type();

        let expression = BoundExpression::Error(BoundErrorExpression::new(
            BoundNodeOrigin::source(source),
            ty,
        ));

        assert_eq!(expression.origin().source_anchor(), source);
        assert_eq!(expression.ty(), Some(ty));
        assert!(expression.is_recovered());
    }
}
