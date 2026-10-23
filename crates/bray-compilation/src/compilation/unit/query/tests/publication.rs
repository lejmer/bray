use super::support::fixtures::callable_compilation;
use crate::compilation::unit::support::check_control_flow;
use crate::fact::{CancellationToken, FactCellTestEvent, FactQueryError, QueryPriority};
use crate::test_support::{
    FactTestGate, compilation, compilation_with_sources_and_worker_budget, source_callable_body_key,
};
use bray_binder::semantic_unit_context;

use bray_checker::SemanticUnitContext;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;
use std::sync::Arc;

#[test]
fn liveness_is_demanded_independently_and_reuses_its_publication() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32) -> i32\n",
        "{\n",
        "    let result: i32 = value;\n",
        "    return result;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.body_semantics.is_published(&key),
        Ok(false)
    );

    let first = match compilation.liveness(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("liveness analysis must publish: {error:?}"),
    };

    assert!(!first.value().last_uses().is_empty());

    assert!(
        first
            .value()
            .last_uses()
            .iter()
            .all(|last_use| last_use.operation().unit() == first.value().unit())
    );

    let second = match compilation.liveness(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated liveness analysis must publish: {error:?}"),
    };

    assert_eq!(first.snapshot_address(), second.snapshot_address());

    let dependencies = match compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
    {
        Ok(Some(dependencies)) => dependencies,
        Ok(None) => panic!("published liveness must retain dependencies"),
        Err(error) => panic!("liveness dependencies must be readable: {error:?}"),
    };

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone()
        ))
    );

    let dependencies = compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone(),
        ))
        .unwrap()
        .expect("the component owns this body's prerequisites");

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));
    assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key)));
}

#[test]
fn dependency_contracts_reuse_the_body_semantic_publication() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32) -> i32\n",
        "{\n",
        "    return value;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.body_semantics.is_published(&key),
        Ok(false)
    );

    let first = match compilation.dependency_contracts(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("dependency contracts must publish: {error:?}"),
    };

    let second = match compilation.dependency_contracts(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated dependency contracts must publish: {error:?}"),
    };

    let liveness = compilation
        .liveness(key.clone())
        .unwrap_or_else(|error| panic!("liveness must publish: {error:?}"));

    let refinements = compilation
        .refinements(key.clone())
        .unwrap_or_else(|error| panic!("refinements must publish: {error:?}"));

    let storage_flow = compilation
        .storage_flow(key.clone())
        .unwrap_or_else(|error| panic!("storage flow must publish: {error:?}"));

    let asynchronous = compilation
        .async_analysis(key.clone())
        .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

    assert_eq!(first.snapshot_address(), second.snapshot_address());
    assert_eq!(first.snapshot_address(), liveness.snapshot_address());
    assert_eq!(first.snapshot_address(), refinements.snapshot_address());
    assert_eq!(first.snapshot_address(), storage_flow.snapshot_address());
    assert_eq!(first.snapshot_address(), asynchronous.snapshot_address());

    let dependencies = match compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
    {
        Ok(Some(dependencies)) => dependencies,
        Ok(None) => panic!("published dependency contracts must retain dependencies"),
        Err(error) => panic!("dependency contract dependencies must be readable: {error:?}"),
    };

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone()
        ))
    );

    let dependencies = compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone(),
        ))
        .unwrap()
        .expect("the component owns this body's prerequisites");

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
            key.clone()
        ))
    );

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key.clone())));
    assert!(dependencies.contains(&crate::fact::CompilationFactKey::MemoryOperations(key)));
}

