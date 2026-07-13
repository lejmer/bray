use std::sync::Arc;

use bray_binder::{
    CheckedUnitBindingError, bind_anonymous_callable, bind_callable_body, bind_constant_template,
    bind_constraint, bind_contract_clause, bind_predicate_definition, bind_runtime_default,
};
use bray_bound_tree::{
    BoundUnitKey, ControlFlowCheckedAnonymousCallable, ControlFlowCheckedCallableBody,
    ControlFlowCheckedConstantTemplateUnit, ControlFlowCheckedConstraintUnit,
    ControlFlowCheckedContractClauseUnit, ControlFlowCheckedPredicateDefinitionUnit,
    ControlFlowCheckedRuntimeDefaultUnit,
};
use bray_checker::DefaultControlFlowChecker;
use bray_diagnostics::DiagnosticResult;

use super::Compilation;
use super::binder::CompilationBinderFacts;
use crate::fact::{CancellationToken, FactQueryError, PublishedUnit};

macro_rules! define_control_flow_query {
    ($method:ident, $bind:ident, $result:ty) => {
        #[doc = concat!(
                                            "Returns one lazily bound and control-flow-checked `",
                                            stringify!($result),
                                            "`."
                                        )]
        pub fn $method(
            &self,
            key: BoundUnitKey,
        ) -> Result<Arc<DiagnosticResult<$result>>, FactQueryError> {
            let cancellation = &self.state.cancellation;
            let facts = self.binder_facts_for(&key, cancellation)?;

            // The cache, binder transaction, and nested dependency records share Arc-backed keys.
            let published = self.control_flow_unit(key.clone(), cancellation, |cancellation| {
                let unit = self.bound_unit_id(&key)?;
                let pending = $bind(&facts, unit, key.clone()).map_err(map_binding_error)?;

                for nested in pending.nested_units() {
                    self.control_flow_checked_anonymous_callable(
                        &facts,
                        nested.clone(),
                        cancellation,
                    )?;
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
    define_control_flow_query!(
        control_flow_checked_callable_body,
        bind_callable_body,
        ControlFlowCheckedCallableBody
    );
    define_control_flow_query!(
        control_flow_checked_runtime_default,
        bind_runtime_default,
        ControlFlowCheckedRuntimeDefaultUnit
    );
    define_control_flow_query!(
        control_flow_checked_constant_template,
        bind_constant_template,
        ControlFlowCheckedConstantTemplateUnit
    );
    define_control_flow_query!(
        control_flow_checked_predicate_definition,
        bind_predicate_definition,
        ControlFlowCheckedPredicateDefinitionUnit
    );
    define_control_flow_query!(
        control_flow_checked_constraint,
        bind_constraint,
        ControlFlowCheckedConstraintUnit
    );
    define_control_flow_query!(
        control_flow_checked_contract_clause,
        bind_contract_clause,
        ControlFlowCheckedContractClauseUnit
    );

    pub(super) fn control_flow_checked_anonymous_callable(
        &self,
        facts: &CompilationBinderFacts<'_>,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnit<ControlFlowCheckedAnonymousCallable>>, FactQueryError> {
        // The cache, binder transaction, and nested dependency records share Arc-backed keys.
        self.control_flow_unit(key.clone(), cancellation, |cancellation| {
            let unit = self.bound_unit_id(&key)?;
            let pending =
                bind_anonymous_callable(facts, unit, key.clone()).map_err(map_binding_error)?;

            for nested in pending.nested_units() {
                self.control_flow_checked_anonymous_callable(facts, nested.clone(), cancellation)?;
            }

            pending
                .finish(&DefaultControlFlowChecker, cancellation)
                .map_err(map_binding_error)
        })
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
