use crate::{BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundUnitId};

/// The exact checked destination of one source control transfer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedControlTransferTarget {
    /// The callable execution scope exited by `return`.
    Callable(BoundCallableBodyId),
    /// The single-yield block result supplied by `yield`.
    Block(BoundBlockId),
    /// The loop expression exited or continued by the transfer.
    Loop(BoundExpressionId),
    /// The iteration expression exited, continued, or supplied by the transfer.
    Iteration(BoundExpressionId),
    /// Earlier recovery prevented a sound target decision.
    Recovered,
}

/// The checked target of one exact control-transfer expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedControlTransfer {
    expression: BoundExpressionId,
    target: CheckedControlTransferTarget,
}

impl CheckedControlTransfer {
    /// Creates a durable target fact for one control-transfer expression.
    pub const fn new(expression: BoundExpressionId, target: CheckedControlTransferTarget) -> Self {
        Self { expression, target }
    }

    /// Returns the source control-transfer expression.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the exact checked target or explicit recovery state.
    pub const fn target(self) -> CheckedControlTransferTarget {
        self.target
    }

    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        self.expression.unit() == unit && self.target.is_valid_for(unit)
    }
}

impl CheckedControlTransferTarget {
    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Callable(body) => body.unit() == unit,
            Self::Block(block) => block.unit() == unit,
            Self::Loop(expression) | Self::Iteration(expression) => expression.unit() == unit,
            Self::Recovered => true,
        }
    }
}

/// How normal completion of one checked block contributes to enclosing control flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedBlockResultRole {
    /// The block completes as an ordinary unit-valued lexical sequence.
    Unit,
    /// The block is the body of one callable execution scope.
    CallableBody(BoundCallableBodyId),
    /// The block contributes the result of one enclosing expression.
    Expression(BoundExpressionId),
    /// Reaching the end of the block starts the next loop iteration.
    LoopBody(BoundExpressionId),
    /// Reaching the end of the block advances one selected iteration protocol.
    IterationBody(BoundExpressionId),
    /// The block is the multi-yield body of one generator iteration.
    GeneratorBody(BoundExpressionId),
    /// Earlier recovery prevented a sound block-result decision.
    Recovered,
}

/// The checked result role of one exact source-shaped block.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedBlockResult {
    block: BoundBlockId,
    role: CheckedBlockResultRole,
}

impl CheckedBlockResult {
    /// Creates a durable result-role fact for one block.
    pub const fn new(block: BoundBlockId, role: CheckedBlockResultRole) -> Self {
        Self { block, role }
    }

    /// Returns the exact block described by this fact.
    pub const fn block(self) -> BoundBlockId {
        self.block
    }

    /// Returns how normal completion contributes to enclosing control flow.
    pub const fn role(self) -> CheckedBlockResultRole {
        self.role
    }

    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        self.block.unit() == unit && self.role.is_valid_for(unit)
    }
}

impl CheckedBlockResultRole {
    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::CallableBody(body) => body.unit() == unit,
            Self::Expression(expression)
            | Self::LoopBody(expression)
            | Self::IterationBody(expression)
            | Self::GeneratorBody(expression) => expression.unit() == unit,
            Self::Unit | Self::Recovered => true,
        }
    }
}
