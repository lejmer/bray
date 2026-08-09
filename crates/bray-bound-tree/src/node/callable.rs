use crate::{BoundBlockId, BoundNodeOrigin};

/// A bound callable body or its category-specific recovery form.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundCallableBody {
    origin: BoundNodeOrigin,
    kind: BoundCallableBodyKind,
}

impl BoundCallableBody {
    /// Creates a checked callable body with its root block.
    pub const fn block(origin: BoundNodeOrigin, block: BoundBlockId) -> Self {
        Self {
            origin,
            kind: BoundCallableBodyKind::Block(block),
        }
    }

    /// Creates a callable-body recovery node.
    pub const fn error(origin: BoundNodeOrigin, body: Option<BoundBlockId>) -> Self {
        Self {
            origin,
            kind: BoundCallableBodyKind::Error(BoundErrorCallableBody::new(body)),
        }
    }

    /// Returns the source or synthesized origin of this callable body.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact body category.
    pub const fn kind(self) -> BoundCallableBodyKind {
        self.kind
    }

    pub(crate) const fn block_id(self) -> Option<BoundBlockId> {
        match self.kind {
            BoundCallableBodyKind::Block(block) => Some(block),
            BoundCallableBodyKind::Error(error) => error.body(),
        }
    }
}

/// The exact semantic form of a callable body.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundCallableBodyKind {
    /// A successfully checked block body.
    Block(BoundBlockId),
    /// A body preserved after semantic recovery.
    Error(BoundErrorCallableBody),
}

/// A callable body preserved after semantic recovery.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BoundErrorCallableBody {
    body: Option<BoundBlockId>,
}

impl BoundErrorCallableBody {
    /// Creates a recovered callable body, retaining a recovered block when available.
    pub const fn new(body: Option<BoundBlockId>) -> Self {
        Self { body }
    }

    /// Returns the recovered block when binding could preserve one.
    pub const fn body(self) -> Option<BoundBlockId> {
        self.body
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::source_anchor;
    use crate::{
        BoundBlockId, BoundCallableBody, BoundCallableBodyKind, BoundNodeOrigin, BoundUnitId,
    };

    #[test]
    fn error_callable_bodies_retain_recovered_blocks() {
        let source = source_anchor();
        let block = BoundBlockId::from_slot(BoundUnitId::new(4), 2);
        let body = BoundCallableBody::error(BoundNodeOrigin::source(source), Some(block));

        assert_eq!(body.origin().source_anchor(), source);

        assert_eq!(
            body.kind(),
            BoundCallableBodyKind::Error(crate::BoundErrorCallableBody::new(Some(block)))
        );
    }
}
