use crate::BoundExpressionId;

/// How an iteration expression accesses its source value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IterationSourceMode {
    /// Observe the source through shared access.
    Shared,
    /// Iterate with exclusive mutable access.
    Mutable,
    /// Consume the source value.
    Move,
}

impl IterationSourceMode {
    /// Returns this iteration source mode's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Mutable => "mutable",
            Self::Move => "move",
        }
    }
}

/// One bound iteration source expression and its selected access mode.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundIterationSource {
    expression: BoundExpressionId,
    mode: IterationSourceMode,
}

impl BoundIterationSource {
    /// Creates one bound iteration source relationship.
    pub const fn new(expression: BoundExpressionId, mode: IterationSourceMode) -> Self {
        Self { expression, mode }
    }

    /// Returns the source expression.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns how the source is accessed.
    pub const fn mode(self) -> IterationSourceMode {
        self.mode
    }
}
