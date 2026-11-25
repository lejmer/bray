use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_ir::{MirOperationKind, MirTerminatorKind};

#[test]
fn box_construction_transfers_its_noncopyable_input_storage() {
    let fixture = include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    );

    let (declarations, _) = fixture
        .split_once("func main")
        .expect("fixture has a main body");

    for (reuse, valid) in [("", true), ("take(pair.left);", false)] {
        let source = format!("{declarations}{}", r#"
            func box_input()
            {
                let mut first: Counter = Counter { value = 0 };
                let mut second: Counter = Counter { value = 0 };
                let pair = Pair {
                    left = Guard { counter = &mut first },
                    right = Guard { counter = &mut second },
                };
                let owned: box[InlineStorage<Pair>] Pair = box[InlineStorage<Pair>](pair, options = 9);
                boxed(owned);
                REUSE
            }
        "#.replace("REUSE", reuse));

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        if valid {
            assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, "box_input"))
                .expect("valid boxed transfer lowers");

            assert!(!lowered.diagnostics().has_errors(), "{:?}", lowered.diagnostics());
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingUseOfMovedStorage,
            );
        }
    }
}

#[test]
fn partially_moved_box_projects_through_policy_and_releases_on_reachable_exits() {
    let source = include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    );

    let (declarations, _) = source
        .split_once("func main")
        .expect("fixture `split_once` call must succeed");

    let compilation = compilation(declarations);
    let key = source_function_body_key(&compilation, "boxed");

    let analysis = compilation
        .async_analysis(key.clone())
        .expect("fixture callable must complete async analysis");

    let release_instances = analysis
        .value()
        .storage_requirements()
        .iter()
        .flat_map(|requirement| requirement.parts().into_iter().flatten())
        .filter_map(bray_bound_tree::StorageCleanupPart::release)
        .map(|call| call.callable())
        .collect::<Vec<_>>();

    assert_eq!(release_instances.len(), 1, "{analysis:?}");

    let lowered = compilation
        .lowered_unit(key)
        .expect("fixture callable must lower successfully");

    let mir = lowered
        .value()
        .as_ref()
        .expect("fixture optional value must be present")
        .mir()
        .expect("fixture instance must retain its MIR");

    let reaches_release = |entry| {
        let mut pending = vec![entry];
        let mut visited = std::collections::BTreeSet::new();

        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }

            let block = mir.block(id).expect("reachable cleanup block must exist");

            if block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).expect("cleanup operation must exist").kind(),
                    MirOperationKind::Call(call)
                        if matches!(call.target(), bray_ir::MirCallTarget::Direct(reference)
                            if release_instances.contains(&reference.instance()))
                )
            }) {
                return true;
            }

            block
                .terminator()
                .kind()
                .for_each_successor(|target| pending.push(target));
        }

        false
    };

    let mut normal = false;
    let mut body_panic = false;
    let mut body_cancellation = false;
    let mut checked_release = false;

    for block in mir.blocks() {
        match block.terminator().kind() {
            MirTerminatorKind::BeginCleanup(edge) => {
                normal |= reaches_release(edge.edge().target());
            }
            MirTerminatorKind::CheckCallOutcome {
                panicked,
                cancelled,
                ..
            } => {
                let release_call = block.operations().iter().any(|operation| {
                    matches!(
                        mir.operation(*operation).expect("checked call operation exists").kind(),
                        MirOperationKind::Call(call) if call.is_cleanup()
                            && matches!(call.target(), bray_ir::MirCallTarget::Direct(reference)
                                if release_instances.contains(&reference.instance()))
                    )
                });

                if release_call {
                    checked_release = true;
                } else {
                    body_panic |= reaches_release(panicked.target());
                    body_cancellation |= reaches_release(cancelled.target());
                }
            }
            _ => {}
        }
    }

    assert!(
        normal && body_panic && !body_cancellation && checked_release,
        "release reachability: normal={normal}, body_panic={body_panic}, body_cancellation={body_cancellation}, checked_release={checked_release}; {mir:?}"
    );

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
            bray_ir::MirOperationKind::Cleanup { place, .. } if place.projections().first().is_some_and(|projection| projection.kind() == &bray_ir::MirProjectionKind::Dereference)
        )), "surviving fields must use the policy's borrowed target: {mir:?}");
}

