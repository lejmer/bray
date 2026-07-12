use std::sync::Arc;

use bray_binder::{
    BinderFactContext, CheckedUnitBindingError, bind_anonymous_callable, bind_callable_body,
    bind_constant_template, bind_constraint, bind_contract_clause, bind_predicate_definition,
    bind_runtime_default,
};
use bray_bound_tree::{
    BoundUnitKey, CheckedAnonymousCallable, CheckedCallableBody, CheckedConstantTemplateUnit,
    CheckedConstraintUnit, CheckedContractClauseUnit, CheckedPredicateDefinitionUnit,
    CheckedRuntimeDefaultUnit,
};
use bray_checker::DefaultControlFlowChecker;
use bray_declarations::DeclarationTable;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{SemanticValueStore, SymbolGraph};
use bray_syntax::SyntaxTree;

use super::Compilation;
use crate::fact::{CancellationToken, FactQueryError, PublishedCheckedUnit};

macro_rules! define_checked_expression_query {
    ($method:ident, $bind:ident, $result:ty) => {
        #[doc = concat!("Returns one lazily bound and checked `", stringify!($result), "`.")]
        pub fn $method<C>(
            &self,
            facts: &C,
            key: BoundUnitKey,
        ) -> Result<Arc<DiagnosticResult<$result>>, FactQueryError>
        where
            C: BinderFactContext + ?Sized,
        {
            let cancellation = &self.state.cancellation;

            // The cache, binder transaction, and nested dependency records share Arc-backed keys.
            let published = self.checked_unit(key.clone(), cancellation, |cancellation| {
                let facts = QueryBinderFacts::new(facts, cancellation);
                let unit = self.state.checked_units.unit_id(&key)?;
                let pending = $bind(&facts, unit, key.clone()).map_err(map_binding_error)?;

                for nested in pending.nested_units() {
                    self.checked_anonymous_callable(facts.base(), nested.clone(), cancellation)?;
                }

                pending
                    .finish(&DefaultControlFlowChecker, cancellation)
                    .map_err(map_binding_error)
            })?;

            Ok(Arc::clone(published.result()))
        }
    };
}

impl Compilation {
    /// Returns one lazily bound and checked declared callable body.
    pub fn checked_callable_body<C>(
        &self,
        facts: &C,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedCallableBody>>, FactQueryError>
    where
        C: BinderFactContext + ?Sized,
    {
        let cancellation = &self.state.cancellation;

        // The cache, binder transaction, and nested dependency records share Arc-backed keys.
        let published = self.checked_unit(key.clone(), cancellation, |cancellation| {
            let facts = QueryBinderFacts::new(facts, cancellation);
            let unit = self.state.checked_units.unit_id(&key)?;
            let pending =
                bind_callable_body(&facts, unit, key.clone()).map_err(map_binding_error)?;

            for nested in pending.nested_units() {
                self.checked_anonymous_callable(facts.base(), nested.clone(), cancellation)?;
            }

            pending
                .finish(&DefaultControlFlowChecker, cancellation)
                .map_err(map_binding_error)
        })?;

        Ok(Arc::clone(published.result()))
    }

    define_checked_expression_query!(
        checked_runtime_default,
        bind_runtime_default,
        CheckedRuntimeDefaultUnit
    );
    define_checked_expression_query!(
        checked_constant_template,
        bind_constant_template,
        CheckedConstantTemplateUnit
    );
    define_checked_expression_query!(
        checked_predicate_definition,
        bind_predicate_definition,
        CheckedPredicateDefinitionUnit
    );
    define_checked_expression_query!(checked_constraint, bind_constraint, CheckedConstraintUnit);
    define_checked_expression_query!(
        checked_contract_clause,
        bind_contract_clause,
        CheckedContractClauseUnit
    );

    pub(super) fn checked_anonymous_callable<C>(
        &self,
        facts: &C,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedCheckedUnit<CheckedAnonymousCallable>>, FactQueryError>
    where
        C: BinderFactContext + ?Sized,
    {
        // The cache, binder transaction, and nested dependency records share Arc-backed keys.
        self.checked_unit(key.clone(), cancellation, |cancellation| {
            let facts = QueryBinderFacts::new(facts, cancellation);
            let unit = self.state.checked_units.unit_id(&key)?;
            let pending =
                bind_anonymous_callable(&facts, unit, key.clone()).map_err(map_binding_error)?;

            for nested in pending.nested_units() {
                self.checked_anonymous_callable(facts.base(), nested.clone(), cancellation)?;
            }

            pending
                .finish(&DefaultControlFlowChecker, cancellation)
                .map_err(map_binding_error)
        })
    }
}

struct QueryBinderFacts<'facts, C: BinderFactContext + ?Sized> {
    base: &'facts C,
    cancellation: &'facts CancellationToken,
}

impl<'facts, C> QueryBinderFacts<'facts, C>
where
    C: BinderFactContext + ?Sized,
{
    const fn new(base: &'facts C, cancellation: &'facts CancellationToken) -> Self {
        Self { base, cancellation }
    }

    const fn base(&self) -> &'facts C {
        self.base
    }
}

impl<C> BinderFactContext for QueryBinderFacts<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    type TargetFacts = C::TargetFacts;
    type SymbolFacts = C::SymbolFacts;
    type Cancellation = CancellationToken;

    fn syntax(&self) -> &SyntaxTree {
        self.base.syntax()
    }

    fn declarations(&self) -> &DeclarationTable {
        self.base.declarations()
    }

    fn symbols(&self) -> &SymbolGraph {
        self.base.symbols()
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.base.semantic_values()
    }

    fn target_facts(&self) -> &Self::TargetFacts {
        self.base.target_facts()
    }

    fn symbol_facts(&self) -> &Self::SymbolFacts {
        self.base.symbol_facts()
    }

    fn cancellation(&self) -> &Self::Cancellation {
        self.cancellation
    }
}

const fn map_binding_error(error: CheckedUnitBindingError) -> FactQueryError {
    match error {
        CheckedUnitBindingError::Cancelled => FactQueryError::Cancelled,
        CheckedUnitBindingError::InvalidUnitKey
        | CheckedUnitBindingError::MissingSyntax
        | CheckedUnitBindingError::MissingOwner
        | CheckedUnitBindingError::MissingModule
        | CheckedUnitBindingError::SemanticValue(_)
        | CheckedUnitBindingError::Construction
        | CheckedUnitBindingError::Binding
        | CheckedUnitBindingError::Assembly => FactQueryError::InfrastructureFailure,
    }
}
