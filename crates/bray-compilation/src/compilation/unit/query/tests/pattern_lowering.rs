use crate::test_support::{compilation, source_function_body_key};

use bray_diagnostics::DiagnosticKind;

use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn pattern_conditions_lower_with_guarded_temporary_cleanup() {
    let compilation = compilation(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../xtask/fixtures/native-execution/pattern-conditions.bray"
    )));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in ["next_value", "probe_return", "main"] {
        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, name))
            .unwrap();

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");
    }
}

#[test]
fn observed_call_result_patterns_evaluate_their_subject_once() {
    for body in [
        "match make() { case ?value { let (first, second) = value; return first; } case none { return 0; } }",
        "if let ?value = make() { let (first, second) = value; return first; } return 0;",
    ] {
        for subject in [
            "make()",
            "trusted make()",
            "trusted internal make()",
            "(trusted internal make())",
        ] {
            let body = body.replace("make()", subject);

            let source = format!(
                "trusted module app; internal func make() -> (i32, bool)? {{ return (7, true); }} func main() -> i32 {{ {body} }}"
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, "main"))
                .unwrap();

            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            let calls = mir
                .operations()
                .iter()
                .filter(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Call(_)))
                .count();

            assert_eq!(
                calls, 1,
                "the subject must be evaluated only before entering the arm: {mir:?}"
            );
        }
    }
}

#[test]
fn singleton_member_moves_preserve_containing_lifecycle_obligations() {
    for lifecycle in [
        "",
        "destruct() {}",
        "finalize() {}",
        "consume enter() -> bool { return true; } exit(pos lease: bool) {}",
    ] {
        for (declaration, body) in [
            (
                format!("struct Wrapper {{ value: Item; {lifecycle} }}"),
                "return wrapper.value;",
            ),
            (
                format!("union Wrapper {{ Only(value: Item); {lifecycle} }}"),
                "match wrapper { case Only(value = item) { return item; } }",
            ),
        ] {
            let source = format!(
                "module app; struct Item {{}} {declaration}
                    func unpack(pos wrapper: Wrapper) -> Item {{ {body} }}"
            );

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "unpack");
            let storage = compilation.storage_plan(key.clone()).unwrap();
            let flow = compilation.storage_flow(key.clone()).unwrap();

            if lifecycle.is_empty() {
                assert!(
                    !flow.diagnostics().has_errors(),
                    "{source}: {:?}",
                    flow.diagnostics()
                );
            } else {
                assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    DiagnosticKind::CheckingIncompleteLifecycleStorage,
                );
            }

            let root = storage
                .value()
                .identity_entries()
                .find_map(|(id, identity)| {
                    matches!(identity, bray_bound_tree::StorageIdentity::Parameter(_)).then_some(id)
                })
                .unwrap();

            assert!(
                flow.value().operations().iter().any(|operation| {
                    operation.status() == bray_bound_tree::StorageOperationStatus::Valid
                        && operation.purpose() == bray_bound_tree::StorageAccessPurpose::Move
                        && storage.value().root_identity(operation.access()) == Some(root)
                        && !storage.value().is_root_access(operation.access())
                }),
                "{source}"
            );

            if lifecycle.is_empty() {
                assert!(
                    flow.value()
                        .exits()
                        .iter()
                        .all(|exit| !exit.live().contains(&root)),
                    "{source}"
                );
            } else {
                assert!(
                    flow.value().exits().iter().any(|exit| exit
                        .moved()
                        .iter()
                        .any(|access| storage.value().root_identity(*access) == Some(root))),
                    "{source}"
                );

                assert!(
                    !flow
                        .value()
                        .exits()
                        .iter()
                        .any(|exit| exit.fully_moved().contains(&root)),
                    "{source}"
                );

                let analysis = compilation.async_analysis(key).unwrap();

                bray_testing::assert_goal_state_diagnostic_kind(
                    analysis.diagnostics(),
                    DiagnosticKind::CheckingIncompleteLifecycleStorage,
                );

                assert!(analysis.value().scope_exits().iter().flat_map(|exit| exit.storage()).any(|decision|
                        decision.identity() == root && decision.disposition() ==
                            bray_bound_tree::AsyncStorageExitDisposition::Recovered(
                                bray_bound_tree::AsyncStorageExitRecoveryCause::UnavailablePartialCleanup)),
                        "{source}: {:?}", analysis.value());
            }
        }
    }
}
