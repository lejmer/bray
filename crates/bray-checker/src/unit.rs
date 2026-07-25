use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{
    BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundUnit, BoundUnitRoot, BoundUnitView,
};
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableSymbolId, SemanticValueStore, SymbolFactContract,
    SymbolFactRequest, SymbolFactResult, SymbolGraph,
};
use bray_target::TargetProfile;

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

    /// Returns the callable declaration that semantically contains this unit, when any.
    pub(crate) fn containing_callable(self) -> Option<CallableSymbolId> {
        let mut symbol = match self.semantic_context {
            SemanticUnitContext::CallableBody(context)
            | SemanticUnitContext::RuntimeDefault(context)
            | SemanticUnitContext::ConstantTemplate(context)
            | SemanticUnitContext::EmbeddedConstant(context)
            | SemanticUnitContext::PredicateDefinition(context)
            | SemanticUnitContext::Constraint(context)
            | SemanticUnitContext::TargetGate(context) => context.declaration(),
            SemanticUnitContext::ContractClause(context) => context.declaration().declaration(),
            SemanticUnitContext::AnonymousCallable(_) => return None,
        };

        loop {
            if let Some(callable) = CallableSymbolId::try_from_any(symbol) {
                return Some(callable);
            }

            symbol = self.symbols().containing_symbol(symbol)?;
        }
    }

    /// Returns the canonical semantic values referenced by the bound unit.
    pub fn semantic_values(self) -> &'view SemanticValueStore {
        self.context.semantic_values()
    }

    /// Returns the compilation-wide symbol graph.
    pub fn symbols(self) -> &'view SymbolGraph {
        self.context.symbols()
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub fn available_compiler_known_symbols(self) -> &'view AvailableCompilerKnownSymbols {
        self.context.available_compiler_known_symbols()
    }

    /// Returns the selected language-level target profile.
    pub fn selected_target(self) -> &'view TargetProfile {
        self.context.selected_target()
    }

    /// Checks one source constant expression embedded in a type template.
    pub(crate) fn checked_constant_expression(
        self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerFactResult<bray_diagnostics::DiagnosticResult<bray_symbols::ConstantTermId>> {
        self.context.checked_constant_expression(occurrence)
    }

    /// Proves static constraints for one exact generic declaration instance.
    pub(crate) fn generic_constraints(
        self,
        obligation: bray_symbols::GenericConstraintObligationKey,
    ) -> CheckerFactResult<bray_diagnostics::DiagnosticResult<bray_symbols::ProofOutcome>> {
        self.context.generic_constraints(obligation)
    }

    /// Returns the checked representation contract for one declared type.
    pub(crate) fn declared_type_representation(
        self,
        subject: bray_symbols::NamedTypeSymbolId,
    ) -> CheckerFactResult<
        bray_diagnostics::DiagnosticResult<bray_symbols::DeclaredTypeRepresentation>,
    > {
        self.context.declared_type_representation(subject)
    }

    /// Checks every source constant expression embedded in one type template.
    pub(crate) fn checked_constant_terms(
        self,
        template: &bray_symbols::TypeExpressionTemplate,
    ) -> CheckerFactResult<bray_diagnostics::DiagnosticResult<crate::CheckedConstantTerms>> {
        let mut terms = BTreeMap::new();
        let mut diagnostics = bray_diagnostics::DiagnosticBag::new();

        for occurrence in template.constant_expressions() {
            let result = self.checked_constant_expression(occurrence)?;

            // The aggregate result owns diagnostics after this dependency result drops.
            diagnostics.add_range(result.diagnostics().clone());
            terms.insert(occurrence.key(), *result.value());
        }

        let checked = crate::CheckedConstantTerms::try_from_terms(terms).map_err(|_| {
            crate::CheckerFactError::Infrastructure(
                CheckerInfrastructureError::SemanticValueUnavailable,
            )
        })?;

        Ok(bray_diagnostics::DiagnosticResult::new(
            checked,
            diagnostics,
        ))
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
