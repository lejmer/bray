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

impl BoundNodeKind {
    /// Returns this bound-node kind's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expression => "expression",
            Self::Pattern => "pattern",
            Self::Block => "block",
            Self::CallableBody => "callable_body",
        }
    }
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

                /// Returns the node's unit-local arena ordinal.
                pub const fn ordinal(self) -> u32 {
                    self.slot
                }

                pub(crate) const fn from_slot(unit: BoundUnitId, slot: u32) -> Self {
                    Self { unit, slot }
                }

                pub(crate) fn to_index(self) -> Option<usize> {
                    usize::try_from(self.slot).ok()
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
        /// Exact typed IDs remain the preferred API types. This erasure is intended for
        /// heterogeneous infrastructure such as diagnostics, visitors, and tooling.
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

            /// Returns the node's unit-local arena ordinal.
            pub const fn ordinal(self) -> u32 {
                match self {
                    $(Self::$variant(id) => id.ordinal(),)+
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

/// A source-correlated execution site used to key checked contracts and storage plans.
///
/// Ordinary operations use their bound node identity. A `with` expression also owns
/// separate implicit `enter` and `exit` calls, which must retain distinct checked inputs
/// and results despite sharing the same source node. Lexical tree traversal continues
/// to use `BoundWalkEvent`, and control-flow analysis identifies individual executions
/// of these sites separately.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundExecutionSite {
    /// An explicit source operation, binding or scope boundary.
    Node(AnyBoundNodeId),
    /// The implicit lifecycle entry selected by a with expression.
    ScopedEnter(BoundExpressionId),
    /// The implicit lifecycle exit paired with a successful scoped entry.
    ScopedExit(BoundExpressionId),
}

impl BoundExecutionSite {
    /// Returns the bound node owning this execution site.
    pub const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::Node(node) => node,
            Self::ScopedEnter(expression) | Self::ScopedExit(expression) => {
                AnyBoundNodeId::Expression(expression)
            }
        }
    }

    /// Returns the owning expression when this execution site belongs to an expression.
    pub const fn expression(self) -> Option<BoundExpressionId> {
        match self.node() {
            AnyBoundNodeId::Expression(expression) => Some(expression),
            _ => None,
        }
    }
}

impl From<AnyBoundNodeId> for BoundExecutionSite {
    fn from(node: AnyBoundNodeId) -> Self {
        Self::Node(node)
    }
}

impl From<BoundExpressionId> for BoundExecutionSite {
    fn from(expression: BoundExpressionId) -> Self {
        Self::Node(expression.into())
    }
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
