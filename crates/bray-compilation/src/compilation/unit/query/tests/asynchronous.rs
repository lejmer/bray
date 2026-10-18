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
    let compilation = compilation(
        r#"
        module app;

        async func main(pending: Future<i32>) -> i32
        {
            await pending
        }
    "#,
    );

    let key = source_callable_body_key(&compilation);

    let analysis = compilation
        .async_analysis(key)
        .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

    let awaits = analysis
        .value()
        .suspensions()
        .iter()
        .filter(|point| {
            matches!(
                point.kind(),
                bray_bound_tree::AsyncSuspensionKind::Await { .. }
            )
        })
        .collect::<Vec<_>>();

    let [suspension] = awaits.as_slice() else {
        panic!("the direct await must publish one await suspension point");
    };

    assert!(suspension.deferred_calls().is_empty());
    assert!(!suspension.is_recovered(), "{suspension:?}");
}

#[test]
fn ordinary_valid_awaits_retain_their_inferred_dependency_contract() {
    let compilation = compilation(
        r#"
        module app;

        async func main(pending: Future<i32>) -> i32
        {
            await pending
        }
    "#,
    );

    let key = source_callable_body_key(&compilation);

    let analysis = compilation
        .async_analysis(key.clone())
        .unwrap_or_else(|error| panic!("async analysis must publish: {error:?}"));

    let dependencies = compilation
        .dependency_contracts(key)
        .unwrap_or_else(|error| panic!("dependency contracts must publish: {error:?}"));

    let awaits = analysis
        .value()
        .suspensions()
        .iter()
        .filter(|point| {
            matches!(
                point.kind(),
                bray_bound_tree::AsyncSuspensionKind::Await { .. }
            )
        })
        .collect::<Vec<_>>();

    let [suspension] = awaits.as_slice() else {
        panic!("the direct await must publish one await suspension point");
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

#[test]
fn implicit_async_cleanup_retains_pending_owners_and_dependencies() {
    for execution in ["async ", ""] {
        let application = compilation(&format!(
            r#"
                module app;

                extern async func work(pos value: &u32);

                async func main()
                {{
                    let value: u32 = 1;
                    let pending = work(&value).start();
                    let owner: Owner = {{ value = 1 }};
                }}

                struct Owner
                {{
                    value: u32;

                    {execution}finalize()
                    {{
                    }}
                }}
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

        let cleanup = analysis
            .value()
            .suspensions()
            .iter()
            .filter(|point| point.kind() == bray_bound_tree::AsyncSuspensionKind::Cleanup)
            .collect::<Vec<_>>();

        if execution.is_empty() {
            // The started future is already moved. Potential storage-flow cleanup
            // points do not require a suspension after checked owner selection.
            assert!(cleanup.is_empty());

            continue;
        }

        assert!(!cleanup.is_empty());

        for point in cleanup {
            assert!(matches!(
                point.occurrence(),
                bray_bound_tree::BoundExecutionSite::Cleanup(_)
            ));

            assert_eq!(point.deferred_calls().len(), 1);
            assert!(!point.is_recovered());

            let state = flow.value().suspension(point.occurrence()).unwrap();

            assert!(!state.cleanup_retention().is_empty());

            for dependency in state.cleanup_retention() {
                assert!(point.retained_subjects().contains(dependency));
                assert!(analysis.value().frame_dependencies().contains(dependency));
            }

            for access in point.cancellation_broadcast() {
                assert!(
                    state
                        .live()
                        .contains(&storage.value().root_identity(*access).unwrap())
                );
            }

            assert!(!point.cancellation_broadcast().is_empty());
        }
    }
}

#[test]
fn implicit_cleanup_retains_the_pending_return_dependency() {
    let application = compilation(
        r#"
        module app;

        async func main(pos output: &u32) -> &u32
        {
            let owner: Owner = { value = 1 };
            return output;
        }

        struct Owner
        {
            value: u32;

            async finalize()
            {
            }
        }
    "#,
    );

    let key = crate::test_support::source_function_body_key(&application, "main");
    let liveness = application.liveness(key.clone()).unwrap();
    let analysis = application.async_analysis(key).unwrap();

    assert!(
        !analysis.diagnostics().has_errors(),
        "{:?}",
        analysis.diagnostics()
    );

    let point = analysis
        .value()
        .suspensions()
        .iter()
        .find(|point| point.kind() == bray_bound_tree::AsyncSuspensionKind::Cleanup)
        .expect("ordinary owner finalization can suspend before returning");

    let retained = liveness
        .value()
        .live_across_scopes()
        .iter()
        .filter(|entry| entry.exit() == point.occurrence().node())
        .collect::<Vec<_>>();

    assert!(!retained.is_empty());

    for entry in retained {
        assert!(
            point.retained_subjects().contains(&entry.subject()),
            "{point:?} misses {entry:?}"
        );
    }
}

#[test]
fn moving_the_finalizer_owner_out_does_not_schedule_cleanup() {
    let application = compilation(
        r#"
        module app;

        async func main() -> Owner
        {
            let owner: Owner = { value = 1 };
            return owner;
        }

        struct Owner
        {
            value: u32;

            async finalize()
            {
            }
        }
    "#,
    );

    let key = crate::test_support::source_function_body_key(&application, "main");
    let analysis = application.async_analysis(key).unwrap();

    assert!(
        !analysis.diagnostics().has_errors(),
        "{:?}",
        analysis.diagnostics()
    );

    assert!(analysis.value().suspensions().is_empty());
}

#[test]
fn replacement_cleanup_retains_the_old_and_incoming_dependencies() {
    let application = compilation(
        r#"
        module app;
        extern async func pause();
        async func main()
        {
            let first: u32 = 1;
            let second: u32 = 2;
            let mut owner = Owner { value = &first };
            owner = Owner { value = &second };
        }
        struct Owner
        {
            value: &u32;
            async finalize()
            {
                await pause();
            }
        }
    "#,
    );

    assert!(
        !application.check_diagnostics().has_errors(),
        "{:?}",
        application.check_diagnostics()
    );

    let key = crate::test_support::source_function_body_key(&application, "main");
    let storage = application.storage_plan(key.clone()).unwrap();
    let flow = application.storage_flow(key.clone()).unwrap();
    let analysis = application.async_analysis(key).unwrap();

    let replacement = analysis
        .value()
        .replacements()
        .first()
        .expect("fixture replaces an initialized owner");

    let occurrence = bray_bound_tree::BoundExecutionSite::Cleanup(replacement.expression().into());

    let point = analysis
        .value()
        .suspensions()
        .iter()
        .find(|point| point.occurrence() == occurrence)
        .unwrap();

    let state = flow.value().suspension(occurrence).unwrap();

    assert!(
        state
            .initialized()
            .contains(&storage.value().root_identity(replacement.access()).unwrap())
    );

    // The old value is still in storage. The evaluated incoming value has moved
    // into the assignment's pending inputs, whose dependencies liveness retains.
    assert!(state.cleanup_retention().iter().any(|subject| matches!(
        subject,
        bray_bound_tree::BoundDependencySubject::BorrowCapability(_)
    )));

    assert!(
        point
            .retained_subjects()
            .iter()
            .filter(|subject| matches!(
                subject,
                bray_bound_tree::BoundDependencySubject::BorrowCapability(_)
            ))
            .count()
            >= 2,
        "old and incoming owners both retain their loans during replacement: {point:?}"
    );

    assert!(
        state
            .cleanup_retention()
            .iter()
            .all(|subject| point.retained_subjects().contains(subject))
    );
}

#[test]
fn input_cleanup_retains_borrowed_dependencies_before_acceptance() {
    for invocation in [
        "await accept(Owner { value = &value });",
        "await accept(Owner { value = &value }, number = rejected());",
        "await accept(Owner { value = &value }, number = { let input = rejected(); yield input; });",
    ] {
        let source = format!(
            r#"
            module app;
            extern async func pause();
            func rejected() -> u32 {{ panic("input"); }}
            struct Owner {{ value: &u32; async finalize() {{ await pause(); }} }}
            async func accept(pos owner: Owner, number: u32 = rejected()) {{}}
            async func main() {{ let value: u32 = 1; {invocation} }}
        "#
        );

        let application = compilation(&source);

        assert!(
            !application.check_diagnostics().has_errors(),
            "{:?}",
            application.check_diagnostics()
        );

        let key = crate::test_support::source_function_body_key(&application, "main");
        let analysis = application.async_analysis(key).unwrap();

        assert!(
            analysis.value().suspensions().iter().any(|point| {
                point.kind() == bray_bound_tree::AsyncSuspensionKind::Cleanup
                    && point.retained_subjects().iter().any(|subject| {
                        matches!(
                            subject,
                            bray_bound_tree::BoundDependencySubject::BorrowCapability(_)
                        )
                    })
            }),
            "pending input cleanup must retain its borrowed provider: {:?}",
            analysis.value()
        );
    }
}
