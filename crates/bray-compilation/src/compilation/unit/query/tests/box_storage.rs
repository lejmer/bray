use crate::test_support::{compilation, source_function_body_key};
use bray_bound_tree::BoundExpression;
use bray_diagnostics::DiagnosticKind;
use bray_ir::{MirBlockKind, MirOperationKind, MirTerminatorKind};

#[test]
fn box_construction_transfers_its_noncopyable_input_storage() {
    let compilation = compilation(include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    ));

    let key = source_function_body_key(&compilation, "main");
    let bound = compilation.bound_unit(key.clone()).unwrap();
    let flow = compilation.storage_flow(key).unwrap();

    let operands = bound
        .value()
        .tree()
        .expressions()
        .find_map(|(_, expression)| match expression {
            BoundExpression::BoxConstruction(expression) => Some(expression.arguments()),
            _ => None,
        })
        .unwrap();

    let [operand, ..] = operands else {
        panic!("fixture must provide a boxed value")
    };

    assert!(
        flow.value().operations().iter().any(|operation| {
            operation.expression() == operand.expression()
                && operation.purpose() == bray_bound_tree::StorageAccessPurpose::Move
                && operation.status() == bray_bound_tree::StorageOperationStatus::Valid
        }),
        "box input must transfer ownership: {flow:?}"
    );
}

#[test]
fn partially_moved_box_projects_through_policy_and_releases_on_both_exits() {
    let source = include_str!(
        "../../../../../../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    );

    let (declarations, _) = source.split_once("func main").unwrap();

    let compilation = compilation(declarations);
    let key = source_function_body_key(&compilation, "boxed");
    let analysis = compilation.async_analysis(key.clone()).unwrap();

    let release_instances = analysis
        .value()
        .storage_requirements()
        .iter()
        .flat_map(|requirement| requirement.parts().into_iter().flatten())
        .filter_map(bray_bound_tree::StorageCleanupPart::release)
        .map(|call| call.callable())
        .collect::<Vec<_>>();

    assert_eq!(release_instances.len(), 1, "{analysis:?}");

    let lowered = compilation.lowered_unit(key).unwrap();
    let mir = lowered.value().as_ref().unwrap().mir().unwrap();

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
    let mut cleanup_panic = false;
    let mut cleanup_cancellation = false;

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
                if block.kind() == MirBlockKind::Ordinary {
                    body_panic |= reaches_release(panicked.target());
                    body_cancellation |= reaches_release(cancelled.target());
                } else {
                    cleanup_panic |= reaches_release(panicked.target());
                    cleanup_cancellation |= reaches_release(cancelled.target());
                }
            }
            _ => {}
        }
    }

    assert!(
        normal && body_panic && body_cancellation && cleanup_panic && cleanup_cancellation,
        "normal exit, body panic/cancellation and cleanup panic/cancellation must each reach the policy release: {mir:?}"
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

    let (declarations, _) = source.split_once("func main").unwrap();

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
        let storage = compilation.storage_plan(key.clone()).unwrap();
        let calls = storage.value().owned_borrows().collect::<Vec<_>>();

        assert_eq!(calls.len(), 2, "{name}: {storage:?}");

        let (_, _, selected) = calls
            .iter()
            .find(|(_, candidate, _)| *candidate == kind)
            .unwrap();

        let lowered = compilation.lowered_unit(key.clone()).unwrap();
        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                MirOperationKind::Call(call) if matches!(call.target(),
                    bray_ir::MirCallTarget::Direct(reference) if reference.instance() == selected.callable())
            )), "{name} must use the selected {kind:?} policy borrow: {mir:?}");

        if matches!(name, "observe_box" | "observe_borrowed_box") {
            assert!(!mir.operations().iter().any(|operation| matches!(operation.kind(),
                    MirOperationKind::Store { destination, .. } if !destination.projections().is_empty()
                )), "observed aliases must not write back to source storage: {mir:?}");

            let analysis = compilation.async_analysis(key).unwrap();

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
        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");

        if matches!(name, "payload" | "nullable") {
            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

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

    let (declarations, _) = source.split_once("func main").unwrap();

    let source = format!(
        "{declarations}\nfunc invalid_borrow_move(pos pair: &box[PairStorage] Pair) {{ let box(value) = pair; consume_pair(value); }}"
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

    assert!(compilation.lowered_unit(key).unwrap().value().is_none());
}
