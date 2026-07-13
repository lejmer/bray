use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundUnitKind,
    BoundUnitView,
};

use crate::CheckerCancellation;

/// Typed inputs for whole-unit semantic checking.
#[derive(Clone, Copy)]
pub struct UnitCheckRequest<'view> {
    view: BoundUnitView<'view>,
    root: UnitCheckRoot,
    cancellation: &'view dyn CheckerCancellation,
}

/// The exact root category of one independently checked semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitCheckRoot {
    /// A declared or anonymous callable body.
    CallableBody(BoundCallableBodyId),
    /// A declaration-owned expression.
    Expression(BoundExpressionId),
    /// An ordered declaration-owned expression sequence.
    ExpressionSequence(BoundBlockId),
}

/// Rejects an inconsistent whole-unit checker request before analysis begins.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitCheckRequestError {
    /// The root belongs to another bound unit.
    ForeignRoot,
    /// The root category does not match the independently checked unit category.
    RootKindMismatch,
}

impl UnitCheckRoot {
    pub(crate) const fn accepts(self, kind: BoundUnitKind) -> bool {
        matches!(
            (self, kind),
            (
                Self::CallableBody(_),
                BoundUnitKind::CallableBody | BoundUnitKind::AnonymousCallable
            ) | (
                Self::Expression(_),
                BoundUnitKind::RuntimeDefault
                    | BoundUnitKind::ConstantTemplate
                    | BoundUnitKind::PredicateDefinition
            ) | (
                Self::ExpressionSequence(_),
                BoundUnitKind::Constraint | BoundUnitKind::ContractClause
            )
        )
    }

    pub(crate) const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::CallableBody(root) => AnyBoundNodeId::CallableBody(root),
            Self::Expression(root) => AnyBoundNodeId::Expression(root),
            Self::ExpressionSequence(root) => AnyBoundNodeId::Block(root),
        }
    }
}

impl<'view> UnitCheckRequest<'view> {
    /// Creates a checker request over committed read-only bound structure.
    ///
    /// Returns an error when the root belongs to another unit or its category
    /// does not match the independently checked unit.
    pub fn new(
        view: BoundUnitView<'view>,
        root: UnitCheckRoot,
        cancellation: &'view dyn CheckerCancellation,
    ) -> Result<Self, UnitCheckRequestError> {
        if root.node().unit() != view.unit() {
            return Err(UnitCheckRequestError::ForeignRoot);
        }

        if !root.accepts(view.kind()) {
            return Err(UnitCheckRequestError::RootKindMismatch);
        }

        Ok(Self {
            view,
            root,
            cancellation,
        })
    }

    /// Returns the read-only bound unit view to analyze.
    pub const fn view(self) -> BoundUnitView<'view> {
        self.view
    }

    /// Returns the exact bound root to analyze.
    pub const fn root(self) -> UnitCheckRoot {
        self.root
    }

    /// Returns whether compilation cancellation has been requested.
    pub fn is_cancelled(self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::{UnitCheckRequest, UnitCheckRoot};

    #[test]
    fn requests_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<UnitCheckRequest<'static>>();
        assert_send_sync::<UnitCheckRoot>();
    }
}
