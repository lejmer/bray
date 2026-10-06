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
