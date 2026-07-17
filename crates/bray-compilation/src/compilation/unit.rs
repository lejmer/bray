use std::sync::Arc;

use bray_binder::{
    BinderDependency, BoundUnitBindingError, BoundUnitComputation, bind_anonymous_callable,
    bind_callable_body, bind_constant_template, bind_constraint, bind_contract_clause,
    bind_predicate_definition, bind_runtime_default,
};
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, BoundUnitKind, BoundUnitRoot, CheckedControlFlowFacts,
    CheckedStorageFacts,
};
use bray_checker::{
    CheckerOutcome, ControlFlowChecker, DefaultControlFlowChecker, DefaultStorageChecker,
    StorageCheckError, StorageChecker, UnitCheckRequest, UnitCheckRoot,
};
use bray_diagnostics::DiagnosticResult;

use super::Compilation;
use super::binder::CompilationBinderFacts;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

impl Compilation {
    /// Returns one bound semantic unit and its diagnostics.
    pub fn bound_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<BoundUnit>>, FactQueryError> {
        let published = self.bound_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns the control-flow facts and diagnostics for one bound semantic unit.
    pub fn checked_control_flow(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedControlFlowFacts>>, FactQueryError> {
        let published =
            self.checked_control_flow_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns complete storage facts and diagnostics for one bound semantic unit.
    pub fn checked_storage(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedStorageFacts>>, FactQueryError> {
        let published = self.checked_storage_with_cancellation(key, &self.state.cancellation)?;

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

                let semantic_values = self.semantic_value_store()?;
                let available_compiler_known_symbols = self.available_compiler_known_symbols();

                check_control_flow(
                    bound.result().value(),
                    semantic_values,
                    available_compiler_known_symbols,
                    cancellation,
                )
            },
        )
    }

    fn checked_storage_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<CheckedStorageFacts>>, FactQueryError> {
        // Bound-unit keys are Arc-backed immutable identities shared by fact dependencies.
        self.unit_fact(
            &self.state.checked_storage,
            CompilationFactKey::CheckedStorage(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                self.checked_control_flow_with_cancellation(key.clone(), cancellation)?;

                for nested in bound.result().value().nested_units() {
                    self.checked_storage_with_cancellation(nested.clone(), cancellation)?;
                }

                let semantic_values = self.semantic_value_store()?;
                let available_compiler_known_symbols = self.available_compiler_known_symbols();

                check_storage(
                    bound.result().value(),
                    semantic_values,
                    available_compiler_known_symbols,
                    cancellation,
                )
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
    semantic_values: &bray_symbols::SemanticValueStore,
    available_compiler_known_symbols: &bray_symbols::AvailableCompilerKnownSymbols,
    cancellation: &CancellationToken,
) -> Result<
    (
        DiagnosticResult<CheckedControlFlowFacts>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let request = UnitCheckRequest::new(
        bound.view(),
        unit_check_root(bound),
        semantic_values,
        available_compiler_known_symbols,
        cancellation,
    )
    .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let result = match DefaultControlFlowChecker.check_control_flow(request) {
        CheckerOutcome::Complete(result) => result.map(|result| result.into_facts()),
        CheckerOutcome::Cancelled => return Err(FactQueryError::Cancelled),
    };

    Ok((result, Box::new([])))
}

fn check_storage(
    bound: &BoundUnit,
    semantic_values: &bray_symbols::SemanticValueStore,
    available_compiler_known_symbols: &bray_symbols::AvailableCompilerKnownSymbols,
    cancellation: &CancellationToken,
) -> Result<
    (
        DiagnosticResult<CheckedStorageFacts>,
        Box<[BinderDependency]>,
    ),
    FactQueryError,
> {
    let root = unit_check_root(bound);
    let request = UnitCheckRequest::new(
        bound.view(),
        root,
        semantic_values,
        available_compiler_known_symbols,
        cancellation,
    )
    .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let result = match DefaultStorageChecker.check_storage(request) {
        Ok(CheckerOutcome::Complete(result)) => result.map(|result| result.into_facts()),
        Ok(CheckerOutcome::Cancelled) => return Err(FactQueryError::Cancelled),
        Err(error) => return Err(map_storage_check_error(error)),
    };

    Ok((result, Box::new([])))
}

const fn unit_check_root(bound: &BoundUnit) -> UnitCheckRoot {
    match bound.root() {
        BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
            UnitCheckRoot::CallableBody(body)
        }
        BoundUnitRoot::Expression(expression) => UnitCheckRoot::Expression(expression),
        BoundUnitRoot::ExpressionSequence(block) => UnitCheckRoot::ExpressionSequence(block),
    }
}

const fn map_storage_check_error(_error: StorageCheckError) -> FactQueryError {
    FactQueryError::InfrastructureFailure
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Compilation;
    use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError};
    use crate::test_support::{FactTestGate, compilation, source_callable_body_key};

    #[test]
    fn repeated_and_concurrent_requests_share_production_semantic_facts() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let first_bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("first bound-unit request must complete: {error:?}"),
        };
        let second_bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("repeated bound-unit request must complete: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first_bound, &second_bound));

        let gate = FactTestGate::holding(FactCellTestEvent::Computing);

        if let Err(error) = compilation
            .state
            .checked_control_flow
            .set_test_observer(&key, gate.observer())
        {
            panic!("control-flow fact must accept a test observer: {error:?}");
        }

        // Bound-unit keys are Arc-backed immutable identities shared by concurrent requests.
        let checked = std::thread::scope(|scope| {
            let compilation = &compilation;
            let owner_key = key.clone();
            let owner = scope.spawn(move || compilation.checked_control_flow(owner_key));

            gate.wait_until_observed(FactCellTestEvent::Computing, 1);

            let waiter_key = key.clone();
            let waiter = scope.spawn(move || compilation.checked_control_flow(waiter_key));

            gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
            gate.release();

            [owner, waiter].map(|handle| match handle.join() {
                Ok(Ok(checked)) => checked,
                Ok(Err(error)) => panic!("concurrent semantic request failed: {error:?}"),
                Err(_) => panic!("concurrent semantic request panicked"),
            })
        });

        assert!(
            checked
                .iter()
                .skip(1)
                .all(|fact| Arc::ptr_eq(&checked[0], fact))
        );
    }

    #[test]
    fn cancelled_production_queries_publish_nothing_and_can_be_retried() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);
        let bound_cancellation = CancellationToken::new();
        let bound_gate = FactTestGate::holding(FactCellTestEvent::Computed);

        if let Err(error) = compilation
            .state
            .bound_units
            .set_test_observer(&key, bound_gate.observer())
        {
            panic!("bound-unit fact must accept a test observer: {error:?}");
        }

        let bound = std::thread::scope(|scope| {
            let request_key = key.clone();
            let request = scope.spawn(|| {
                compilation.bound_unit_with_cancellation(request_key, &bound_cancellation)
            });

            bound_gate.wait_until_observed(FactCellTestEvent::Computed, 1);
            bound_cancellation.cancel();
            bound_gate.release();

            match request.join() {
                Ok(result) => result,
                Err(_) => panic!("cancelled bound-unit request panicked"),
            }
        });

        assert!(matches!(bound, Err(FactQueryError::Cancelled)));
        assert_eq!(compilation.state.bound_units.is_published(&key), Ok(false));

        let bound = compilation.bound_unit(key.clone());

        assert!(bound.is_ok());

        let checked_cancellation = CancellationToken::new();
        let checked_gate = FactTestGate::holding(FactCellTestEvent::Computed);

        if let Err(error) = compilation
            .state
            .checked_control_flow
            .set_test_observer(&key, checked_gate.observer())
        {
            panic!("control-flow fact must accept a test observer: {error:?}");
        }

        let checked = std::thread::scope(|scope| {
            let request_key = key.clone();
            let request = scope.spawn(|| {
                compilation
                    .checked_control_flow_with_cancellation(request_key, &checked_cancellation)
            });

            checked_gate.wait_until_observed(FactCellTestEvent::Computed, 1);
            checked_cancellation.cancel();
            checked_gate.release();

            match request.join() {
                Ok(result) => result,
                Err(_) => panic!("cancelled control-flow request panicked"),
            }
        });

        assert!(matches!(checked, Err(FactQueryError::Cancelled)));
        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(false)
        );

