use super::support::representation::{assert_expression_representation, type_representation};
use crate::test_support::{compilation, only_call_selection, source_callable_body_key};
use bray_bound_tree::{BoundCallResult, SemanticSelection};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn async_analysis_reports_awaits_in_synchronous_callables() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    await child();\n",
        "}\n",
        "async func child() -> i32\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.async_analysis(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("async analysis must publish: {error:?}"),
    };

    assert_eq!(analysis.value().suspensions().len(), 1);

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
    );

    assert!(
        analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingAwaitOutsideAsyncCallable)
            .next()
            .is_some()
    );

    assert!(
        compilation
            .semantic_diagnostics()
            .by_kind(DiagnosticKind::CheckingAwaitOutsideAsyncCallable)
            .next()
            .is_some()
    );
}

#[test]
fn async_analysis_respects_anonymous_callable_execution_modes() {
    for (modifier, expected_diagnostic) in [("", true), ("async ", false)] {
        let compilation = compilation(&format!(
            concat!(
                "module app;\n",
                "async func main()\n",
                "{{\n",
                "    await {modifier}lambda()\n",
                "    {{\n",
                "        await child();\n",
                "    }}();\n",
                "}}\n",
                "async func child()\n",
                "{{\n",
                "}}\n",
            ),
            modifier = modifier,
        ));

        let main = source_callable_body_key(&compilation);

        let bound = compilation
            .bound_unit(main)
            .unwrap_or_else(|error| panic!("source callable must bind: {error:?}"));

        let [anonymous] = bound.value().nested_units() else {
            panic!("source callable must contain one anonymous callable");
        };

        let analysis = compilation
            .async_analysis(anonymous.clone())
            .unwrap_or_else(|error| panic!("anonymous async analysis must publish: {error:?}"));

        let has_diagnostic = analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingAwaitOutsideAsyncCallable)
            .next()
            .is_some();

        assert_eq!(has_diagnostic, expected_diagnostic, "{modifier:?}");
    }
}

#[test]
fn opaque_future_parameters_do_not_imply_recovery() {
    let compilation = compilation(concat!(
        "module app;\n",
        "async func main(pending: Future<i32>) -> i32\n",
        "{\n",
        "    await pending\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = compilation
        .async_analysis(key)
        .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

    let [suspension] = analysis.value().suspensions() else {
        panic!("the direct await must publish one suspension point");
    };

    assert!(suspension.deferred_calls().is_empty());
    assert!(!suspension.is_recovered(), "{suspension:?}");
}

#[test]
fn ordinary_valid_awaits_retain_their_inferred_dependency_contract() {
    let compilation = compilation(concat!(
        "module app;\n",
        "async func main(pending: Future<i32>) -> i32\n",
        "{\n",
        "    await pending\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = compilation
        .async_analysis(key.clone())
        .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

    let dependencies = compilation
        .dependency_contracts(key)
        .unwrap_or_else(|error| panic!("dependency contracts must publish: {error:?}"));

    let [suspension] = analysis.value().suspensions() else {
        panic!("the direct await must publish one suspension point");
    };

    let contract = suspension
        .dependency_contract()
        .unwrap_or_else(|| panic!("a valid await must select its inferred contract"));

    assert!(dependencies.value().contract(contract).is_some());
    assert!(!suspension.is_recovered());
}

#[test]
fn async_analysis_follows_deferred_calls_through_local_future_bindings() {
    let compilation = compilation(concat!(
        "module app;\n",
        "async func main() -> i32\n",
        "{\n",
        "    let retained: i32 = 1;\n",
        "    let pending = child();\n",
        "    let ignored: i32 = await pending;\n",
        "    return retained;\n",
        "}\n",
        "async func child() -> i32\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let liveness = match compilation.liveness(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("liveness analysis must publish: {error:?}"),
    };

    let analysis = match compilation.async_analysis(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("async analysis must publish: {error:?}"),
    };

    let storage = match compilation.storage_plan(key.clone()) {
        Ok(storage) => storage,
        Err(error) => panic!("storage plan must publish: {error:?}"),
    };

    let [suspension] = analysis.value().suspensions() else {
        panic!("the direct await must publish one suspension point");
    };

    assert_eq!(suspension.deferred_calls().len(), 1);
    assert!(!suspension.is_recovered());
    assert!(!analysis.value().frame_dependencies().is_empty());

    assert_eq!(
        suspension.retained_subjects(),
        analysis.value().frame_dependencies()
    );

    assert!(
        liveness
            .value()
            .live_across_suspensions()
            .iter()
            .all(|entry| analysis
                .value()
                .frame_dependencies()
                .contains(&entry.subject()))
    );

    assert!(
        analysis
            .value()
            .scope_exits()
            .iter()
            .all(|exit| exit.cancellation_broadcast() == exit.lifecycle_resolution())
    );

    assert!(
        analysis
            .value()
            .scope_exits()
            .iter()
            .any(|exit| !exit.cancellation_broadcast().is_empty())
    );

    let cleanup_roles = analysis
        .value()
        .scope_exits()
        .iter()
        .flat_map(|exit| exit.cancellation_broadcast())
        .filter_map(|access| storage.value().access(*access))
        .filter_map(|access| type_representation(&compilation, access.reached_type()))
        .collect::<Vec<_>>();

    assert!(cleanup_roles.contains(&RepresentationRole::Future));

    assert!(
        cleanup_roles
            .iter()
            .all(|role| *role == RepresentationRole::Future)
    );

    let repeated = match compilation.async_analysis(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated async analysis must publish: {error:?}"),
    };

    assert_eq!(analysis.snapshot_address(), repeated.snapshot_address());
}

#[test]
fn asynchronous_calls_publish_target_specific_future_types() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let pending = produce();\n",
        "}\n",
        "async func produce() -> i32\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let types = match compilation.expression_types(key.clone()) {
        Ok(types) => types,
        Err(error) => panic!("asynchronous expression types must publish: {error:?}"),
    };

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("asynchronous call selection must publish: {error:?}"),
    };

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("asynchronous invocation must publish a call selection");
    };

    let BoundCallResult::LazyFuture(future) = call.resolution().result() else {
        panic!("asynchronous invocation must construct a lazy future");
    };

    let Some(result) = types.value().expression(selection.expression()) else {
        panic!("asynchronous call must have a final type");
    };

    assert_eq!(result.ty(), future.future_type());

    assert_expression_representation(
        &compilation,
        types.value(),
        selection.expression(),
        RepresentationRole::Future,
    );
}
