use crate::BoundUnitId;

mod sealed {
    pub trait Sealed {}
}

/// Classifies a substantial bound node without identifying a node instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundNodeKind {
    /// A bound expression.
    Expression,
    /// A bound pattern.
    Pattern,
    /// A bound block.
    Block,
    /// A complete bound callable body.
    CallableBody,
}

/// Identifies one exact bound node category at the type level.
///
/// This trait is sealed so heterogeneous infrastructure cannot claim a category that does not
/// correspond to one of Bray's exact typed bound node IDs.
pub trait ExactBoundNodeId: sealed::Sealed + Copy {
    /// The substantial bound node kind represented by this exact ID type.
    const KIND: BoundNodeKind;

    /// Returns the bound unit that owns this node ID.
    fn unit(self) -> BoundUnitId;
}

macro_rules! define_bound_node_ids {
    ($($id:ident => $variant:ident : $kind:ident),+ $(,)?) => {
        $(
            #[doc = concat!("The exact typed ID of a bound `", stringify!($kind), "` node.")]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $id {
                unit: BoundUnitId,
                slot: u32,
            }

            impl $id {
                /// Returns the bound unit that owns this node ID.
                pub const fn unit(self) -> BoundUnitId {
                    self.unit
                }

                /// Returns the stable substantial node category associated with this exact ID.
                pub const fn kind(self) -> BoundNodeKind {
                    BoundNodeKind::$kind
                }

                #[cfg(test)]
                pub(super) const fn from_slot(unit: BoundUnitId, slot: u32) -> Self {
                    Self { unit, slot }
                }
            }

            impl sealed::Sealed for $id {}

            impl ExactBoundNodeId for $id {
                const KIND: BoundNodeKind = BoundNodeKind::$kind;

                fn unit(self) -> BoundUnitId {
                    self.unit()
                }
            }

        )+

        /// A closed type-erased reference to any substantial bound node.
        ///
        /// Exact typed IDs remain the canonical storage and API types. This erasure is intended
        /// for heterogeneous infrastructure such as diagnostics, visitors, and tooling.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum AnyBoundNodeId {
            $(
                #[doc = concat!("A bound `", stringify!($kind), "` node ID.")]
                $variant($id),
            )+
        }

        impl AnyBoundNodeId {
            /// Returns the bound unit that owns this node ID.
            pub const fn unit(self) -> BoundUnitId {
                match self {
                    $(Self::$variant(id) => id.unit(),)+
                }
            }

            /// Returns the exact substantial node kind retained by this erased ID.
            pub const fn kind(self) -> BoundNodeKind {
                match self {
                    $(Self::$variant(id) => id.kind(),)+
                }
            }
        }

        $(
            impl From<$id> for AnyBoundNodeId {
                fn from(id: $id) -> Self {
                    Self::$variant(id)
                }
            }
        )+
    };
}

define_bound_node_ids! {
    BoundExpressionId => Expression: Expression,
    BoundPatternId => Pattern: Pattern,
    BoundBlockId => Block: Block,
    BoundCallableBodyId => CallableBody: CallableBody,
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{
        AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundNodeKind,
        BoundPatternId, ExactBoundNodeId,
    };
    use crate::BoundUnitId;

    #[test]
    fn exact_ids_retain_unit_and_kind_after_erasure() {
        let unit = BoundUnitId::new(7);
        let expression = BoundExpressionId::from_slot(unit, 3);
        let pattern = BoundPatternId::from_slot(unit, 3);
        let expression = AnyBoundNodeId::from(expression);
        let pattern = AnyBoundNodeId::from(pattern);

        assert_eq!(expression.unit(), pattern.unit());
        assert_eq!(expression.kind(), BoundNodeKind::Expression);
        assert_eq!(pattern.kind(), BoundNodeKind::Pattern);
        assert_ne!(expression, pattern);
    }

    #[test]
    fn exact_ids_are_compact_unit_and_slot_pairs() {
        assert_eq!(size_of::<BoundUnitId>(), size_of::<u32>());
        assert_eq!(size_of::<BoundExpressionId>(), size_of::<[u32; 2]>());
    }

    #[test]
    fn checked_access_rejects_foreign_units() {
        let unit = BoundUnitId::new(2);
        let local = BoundBlockId::from_slot(unit, 4);
        let foreign = BoundBlockId::from_slot(BoundUnitId::new(3), 4);

        assert_eq!(unit.checked_node(local), Some(local));
        assert_eq!(unit.checked_node(foreign), None);
    }

    #[test]
    fn exact_id_trait_is_category_specific() {
        let callable_body = BoundCallableBodyId::from_slot(BoundUnitId::new(1), 0);

        assert_eq!(BoundExpressionId::KIND, BoundNodeKind::Expression);
        assert_eq!(BoundPatternId::KIND, BoundNodeKind::Pattern);
        assert_eq!(BoundBlockId::KIND, BoundNodeKind::Block);
        assert_eq!(BoundCallableBodyId::KIND, BoundNodeKind::CallableBody);
        assert_eq!(callable_body.kind(), BoundNodeKind::CallableBody);
    }

    #[test]
    fn node_ids_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<BoundExpressionId>();
        assert_send_sync::<AnyBoundNodeId>();
    }
}
