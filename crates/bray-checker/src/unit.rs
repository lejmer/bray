use std::sync::Arc;

use bray_bound_tree::{
    BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundUnit, BoundUnitRoot, BoundUnitView,
};
use bray_symbols::{
    AvailableCompilerKnownSymbols, SemanticValueStore, SymbolFactContract, SymbolFactRequest,
    SymbolFactResult,
};

use crate::{
    CheckerFactResult, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerSource, SemanticUnitContext,
};

/// A validated read-only view of one bound unit for focused checker services.
pub struct CheckerUnitView<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    unit: &'view BoundUnit,
    view: BoundUnitView<'view>,
    root: CheckerUnitRoot,
    semantic_context: &'view SemanticUnitContext,
    context: &'view C,
}

impl<C> Clone for CheckerUnitView<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for CheckerUnitView<'_, C> where C: CheckerRequestContext + ?Sized {}

/// The exact root category exposed by a checker unit view.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerUnitRoot {
    /// A declared or anonymous callable body.
    CallableBody(BoundCallableBodyId),
    /// A declaration-owned expression.
    Expression(BoundExpressionId),
    /// An ordered declaration-owned expression sequence.
    ExpressionSequence(BoundBlockId),
}

/// An inconsistency that prevents construction of a checker unit view.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerUnitViewError {
    /// The semantic context does not identify the supplied bound unit.
    SemanticContextMismatch,
}

impl CheckerUnitRoot {
    const fn from_bound_root(root: BoundUnitRoot) -> Self {
        match root {
            BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
                Self::CallableBody(body)
            }
            BoundUnitRoot::Expression(expression) => Self::Expression(expression),
            BoundUnitRoot::ExpressionSequence(block) => Self::ExpressionSequence(block),
        }
    }
}

impl<'view, C> CheckerUnitView<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    /// Creates a read-only checker view over one canonical bound unit.
    ///
    /// Returns an error when the semantic context does not describe that unit.
    pub fn new(
        unit: &'view BoundUnit,
        semantic_context: &'view SemanticUnitContext,
        context: &'view C,
    ) -> Result<Self, CheckerUnitViewError> {
        if semantic_context.kind() != unit.key().kind()
            || semantic_context.key() != unit.key()
            || !context.semantic_context_matches(unit, semantic_context)
        {
            return Err(CheckerUnitViewError::SemanticContextMismatch);
        }

        Ok(Self {
            unit,
            view: unit.view(),
            root: CheckerUnitRoot::from_bound_root(unit.root()),
            semantic_context,
            context,
        })
    }

    /// Returns the read-only bound unit view to analyze.
    pub const fn view(self) -> BoundUnitView<'view> {
        self.view
    }

    /// Returns the canonical bound unit being analyzed.
    pub const fn unit(self) -> &'view BoundUnit {
        self.unit
    }

    /// Returns the exact bound root to analyze.
    pub const fn root(self) -> CheckerUnitRoot {
        self.root
    }

    /// Returns the category-specific semantic inputs active at unit entry.
    pub const fn semantic_context(self) -> &'view SemanticUnitContext {
        self.semantic_context
    }

    /// Returns the canonical semantic values referenced by the bound unit.
    pub fn semantic_values(self) -> &'view SemanticValueStore {
        self.context.semantic_values()
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub fn available_compiler_known_symbols(self) -> &'view AvailableCompilerKnownSymbols {
        self.context.available_compiler_known_symbols()
    }

    /// Resolves a bound source anchor without exposing its source snapshot.
    pub fn source(
        self,
        anchor: bray_bound_tree::BoundSourceAnchor,
    ) -> Result<CheckerSource<'view>, CheckerInfrastructureError> {
        self.context.source(anchor)
    }

    /// Requests one exact symbol-owned semantic fact.
    pub fn symbol_fact<F>(
        self,
        request: SymbolFactRequest<F>,
    ) -> CheckerFactResult<Arc<SymbolFactResult<F>>>
    where
        F: SymbolFactContract,
        C: CheckerSemanticFactProvider<F>,
    {
        self.context.symbol_fact(request)
    }

    /// Returns whether compilation cancellation has been requested.
    pub fn is_cancelled(self) -> bool {
        self.context.cancellation().is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckerUnitRoot, CheckerUnitView};
    use crate::test_support::TestCheckerContext;

    #[test]
    fn views_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckerUnitView<'static, TestCheckerContext>>();
        assert_send_sync::<CheckerUnitRoot>();
    }
}
