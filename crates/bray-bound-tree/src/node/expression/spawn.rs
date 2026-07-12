use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::TypeId;

use crate::{BoundArgument, BoundExpressionId, BoundNodeOrigin};

/// The exact execution mode of a spawn expression.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundSpawnMode {
    /// Spawn a task in the current task scope.
    Task,
    /// Spawn a detached task.
    DetachedTask,
    /// Spawn an operating-system thread call.
    Thread,
}

/// The exact input shape of a spawn expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundSpawnInput {
    /// A task expression.
    Task(BoundExpressionId),
    /// A thread callee and its ordered arguments.
    Thread {
        /// The thread entry callable expression.
        callee: BoundExpressionId,
        /// Arguments in source order.
        arguments: Arc<[BoundArgument]>,
    },
}

/// A task or thread spawn expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundSpawnExpression {
    origin: BoundNodeOrigin,
    mode: BoundSpawnMode,
    input: BoundSpawnInput,
    operands: Arc<[BoundExpressionId]>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundSpawnExpression {
    /// Creates a spawn expression.
    pub fn new(
        origin: BoundNodeOrigin,
        mode: BoundSpawnMode,
        input: BoundSpawnInput,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        let operands = match &input {
            BoundSpawnInput::Task(expression) => shared_slice([*expression]),
            BoundSpawnInput::Thread { callee, arguments } => shared_slice(
                std::iter::once(*callee).chain(arguments.iter().map(BoundArgument::expression)),
            ),
        };

        Self {
            origin,
            mode,
            input,
            operands,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the exact execution mode.
    pub const fn mode(&self) -> BoundSpawnMode {
        self.mode
    }

    /// Returns the exact spawn input.
    pub const fn input(&self) -> &BoundSpawnInput {
        &self.input
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this spawn.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}