        assert!(compilation.checked_control_flow(key).is_ok());
    }

    #[test]
    fn recursive_callable_references_do_not_form_body_fact_cycles() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func recurse()\n",
            "{\n",
            "    recurse();\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);

        let checked = match compilation.checked_control_flow(key) {
            Ok(checked) => checked,
            Err(error) => panic!("recursive callable must check without a cycle: {error:?}"),
        };

        assert!(checked.diagnostics().is_empty());
    }

    #[test]
    fn storage_queries_are_lazy_cached_and_depend_on_control_flow() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func inspect()\n",
            "{\n",
            "    let value: i32 = 1;\n",
            "    value;\n",
            "}\n",
        ));
        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.checked_storage.is_published(&key),
            Ok(false)
        );

        let first = match compilation.checked_storage(key.clone()) {
            Ok(storage) => storage,
            Err(error) => panic!("storage query must complete: {error:?}"),
        };
        let second = match compilation.checked_storage(key.clone()) {
            Ok(storage) => storage,
            Err(error) => panic!("repeated storage query must complete: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));
        assert!(first.diagnostics().is_empty());
        assert!(!first.value().locals().is_empty());
        assert!(!first.value().occurrences().is_empty());
        assert_eq!(
            compilation.state.checked_control_flow.is_published(&key),
            Ok(true)
        );
        assert_eq!(
            compilation.state.checked_storage.is_published(&key),
            Ok(true)
        );
    }

    #[test]
    fn cancelled_storage_queries_publish_nothing_and_can_be_retried() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        if let Err(error) = compilation.checked_control_flow(key.clone()) {
            panic!("storage prerequisite must complete: {error:?}");
        }

        let cancellation = CancellationToken::new();
        let gate = FactTestGate::holding(FactCellTestEvent::Computed);

        if let Err(error) = compilation
            .state
            .checked_storage
            .set_test_observer(&key, gate.observer())
        {
            panic!("storage fact must accept a test observer: {error:?}");
        }

        let checked = std::thread::scope(|scope| {
            let request_key = key.clone();
            let request = scope.spawn(|| {
                compilation.checked_storage_with_cancellation(request_key, &cancellation)
            });

            gate.wait_until_observed(FactCellTestEvent::Computed, 1);
            cancellation.cancel();
            gate.release();

            match request.join() {
                Ok(result) => result,
                Err(_) => panic!("cancelled storage request panicked"),
            }
        });

        assert!(matches!(checked, Err(FactQueryError::Cancelled)));
        assert_eq!(
            compilation.state.checked_storage.is_published(&key),
            Ok(false)
        );
        assert!(compilation.checked_storage(key).is_ok());
    }

    fn callable_compilation() -> Compilation {
        compilation(
            r#"module app;
func main()
{
}
"#,
        )
    }
}