#[test]
fn box_patterns_select_borrows_independently_of_partial_cleanup() {
    let source = include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    );

    let (declarations, _) = source
        .split_once("func main")
        .expect("fixture `split_once` call must succeed");

    let compilation = compilation(declarations);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for (name, kind) in [
        ("observe_box", bray_symbols::BorrowKind::Shared),
        ("observe_borrowed_box", bray_symbols::BorrowKind::Shared),
        ("move_box_target", bray_symbols::BorrowKind::Mutable),
    ] {
        let key = source_function_body_key(&compilation, name);

        let storage = compilation
            .storage_plan(key.clone())
            .expect("fixture callable must have a storage plan");

        let calls = storage.value().owned_borrows().collect::<Vec<_>>();

        assert_eq!(calls.len(), 2, "{name}: {storage:?}");

        let (_, _, selected) = calls
            .iter()
            .find(|(_, candidate, _)| *candidate == kind)
            .expect("fixture lookup must find its expected value");

        let lowered = compilation
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        let mir = lowered
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR");

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                MirOperationKind::Call(call) if matches!(call.target(),
                    bray_ir::MirCallTarget::Direct(reference) if reference.instance() == selected.callable())
            )), "{name} must use the selected {kind:?} policy borrow: {mir:?}");

        if matches!(name, "observe_box" | "observe_borrowed_box") {
            assert!(!mir.operations().iter().any(|operation| matches!(operation.kind(),
                    MirOperationKind::Store { destination, .. } if !destination.projections().is_empty()
                )), "observed aliases must not write back to source storage: {mir:?}");

            let analysis = compilation
                .async_analysis(key)
                .expect("fixture callable must complete async analysis");

            assert!(
                analysis
                    .value()
                    .storage_requirements()
                    .iter()
                    .all(|requirement| requirement.parts().is_none()),
                "observation must not require partial cleanup metadata: {analysis:?}"
            );
        }
    }
}

#[test]
fn guarded_part_fixture_preserves_payload_and_nullable_cleanup() {
    let compilation = compilation(include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    ));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in [
        "fields",
        "tupled",
        "payload",
        "nullable",
        "destructed",
        "indexed",
        "indexed_return",
        "nested_indexed",
        "indexed_repaired",
        "guarded",
        "guarded_alternative",
        "boxed",
        "boxed_panic",
        "observe_box",
        "observe_borrowed_box",
        "move_box_target",
        "main",
    ] {
        let key = source_function_body_key(&compilation, name);

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");

        if matches!(name, "payload" | "nullable") {
            let mir = lowered
                .value()
                .as_ref()
                .expect("fixture optional value must be present")
                .mir()
                .expect("fixture instance must retain its MIR");

            assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                    bray_ir::MirOperationKind::Cleanup { place, .. } if !place.projections().is_empty()
                )), "{name}: {mir:?}");
        }
    }
}

#[test]
fn borrowed_box_patterns_do_not_grant_target_ownership() {
    let source = include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    );

    let (declarations, _) = source
        .split_once("func main")
        .expect("fixture `split_once` call must succeed");

    let source = format!(
        r#"{declarations}
        func invalid_borrow_move(pos pair: &box[InlineStorage<Pair>] Pair)
        {{
            let box(value) = pair;

            consume_pair(value);
        }}
        "#
    );

    let compilation = compilation(&source);
    let diagnostics = compilation.check_diagnostics();

    assert!(
        diagnostics
            .by_kind(DiagnosticKind::CheckingMissingStorageOwnership)
            .next()
            .is_some(),
        "{diagnostics:?}"
    );

    let key = source_function_body_key(&compilation, "invalid_borrow_move");

    assert!(
        compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully")
            .value()
            .is_none()
    );
}
