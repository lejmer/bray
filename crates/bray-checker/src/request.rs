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
    CheckerSemanticFactProvider, CheckerSource, UnitCheckEntryContext,
};

/// Typed inputs for whole-unit semantic checking.
pub struct UnitCheckRequest<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    view: BoundUnitView<'view>,
    root: UnitCheckRoot,
    entry: &'view UnitCheckEntryContext,
    context: &'view C,
}

impl<C> Clone for UnitCheckRequest<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for UnitCheckRequest<'_, C> where C: CheckerRequestContext + ?Sized {}

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
    /// The entry context does not identify the exact bound unit being checked.
    EntryContextMismatch,
}

impl UnitCheckRoot {
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

impl<'view, C> UnitCheckRequest<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    /// Creates a checker request over one complete canonical bound unit.
    ///
    /// Returns an error when the entry context does not describe that unit.
    pub fn new(
        unit: &'view BoundUnit,
        entry: &'view UnitCheckEntryContext,
        context: &'view C,
    ) -> Result<Self, UnitCheckRequestError> {
        if entry.kind() != unit.key().kind()
            || entry.key() != unit.key()
            || !context.entry_context_matches(unit, entry)
        {
            return Err(UnitCheckRequestError::EntryContextMismatch);
        }

        Ok(Self {
            view: unit.view(),
            root: UnitCheckRoot::from_bound_root(unit.root()),
            entry,
            context,
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

    /// Returns the category-specific semantic inputs active at unit entry.
    pub const fn entry(self) -> &'view UnitCheckEntryContext {
        self.entry
    }

    /// Returns the canonical semantic values referenced by the bound unit.
    pub fn semantic_values(self) -> &'view SemanticValueStore {
        self.context.semantic_values()
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub fn available_compiler_known_symbols(self) -> &'view AvailableCompilerKnownSymbols {
        self.context.available_compiler_known_symbols()
    }

    /// Resolves the exact compiler-known declarations assigned to an operation role.
    pub fn compiler_known_operation_contract(
        self,
        role: crate::CompilerKnownOperationRole,
    ) -> Option<crate::CompilerKnownOperationContract> {
        self.context.compiler_known_operation_contract(role)
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
    use super::{UnitCheckRequest, UnitCheckRoot};
    use crate::test_support::TestCheckerContext;

    #[test]
    fn requests_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<UnitCheckRequest<'static, TestCheckerContext>>();
        assert_send_sync::<UnitCheckRoot>();
    }
}
