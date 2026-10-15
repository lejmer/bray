use super::support::representation::assert_expression_representation;
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

    // Await consumes the frame. The caller must not schedule it for cleanup again.
    assert!(
        analysis
            .value()
            .scope_exits()
            .iter()
            .all(|exit| exit.cancellation_broadcast().is_empty())
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

#[test]
fn suspension_retains_unused_cleanup_owners_and_their_dependencies() {
    for (consume, pending) in [("", true), ("try await owner.join();", false)] {
        let application = compilation(&format!(
            r#"
            module app;
            extern async func work(pos value: &u32);
            async func main() {{
                let value: u32 = 1;
                let owner = work(&value).start();
                {consume}
                await pause();
            }}
            async func pause() {{}}
        "#
        ));

        let key = crate::test_support::source_function_body_key(&application, "main");
        let storage = application.storage_plan(key.clone()).unwrap();
        let flow = application.storage_flow(key.clone()).unwrap();
        let analysis = application.async_analysis(key).unwrap();

        assert!(
            !analysis.diagnostics().has_errors(),
            "{:?}",
            analysis.diagnostics()
        );

        let suspension = analysis
            .value()
            .suspensions()
            .last()
            .expect("pause suspends after any explicit consumption");

        let snapshot = flow.value().suspension(suspension.occurrence()).unwrap();

        let cleanup_owners = analysis
            .value()
            .storage_requirements()
            .iter()
            .filter(|requirement| {
                matches!(
                    requirement.cleanup(),
                    bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                ) && snapshot.initialized().contains(&requirement.identity())
                    && matches!(
                        storage.value().identity(requirement.identity()),
                        Some(bray_bound_tree::StorageIdentity::LocalOwned(_))
                    )
            })
            .map(|requirement| {
                bray_bound_tree::BoundDependencySubject::Storage(requirement.identity())
            })
            .collect::<Vec<_>>();

        assert_eq!(
            !cleanup_owners.is_empty(),
            pending,
            "only an unresolved task still owns cleanup"
        );

        for subject in cleanup_owners {
            assert!(
                suspension.retained_subjects().contains(&subject),
                "cleanup owner must survive suspension: {subject:?}"
            );
        }

        assert_eq!(
            snapshot.cleanup_retention().iter().any(|subject| matches!(
                subject,
                bray_bound_tree::BoundDependencySubject::BorrowCapability(_)
            )),
            pending,
            "cleanup keeps the loan until consumption, even without an ordinary read"
        );

        assert!(
            snapshot
                .cleanup_retention()
                .iter()
                .all(|subject| suspension.retained_subjects().contains(subject))
        );
    }
}
