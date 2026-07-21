use bray_symbols::TypeId;

use crate::{BoundExpressionId, BoundNodeOrigin};

/// Checked facts for composing a `Future<T>` directly into the current run.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundFutureComposition {
    future_type: TypeId,
    completion_type: TypeId,
}

impl BoundFutureComposition {
    /// Creates checked direct-await composition facts.
    pub const fn new(future_type: TypeId, completion_type: TypeId) -> Self {
        Self {
            future_type,
            completion_type,
        }
    }

    /// Returns the consumed compiler-known `Future<T>` type.
    pub const fn future_type(self) -> TypeId {
        self.future_type
    }

    /// Returns the value type produced on normal completion.
    pub const fn completion_type(self) -> TypeId {
        self.completion_type
    }
}

/// Whether the direct-await composition has completed semantic checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundAwaitResolution {
    /// Operand checking and future-type decomposition are still pending.
    Pending,
    /// The operand composes a checked `Future<T>` into the current run.
    Composition(BoundFutureComposition),
}

impl BoundAwaitResolution {
    const fn ty(self) -> Option<TypeId> {
        match self {
            Self::Pending => None,
            Self::Composition(composition) => Some(composition.completion_type()),
        }
    }
}

/// A direct await that composes one lazy future into the current run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundAwaitExpression {
    origin: BoundNodeOrigin,
    operand: BoundExpressionId,
    resolution: BoundAwaitResolution,
    is_recovered: bool,
}

impl BoundAwaitExpression {
    /// Creates a direct await awaiting semantic future-type checking.
    pub const fn pending(
        origin: BoundNodeOrigin,
        operand: BoundExpressionId,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            operand,
            resolution: BoundAwaitResolution::Pending,
            is_recovered,
        }
    }

    /// Creates a checked direct-await composition.
    pub const fn resolved(
        origin: BoundNodeOrigin,
        operand: BoundExpressionId,
        composition: BoundFutureComposition,
    ) -> Self {
        Self {
            origin,
            operand,
            resolution: BoundAwaitResolution::Composition(composition),
            is_recovered: false,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the future expression consumed by direct composition.
    pub const fn operand(self) -> BoundExpressionId {
        self.operand
    }

    /// Returns the pending or completed composition facts.
    pub const fn resolution(self) -> BoundAwaitResolution {
        self.resolution
    }

    /// Returns the completion type once semantic checking has completed.
    pub const fn ty(self) -> Option<TypeId> {
        self.resolution.ty()
    }

    /// Returns whether recovery contributed to this direct await.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        std::slice::from_ref(&self.operand)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::TypeData;

    use super::{BoundAwaitExpression, BoundAwaitResolution, BoundFutureComposition};
    use crate::{BoundExpressionId, BoundUnitId};

    #[test]
    fn direct_await_retains_current_run_composition_types() {
        let values = crate::test_support::semantic_values();
        let completion_type = crate::test_support::error_type_in(&values);

        let future_type = match values.intern_type(TypeData::Slice(completion_type)) {
            Ok(ty) => ty,
            Err(error) => panic!("future test type must be interned: {error:?}"),
        };

        let operand = BoundExpressionId::from_slot(BoundUnitId::new(5), 0);
        let expression = BoundAwaitExpression::resolved(
            crate::BoundNodeOrigin::source(crate::test_support::source_anchor()),
            operand,
            BoundFutureComposition::new(future_type, completion_type),
        );

        assert_eq!(expression.operand(), operand);
        assert_eq!(expression.ty(), Some(completion_type));

        assert_eq!(
            expression.resolution(),
            BoundAwaitResolution::Composition(BoundFutureComposition::new(
                future_type,
                completion_type,
            ))
        );
    }
}
