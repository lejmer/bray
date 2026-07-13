use std::sync::Arc;

use bray_binder::{
    BinderDependency, BoundUnitBindingError, BoundUnitComputation, bind_anonymous_callable,
    bind_callable_body, bind_constant_template, bind_constraint, bind_contract_clause,
    bind_predicate_definition, bind_runtime_default,
};
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, BoundUnitKind, BoundUnitRoot, CheckedControlFlowFacts,
};
use bray_checker::{
    CheckerCancellation, CheckerOutcome, ControlFlowChecker, DefaultControlFlowChecker,
    UnitCheckRequest, UnitCheckRoot,
};
use bray_diagnostics::DiagnosticResult;

use super::Compilation;
use super::binder::CompilationBinderFacts;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

impl Compilation {
    /// Returns one lazily bound immutable semantic unit.
    pub fn bound_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<BoundUnit>>, FactQueryError> {
        let published = self.bound_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns durable control-flow facts for one lazily bound semantic unit.
    pub fn checked_control_flow(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedControlFlowFacts>>, FactQueryError> {
        let published =
            self.checked_control_flow_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    fn bound_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<BoundUnit>>, FactQueryError> {
        let facts = self.binder_facts_for(&key, cancellation)?;

        self.unit_fact(
            &self.state.bound_units,
            CompilationFactKey::BoundUnit(key.clone()),
            key.clone(),
            cancellation,
            |_| {
                let unit = self.bound_unit_id(&key)?;

                bind_unit(&facts, unit, key)
                    .map(BoundUnitComputation::into_parts)
                    .map_err(map_binding_error)
            },
        )
    }

    fn checked_control_flow_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedControlFlowFacts>>, FactQueryError> {
        self.unit_fact(
            &self.state.checked_control_flow,
            CompilationFactKey::CheckedControlFlow(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                for nested in bound.result().value().nested_units() {
                    self.checked_control_flow_with_cancellation(nested.clone(), cancellation)?;
                }

                check_control_flow(bound.result().value(), cancellation)
            },
        )
    }
}

fn bind_unit(
    facts: &CompilationBinderFacts<'_>,
    unit: bray_bound_tree::BoundUnitId,
    key: BoundUnitKey,
) -> Result<BoundUnitComputation, BoundUnitBindingError> {
    match key.kind() {
        BoundUnitKind::CallableBody => bind_callable_body(facts, unit, key)?.finish(),
        BoundUnitKind::AnonymousCallable => bind_anonymous_callable(facts, unit, key)?.finish(),
        BoundUnitKind::RuntimeDefault => bind_runtime_default(facts, unit, key)?.finish(),
        BoundUnitKind::ConstantTemplate => bind_constant_template(facts, unit, key)?.finish(),
        BoundUnitKind::PredicateDefinition => bind_predicate_definition(facts, unit, key)?.finish(),
        BoundUnitKind::Constraint => bind_constraint(facts, unit, key)?.finish(),
        BoundUnitKind::ContractClause => bind_contract_clause(facts, unit, key)?.finish(),
    }
}

fn check_control_flow(
    bound: &BoundUnit,
    cancellation: &CancellationToken,
) -> Result<
    (
        DiagnosticResult<CheckedControlFlowFacts>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let root = match bound.root() {
        BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
            UnitCheckRoot::CallableBody(body)
        }
        BoundUnitRoot::Expression(expression) => UnitCheckRoot::Expression(expression),
        BoundUnitRoot::ExpressionSequence(block) => UnitCheckRoot::ExpressionSequence(block),
    };

    let bridge = CheckerCancellationBridge(cancellation);
    let request = UnitCheckRequest::new(bound.view(), root, &bridge)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let result = match DefaultControlFlowChecker.check_control_flow(request) {
        CheckerOutcome::Complete(result) => result.map(|result| result.into_facts()),
        CheckerOutcome::Cancelled => return Err(FactQueryError::Cancelled),
    };

    Ok((result, Box::new([])))
}

const fn map_binding_error(error: BoundUnitBindingError) -> FactQueryError {
    match error {
        BoundUnitBindingError::Cancelled => FactQueryError::Cancelled,
        BoundUnitBindingError::InvalidUnitKey
        | BoundUnitBindingError::MissingSyntax
        | BoundUnitBindingError::MissingOwner
        | BoundUnitBindingError::MissingModule
        | BoundUnitBindingError::SemanticValue(_)
        | BoundUnitBindingError::Construction
        | BoundUnitBindingError::Binding
        | BoundUnitBindingError::Assembly => FactQueryError::InfrastructureFailure,
    }
}

struct CheckerCancellationBridge<'cancellation>(&'cancellation CancellationToken);

impl CheckerCancellation for CheckerCancellationBridge<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}