#[test]
fn storage_flow_is_lazy_and_reports_overlapping_borrows() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let mut value: i32 = 1;\n",
        "    let shared: &i32 = &value;\n",
        "    let exclusive: & mut i32 = & mut value;\n",
        "    shared;\n",
        "    exclusive;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.body_semantics.is_published(&key),
        Ok(false)
    );

    let analysis = match compilation.storage_flow(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(
        analysis.value().operations().iter().any(|operation| {
            operation.status() == bray_bound_tree::StorageOperationStatus::ConflictingBorrow
        }),
        "{analysis:?}"
    );

    let diagnostic = analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingConflictingBorrow)
        .next()
        .unwrap_or_else(|| panic!("overlapping borrows must produce a diagnostic"));

    bray_testing::assert_goal_state_diagnostic(diagnostic);

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingConflictingBorrow,
    );

    let repeated = match compilation.storage_flow(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated storage-flow checking must publish: {error:?}"),
    };

    assert_eq!(analysis.snapshot_address(), repeated.snapshot_address());

    let dependencies = match compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
    {
        Ok(Some(dependencies)) => dependencies,
        Ok(None) => panic!("published storage flow must retain dependencies"),
        Err(error) => panic!("storage-flow dependencies must be readable: {error:?}"),
    };

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone()
        ))
    );

    let dependencies = compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone(),
        ))
        .unwrap()
        .expect("the component owns this body's prerequisites");

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key.clone())));

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
            key.clone()
        ))
    );

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::MemoryOperations(key)));
}

