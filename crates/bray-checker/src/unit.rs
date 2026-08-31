use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{
    BoundBlockId, BoundCallableBodyId, BoundExpression, BoundExpressionId, BoundUnit, BoundUnitKey,
    BoundUnitRoot, BoundUnitView,
};
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableSymbolId, SemanticValueStore, SymbolGraph,
    SymbolQueryContract, SymbolQueryRequest,
};
use bray_target::TargetProfile;

use crate::{
    CheckerInfrastructureError, CheckerQueryResult, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerSource, ImplementationHookResolution, SemanticUnitContext,
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
            BoundUnitRoot::CallableBody { body, .. }
            | BoundUnitRoot::AnonymousCallable { body, .. } => Self::CallableBody(body),
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

    pub(crate) fn anonymous_callable_unit(self, call: BoundExpressionId) -> Option<BoundUnitKey> {
        let BoundExpression::Call(call) = self.view.expression(call)? else {
            return None;
        };

        let BoundExpression::AnonymousCallable(callable) = self.view.expression(call.callee())?
        else {
            return None;
        };

        // Unit keys own Arc-backed stable identities independently.
        Some(callable.unit().clone())
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

    pub(crate) const fn context(self) -> &'view C {
        self.context
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

    pub(crate) fn lookup_member(
        self,
        owner: bray_symbols::AnySymbolId,
        name: &str,
    ) -> CheckerQueryResult<
        bray_symbols::MemberLookupResult<bray_symbols::AnySymbolId>,
        C::UpstreamError,
    > {
        self.context.lookup_member(owner, name)
    }

    pub(crate) fn member_name(
        self,
        member: bray_symbols::AnySymbolId,
    ) -> CheckerQueryResult<Option<&'view bray_symbols::SymbolName>, C::UpstreamError> {
        self.context.member_name(member)
    }

    pub(crate) fn structure(
        self,
        id: bray_symbols::StructSymbolId,
    ) -> CheckerQueryResult<Option<&'view bray_symbols::StructSymbol>, C::UpstreamError> {
        self.context.structure(id)
    }

    pub(crate) fn union(
        self,
        id: bray_symbols::UnionSymbolId,
    ) -> CheckerQueryResult<Option<&'view bray_symbols::UnionSymbol>, C::UpstreamError> {
        self.context.union(id)
    }

    pub(crate) fn union_variant(
        self,
        id: bray_symbols::UnionVariantSymbolId,
    ) -> CheckerQueryResult<Option<&'view bray_symbols::UnionVariantSymbol>, C::UpstreamError> {
        self.context.union_variant(id)
    }

    pub(crate) fn union_payload_field(
        self,
        id: bray_symbols::UnionPayloadFieldSymbolId,
    ) -> CheckerQueryResult<Option<&'view bray_symbols::UnionPayloadFieldSymbol>, C::UpstreamError>
    {
        self.context.union_payload_field(id)
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub fn available_compiler_known_symbols(self) -> &'view AvailableCompilerKnownSymbols {
        self.context.available_compiler_known_symbols()
    }

    /// Resolves compiler behavior carried by one exact declaration identity.
    pub(crate) fn implementation_hook(
        self,
        symbol: bray_symbols::AnySymbolId,
    ) -> CheckerQueryResult<Option<ImplementationHookResolution>, C::UpstreamError> {
        self.context.implementation_hook(symbol)
    }

    /// Returns the selected language-level target profile.
    pub fn selected_target(self) -> &'view TargetProfile {
        self.context.selected_target()
    }

    /// Checks one source constant expression embedded in a type template.
    pub(crate) fn checked_constant_expression(
        self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<bray_symbols::ConstantTermId>,
        C::UpstreamError,
    > {
        self.context.checked_constant_expression(occurrence)
    }

    /// Proves static constraints for one exact generic declaration instance.
    pub(crate) fn generic_constraints(
        self,
        obligation: bray_symbols::GenericConstraintObligationKey,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<bray_symbols::ProofOutcome>,
        C::UpstreamError,
    > {
        self.context.generic_constraints(obligation)
    }

    pub(crate) fn implementation_selection(
        self,
        requirement: bray_symbols::ImplementationRequirementKey,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<bray_symbols::ImplementationSelection>,
        C::UpstreamError,
    > {
        self.context.implementation_selection(requirement)
    }

    /// Returns the checked representation contract for one declared type.
    pub(crate) fn declared_type_representation(
        self,
        subject: bray_symbols::NamedTypeSymbolId,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<bray_symbols::DeclaredTypeRepresentation>,
        C::UpstreamError,
    > {
        self.context.declared_type_representation(subject)
    }

    pub(crate) fn plain_storage_atomic_representation(
        self,
        ty: bray_symbols::TypeId,
    ) -> CheckerQueryResult<Option<bray_target::TargetAtomicRepresentation>, C::UpstreamError> {
        self.context.plain_storage_atomic_representation(ty)
    }

    pub(crate) fn declared_type_has_lifecycle(
        self,
        subject: bray_symbols::NamedTypeSymbolId,
    ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<bool>, C::UpstreamError> {
        self.context.declared_type_has_lifecycle(subject)
    }

    /// Checks every source constant expression embedded in one type template.
    pub(crate) fn checked_constant_terms(
        self,
        template: &bray_symbols::TypeExpressionTemplate,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<crate::CheckedConstantTerms>,
        C::UpstreamError,
    > {
        let mut terms = BTreeMap::new();
        let mut diagnostics = bray_diagnostics::DiagnosticBag::new();

        for occurrence in template.constant_expressions() {
            let result = self.checked_constant_expression(occurrence)?;

            // The aggregate result owns diagnostics after this dependency result drops.
            diagnostics.add_range(result.diagnostics().clone());
            terms.insert(occurrence.key(), *result.value());
        }

        let checked = crate::CheckedConstantTerms::try_from_terms(terms).map_err(|_| {
            crate::CheckerQueryError::Infrastructure(
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

    /// Resolves one declaration syntax anchor from the current compilation snapshot.
    pub fn source_syntax(
        self,
        anchor: bray_declarations::SyntaxAnchor,
    ) -> Result<CheckerSource<'view>, CheckerInfrastructureError> {
        self.context.source_syntax(anchor)
    }

    /// Requests one exact symbol-owned semantic query.
    pub fn resolve_symbol_query<F>(
        self,
        request: SymbolQueryRequest<F>,
    ) -> CheckerQueryResult<
        Arc<bray_diagnostics::DiagnosticResult<<F as bray_symbols::SymbolQueryContract>::Value>>,
        C::UpstreamError,
    >
    where
        F: SymbolQueryContract,
        C: CheckerSemanticQueryProvider<F>,
    {
        self.context.resolve_symbol_query(request)
    }

    /// Returns whether compilation cancellation has been requested.
    pub fn is_cancelled(self) -> bool {
        self.context.cancellation().is_cancelled()
    }
}

pub(crate) fn semantic_inputs_match<C, const N: usize>(
    request: CheckerUnitView<'_, C>,
    inputs: [(bray_bound_tree::BoundUnitId, bray_bound_tree::BoundUnitKind); N],
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let identity = (request.unit().unit(), request.unit().key().kind());

    inputs.into_iter().all(|input| input == identity)
}

pub(crate) fn expression_block_owners<C>(
    request: CheckerUnitView<'_, C>,
    expressions: impl IntoIterator<Item = BoundExpressionId>,
) -> Result<BTreeMap<BoundBlockId, BoundExpressionId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut owners = BTreeMap::new();

    for expression in expressions {
        if request.is_cancelled() {
            return Ok(owners);
        }

        let Some(bound) = request.view().expression(expression) else {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        };

        for block in bound.child_blocks() {
            owners.insert(block, expression);
        }
    }

    Ok(owners)
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
