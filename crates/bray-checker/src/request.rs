use bray_bound_tree::{
    AnyBoundNodeId, BoundCallableBodyId, BoundExpressionId, BoundUnitKind, BoundUnitView,
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
                    | BoundUnitKind::Constraint
                    | BoundUnitKind::ContractClause
            )
        )
    }

    pub(crate) const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::CallableBody(root) => AnyBoundNodeId::CallableBody(root),
            Self::Expression(root) => AnyBoundNodeId::Expression(root),
        }
    }
}

impl<'view> UnitCheckRequest<'view> {
    /// Creates a checker request over committed read-only bound structure.
    pub const fn new(
        view: BoundUnitView<'view>,
        root: UnitCheckRoot,
        cancellation: &'view dyn CheckerCancellation,
    ) -> Self {
        Self {
            view,
            root,
            cancellation,
        }
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