#[test]
fn repeated_and_concurrent_requests_share_production_semantics() {
    let worker_budget = crate::WorkerBudget::new(2)
        .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"));

    let compilation = compilation_with_sources_and_worker_budget(
        &[r#"module app;
func main()
{
}
"#],
        worker_budget,
    );

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

    let declared_gate = FactTestGate::holding(FactCellTestEvent::Computing);

    if let Err(error) = compilation
        .state
        .declared_value_type_templates
        .set_test_observer(&key, declared_gate.observer())
    {
        panic!("declared value type result must accept a test observer: {error:?}");
    }

    let declared = std::thread::scope(|scope| {
        let owner_key = key.clone();
        let owner = scope.spawn(|| compilation.declared_value_type_templates(owner_key));

        declared_gate.wait_until_observed(FactCellTestEvent::Computing, 1);

        let waiter_key = key.clone();
        let waiter = scope.spawn(|| compilation.declared_value_type_templates(waiter_key));

        declared_gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
        declared_gate.release();

        [owner, waiter].map(|handle| match handle.join() {
            Ok(Ok(analysis)) => analysis,
            Ok(Err(error)) => panic!("concurrent declared value types failed: {error:?}"),
            Err(_) => panic!("concurrent declared value type request panicked"),
        })
    });

    assert!(Arc::ptr_eq(&declared[0], &declared[1]));

    let expression_gate = FactTestGate::holding(FactCellTestEvent::Computing);

    if let Err(error) = compilation
        .state
        .expression_semantics
        .set_test_observer(&key, expression_gate.observer())
    {
        panic!("expression semantic result must accept a test observer: {error:?}");
    }

    let (types, selections) = std::thread::scope(|scope| {
        let types_key = key.clone();
        let types = scope.spawn(|| compilation.expression_types(types_key));

        expression_gate.wait_until_observed(FactCellTestEvent::Computing, 1);

        let selections_key = key.clone();

        let selections = scope.spawn(|| compilation.semantic_selections(selections_key));

        expression_gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
        expression_gate.release();

        let types = match types.join() {
            Ok(Ok(types)) => types,
            Ok(Err(error)) => panic!("concurrent expression types failed: {error:?}"),
            Err(_) => panic!("concurrent expression type request panicked"),
        };

        let selections = match selections.join() {
            Ok(Ok(selections)) => selections,
            Ok(Err(error)) => panic!("concurrent semantic selections failed: {error:?}"),
            Err(_) => panic!("concurrent semantic selection request panicked"),
        };

        (types, selections)
    });

    assert_eq!(types.value().unit(), selections.value().unit());
    assert_eq!(types.value().kind(), selections.value().kind());
    assert_eq!(types.diagnostics(), selections.diagnostics());

    let gate = FactTestGate::holding(FactCellTestEvent::Computing);

    if let Err(error) = compilation
        .state
        .checked_control_flow
        .set_test_observer(&key, gate.observer())
    {
        panic!("control-flow analysis must accept a test observer: {error:?}");
    }

    // Bound-unit keys are Arc-backed immutable identities shared by concurrent requests.
    let checked = std::thread::scope(|scope| {
        let compilation = &compilation;
        let owner_key = key.clone();
        let owner = scope.spawn(move || compilation.control_flow(owner_key));

        gate.wait_until_observed(FactCellTestEvent::Computing, 1);

        let waiter_key = key.clone();
        let waiter = scope.spawn(move || compilation.control_flow(waiter_key));

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
            .all(|result| Arc::ptr_eq(&checked[0], result))
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
        panic!("bound unit must accept a test observer: {error:?}");
    }

    let bound = std::thread::scope(|scope| {
        let request_key = key.clone();

        let request = scope.spawn(|| {
            compilation.bound_unit_with_priority(
                request_key,
                &bound_cancellation,
                QueryPriority::Interactive,
            )
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
        panic!("control-flow analysis must accept a test observer: {error:?}");
    }

    let checked = std::thread::scope(|scope| {
        let request_key = key.clone();

        let request = scope.spawn(|| {
            compilation.control_flow_with_cancellation(request_key, &checked_cancellation)
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

    assert!(compilation.control_flow(key.clone()).is_ok());

    let expression_cancellation = CancellationToken::new();
    let expression_gate = FactTestGate::holding(FactCellTestEvent::Computed);

    if let Err(error) = compilation
        .state
        .expression_semantics
        .set_test_observer(&key, expression_gate.observer())
    {
        panic!("expression semantic result must accept a test observer: {error:?}");
    }

    let expression_semantics = std::thread::scope(|scope| {
        let request_key = key.clone();

        let request = scope.spawn(|| {
            compilation
                .expression_semantics_with_cancellation(request_key, &expression_cancellation)
        });

        expression_gate.wait_until_observed(FactCellTestEvent::Computed, 1);
        expression_cancellation.cancel();
        expression_gate.release();

        match request.join() {
            Ok(result) => result,
            Err(_) => panic!("cancelled expression semantic request panicked"),
        }
    });

    assert!(matches!(
        expression_semantics,
        Err(FactQueryError::Cancelled)
    ));

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(&key),
        Ok(false)
    );

    assert!(compilation.expression_types(key).is_ok());
}

#[test]
fn recursive_callable_references_do_not_form_body_query_cycles() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func recurse()\n",
        "{\n",
        "    recurse();\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let checked = match compilation.control_flow(key) {
        Ok(checked) => checked,
        Err(error) => panic!("recursive callable must check without a cycle: {error:?}"),
    };

    assert!(checked.diagnostics().is_empty());
}

#[test]
fn unit_queries_do_not_force_nested_units() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let callable = lambda()\n",
        "    {\n",
        "    };\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("callable body must bind: {error:?}"),
    };

    let [nested] = bound.value().nested_units() else {
        panic!("test callable must contain one nested semantic unit");
    };

    assert_eq!(
        compilation.state.checked_control_flow.is_published(nested),
        Ok(false)
    );

    assert_eq!(
        compilation.state.storage_plans.is_published(nested),
        Ok(false)
    );

    if let Err(error) = compilation.control_flow(key.clone()) {
        panic!("parent control-flow analysis must be available: {error:?}");
    }

    assert_eq!(
        compilation.state.checked_control_flow.is_published(nested),
        Ok(false)
    );

    if let Err(error) = compilation.storage_plan(key) {
        panic!("parent storage plan must be available: {error:?}");
    }

    assert_eq!(
        compilation.state.storage_plans.is_published(nested),
        Ok(false)
    );
}

#[test]
#[should_panic(expected = "checker context category must match")]
fn invalid_checker_unit_views_expose_the_request_bug() {
    let compilation = callable_compilation();
    let key = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("bound unit must be available: {error:?}"),
    };

    let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
        Ok(context) => context,
        Err(error) => panic!("checker context must be available: {error:?}"),
    };

    let canonical = semantic_unit_context(context.symbols(), bound.value());

    let SemanticUnitContext::CallableBody(declaration) = canonical else {
        panic!("callable body must produce a callable-body checker entry");
    };

    let invalid = SemanticUnitContext::Constraint(declaration);
    let _ = check_control_flow(bound.value(), &invalid, &context);
}

#[test]
#[should_panic(expected = "must have an owner in its symbol graph")]
fn invalid_semantic_unit_contexts_expose_the_producer_bug() {
    let primary = callable_compilation();
    let key = source_callable_body_key(&primary);

    let bound = match primary.bound_unit(key.clone()) {
        Ok(bound) => bound,
        Err(error) => panic!("bound unit must be available: {error:?}"),
    };

    let foreign = compilation(
        r#"module other;
func other()
{
}
"#,
    );

    let symbols = match foreign.symbol_graph() {
        Ok(symbols) => symbols,
        Err(error) => panic!("foreign symbol graph must be available: {error:?}"),
    };

    semantic_unit_context(symbols, bound.value());
}

#[test]
fn selected_call_inventory_precedes_flow_and_reuses_its_publication() {
    let compilation = compilation(
        "module app; func identity(pos value: bool) -> bool { return value; } func main() -> bool { return identity(true); }",
    );

    let key = crate::test_support::source_function_body_key(&compilation, "main");
    let cancellation = CancellationToken::new();

    let first = compilation
        .selected_body_behavior_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert_eq!(first.result().value().calls().len(), 1);

    assert_eq!(
        compilation.state.body_semantics.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.storage_plans.is_published(&key),
        Ok(false)
    );

    let checked = compilation
        .body_semantics_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert!(!checked.result().diagnostics().has_errors(), "{checked:#?}");

    let second = compilation
        .selected_body_behavior_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert!(Arc::ptr_eq(&first, &second));

    let dependencies = compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
        .unwrap()
        .unwrap();

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::SelectedBodyBehavior(key)));
}

#[test]
fn prepared_body_is_shared_by_ownership_unwind_and_execution_proofs() {
    let compilation = compilation(
        r#"
        module app;
        struct Resource { destruct() {} }
        func consume(pos resource: Resource) {}
        func main() {
            let resource: Resource = Resource {};
            consume(resource);
        }
    "#,
    );

    let key = crate::test_support::source_function_body_key(&compilation, "main");
    let cancellation = CancellationToken::new();

    let prepared = compilation
        .prepared_body_analysis_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert!(
        !prepared.result().diagnostics().has_errors(),
        "{:?}",
        prepared.result().diagnostics()
    );

    assert_eq!(
        compilation.state.body_semantics.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.unwind_candidates.is_published(&key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.execution_candidates.is_published(&key),
        Ok(false)
    );

    let mut pending = vec![crate::fact::CompilationFactKey::PreparedBodyAnalysis(
        key.clone(),
    )];

    let mut visited = std::collections::BTreeSet::new();

    while let Some(dependency) = pending.pop() {
        if !visited.insert(dependency.clone()) {
            continue;
        }

        assert!(
            !matches!(
                dependency,
                crate::fact::CompilationFactKey::ResultDependencyComponent(_)
            ),
            "structural preparation must not demand inferred reference summaries: {dependency:?}",
        );

        pending.extend(
            compilation
                .state
                .fact_runtime
                .dependencies(&dependency)
                .unwrap()
                .into_iter()
                .flatten(),
        );
    }

    let unwind = compilation
        .unwind_behavior(key.clone(), &cancellation)
        .unwrap();

    assert!(!unwind.result().diagnostics().has_errors(), "{unwind:#?}");

    let body = compilation
        .body_semantics_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert!(!body.result().diagnostics().has_errors(), "{body:#?}");

    let execution = compilation
        .execution_candidates_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert!(
        !execution.result().diagnostics().has_errors(),
        "{execution:#?}"
    );

    let reused = compilation
        .prepared_body_analysis_with_cancellation(key.clone(), &cancellation)
        .unwrap();

    assert!(Arc::ptr_eq(&prepared, &reused));

    for consumer in [
        crate::fact::CompilationFactKey::BodySemantics(key.clone()),
        crate::fact::CompilationFactKey::UnwindCandidate(key.clone()),
        crate::fact::CompilationFactKey::ExecutionCandidates(key.clone()),
    ] {
        let dependencies = compilation
            .state
            .fact_runtime
            .dependencies(&consumer)
            .unwrap()
            .unwrap();

        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::PreparedBodyAnalysis(
                key.clone()
            )),
            "{consumer:?}: {dependencies:?}"
        );
    }
}

#[test]
fn recursive_result_dependencies_share_one_component_publication() {
    let application = compilation(
        r#"
        module app;
        func first(pos value: &u32, pos flag: bool) -> &u32 {
            if flag { return value; }
            return second(value, true);
        }
        func second(pos value: &u32, pos flag: bool) -> &u32 {
            if flag { return value; }
            return first(value, true);
        }
    "#,
    );

    let members = ["first", "second"].map(|name| {
        let callable: bray_symbols::CallableSymbolId =
            crate::test_support::source_function(&application, name).into();

        (
            callable,
            crate::test_support::source_function_body_key(&application, name),
        )
    });

    let owner = members
        .iter()
        .min_by_key(|(callable, _)| *callable)
        .unwrap();

    let key = crate::fact::CompilationFactKey::ResultDependencyComponent(owner.1.clone());
    let cancellation = &application.state.cancellation;
    let context = application.binding_context(cancellation).unwrap();
    let start = std::sync::Barrier::new(members.len());

    std::thread::scope(|scope| {
        let handles = members
            .iter()
            .map(|(callable, _)| {
                let start = &start;

                scope.spawn(move || {
                    start.wait();

                    bray_binder::SymbolQueryProvider::resolve_symbol_query(
                        &context,
                        bray_symbols::SymbolQueryRequest::<
                            bray_symbols::CallableResultDependenciesQuery,
                        >::new(*callable),
                    )
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let result = handle.join().unwrap();

            assert!(!result.diagnostics().has_errors(), "{result:?}");
        }
    });

    let mut first = None;

    for (callable, body) in members.iter().rev() {
        let request = bray_symbols::SymbolQueryRequest::<
            bray_symbols::CallableResultDependenciesQuery,
        >::new(*callable);

        let result =
            bray_binder::SymbolQueryProvider::resolve_symbol_query(&context, request).unwrap();

        assert!(!result.diagnostics().has_errors(), "{result:?}");

        let published = application
            .state
            .result_dependency_components
            .get_or_compute(
                &application.state.fact_runtime,
                cancellation,
                key.clone(),
                owner.1.clone(),
                || panic!("demanding any member must publish the complete component"),
            )
            .unwrap();

        assert_eq!(
            published
                .result()
                .value()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            members
                .iter()
                .map(
                    |(callable, _)| bray_checker::ResultDependencyOwner::Declaration(
                        callable.into_any()
                    )
                )
                .collect()
        );

        assert_eq!(
            published.result().value()
                [&bray_checker::ResultDependencyOwner::Declaration(callable.into_any())]
                .dependencies,
            *result.value()
        );

        if let Some(first) = &first {
            assert!(Arc::ptr_eq(first, &published));
        }

        let body_result = application
            .body_semantics_with_cancellation(body.clone(), cancellation)
            .unwrap();

        assert!(
            Arc::ptr_eq(
                published.result().value()
                    [&bray_checker::ResultDependencyOwner::Declaration(callable.into_any())]
                    .body
                    .value(),
                body_result.result().value()
            ),
            "the body query must share the component's settled snapshot without copying or rechecking it"
        );

        first = Some(published);

        let dependencies = application
            .state
            .fact_runtime
            .dependencies(&crate::fact::CompilationFactKey::from(
                crate::fact::SymbolQueryKey::new(request.symbol(), request.kind()),
            ))
            .unwrap()
            .unwrap();

        assert!(dependencies.contains(&key), "{dependencies:?}");

        assert_eq!(
            application
                .state
                .result_dependency_components
                .is_published(body),
            Ok(body == &owner.1)
        );
    }

    let dependencies = application
        .state
        .fact_runtime
        .dependencies(&key)
        .unwrap()
        .unwrap();

    for (_, body) in members {
        assert!(
            dependencies.contains(&crate::fact::CompilationFactKey::SelectedBodyBehavior(
                body.clone()
            ))
        );

        assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(body)));
    }
}

#[test]
fn recursive_runtime_default_and_callable_share_checked_component() {
    let application = compilation(
        r#"
        module app;
        func recurse(value: u32 = repeat()) -> u32 { return value; }
        func repeat() -> u32 { return recurse(); }
    "#,
    );

    let cancellation = &application.state.cancellation;
    let context = application.binding_context(cancellation).unwrap();

    let callable: bray_symbols::CallableSymbolId =
        crate::test_support::source_function(&application, "repeat").into();

    let body = crate::test_support::source_function_body_key(&application, "repeat");

    let behavior = application
        .selected_body_behavior_with_cancellation(body.clone(), cancellation)
        .unwrap();

    let provider = behavior.result().value().defaults()[0];

    let default = application
        .declared_unit_key(
            provider.symbol(),
            bray_bound_tree::BoundUnitKind::RuntimeDefault,
        )
        .unwrap()
        .unwrap();

    let members = [(callable.into_any(), body), (provider.symbol(), default)];
    let owner = members.iter().min_by_key(|(symbol, _)| *symbol).unwrap();
    let start = std::sync::Barrier::new(2);

    std::thread::scope(|scope| {
        let handles = members
            .iter()
            .map(|(_, key)| {
                let application = &application;
                let start = &start;

                scope.spawn(move || {
                    start.wait();

                    application
                        .body_semantics_with_cancellation(key.clone(), cancellation)
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let body = handle.join().unwrap();

            assert!(!body.result().diagnostics().has_errors(), "{body:?}");
        }
    });

    let published = application
        .state
        .result_dependency_components
        .get_or_compute(
            &application.state.fact_runtime,
            cancellation,
            crate::fact::CompilationFactKey::ResultDependencyComponent(owner.1.clone()),
            owner.1.clone(),
            || panic!("both bodies must already share one completed component"),
        )
        .unwrap();

    assert_eq!(published.result().value().len(), 2);

    let summary =
        crate::compilation::binder::default_result_dependencies(&context, provider).unwrap();

    assert_eq!(
        summary,
        published.result().value()
            [&bray_checker::ResultDependencyOwner::Declaration(provider.symbol())]
            .dependencies
    );

    for (symbol, key) in &members {
        let body = application
            .body_semantics_with_cancellation(key.clone(), cancellation)
            .unwrap();

        assert!(Arc::ptr_eq(
            body.result().value(),
            published.result().value()[&bray_checker::ResultDependencyOwner::Declaration(*symbol)]
                .body
                .value()
        ));

        assert_eq!(
            application
                .state
                .result_dependency_components
                .is_published(key),
            Ok(key == &owner.1)
        );
    }
}

#[test]
fn dependency_component_discovery_visits_each_chain_body_once() {
    const LENGTH: usize = 16;

    let mut source = String::from("module app;\n");

    for index in 0..LENGTH {
        let result = if index + 1 == LENGTH {
            "value".to_owned()
        } else {
            format!("link{}(value)", index + 1)
        };

        source.push_str(&format!(
            r#"
            func link{index}(pos value: &u32) -> &u32 {{
                return {result};
            }}
            "#
        ));
    }

    for reverse in [false, true] {
        let application = compilation(&source);

        let context = application
            .binding_context(&application.state.cancellation)
            .unwrap();

        for step in 0..LENGTH {
            let index = if reverse { LENGTH - step - 1 } else { step };
            let name = format!("link{index}");

            let callable: bray_symbols::CallableSymbolId =
                crate::test_support::source_function(&application, &name).into();

            let summary = bray_binder::SymbolQueryProvider::resolve_symbol_query(
                &context,
                bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(callable),
            ).unwrap();

            assert!(!summary.diagnostics().has_errors(), "{summary:?}");

            let body = application
                .body_semantics_with_cancellation(
                    crate::test_support::source_function_body_key(&application, &name),
                    &application.state.cancellation,
                )
                .unwrap();

            assert!(!body.result().diagnostics().has_errors(), "{body:?}");
        }

        assert_eq!(
            application
                .state
                .result_dependency_discoveries
                .load(std::sync::atomic::Ordering::Relaxed),
            LENGTH,
            "both caller-first and callee-first demand must reuse discovered membership"
        );
    }
}
