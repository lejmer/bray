use crate::test_support::{compilation, source_function_body_key};

#[test]
fn replacement_plans_capture_old_state_after_rhs_transfers() {
    use bray_bound_tree::{AsyncStorageCleanupRequirement, StorageReplacementState};

    for (body, expected, partial) in [
        (
            "let mut value: Guard = Guard {}; value = Guard {};",
            StorageReplacementState::Present,
            false,
        ),
        (
            "let mut value: Guard = Guard {}; let taken = value; value = Guard {};",
            StorageReplacementState::Absent,
            false,
        ),
        (
            "let mut value: Guard = Guard {}; value = value;",
            StorageReplacementState::Absent,
            false,
        ),
        (
            "let mut value: Guard = Guard {}; if flag { take(value); } value = Guard {};",
            StorageReplacementState::Conditional,
            false,
        ),
        (
            "let mut value: Pair = Pair { left = Guard {}, right = Guard {} }; let taken = value.left; value = Pair { left = Guard {}, right = Guard {} };",
            StorageReplacementState::Conditional,
            true,
        ),
    ] {
        let source = format!(
            "module app; struct Guard {{ destruct() {{}} }} struct Pair {{ left: Guard; right: Guard; }} func take(pos value: Guard) {{}} func probe(pos flag: bool) {{ {body} }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "probe");
        let flow = compilation.storage_flow(key.clone()).unwrap();

        let [decision] = flow.value().replacements() else {
            panic!("one assignment must publish one old-state decision: {flow:?}");
        };

        assert_eq!(decision.state(), expected, "{source}");

        let analysis = compilation.async_analysis(key.clone()).unwrap();

        let [plan] = analysis.value().replacements() else {
            panic!("one assignment must publish one cleanup plan: {analysis:?}");
        };

        assert_eq!(plan.expression(), decision.expression());
        assert_eq!(plan.access(), decision.access());
        assert_eq!(plan.parts().is_some(), partial, "{source}");

        assert_eq!(
            plan.cleanup() == AsyncStorageCleanupRequirement::None,
            expected == StorageReplacementState::Absent,
            "{source}"
        );

        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(
            lowered
                .value()
                .as_ref()
                .and_then(|unit| unit.mir())
                .is_some(),
            "{source}: {lowered:?}"
        );
    }
}

#[test]
fn nested_cleanup_native_fixture_lowers_checked_paths() {
    let compilation = compilation(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../xtask/fixtures/composition/cleanup/main.bray"
    )));

    for name in [
        "guard_id",
        "make_guard",
        "pending_result",
        "nested_cleanup",
        "nested_replacement",
    ] {
        let key = source_function_body_key(&compilation, name);

        assert!(
            compilation
                .bound_unit(key.clone())
                .unwrap()
                .diagnostics()
                .is_empty(),
            "{name}"
        );

        let lowered = compilation.lowered_unit(key);

        assert!(lowered.is_ok(), "{name}: {lowered:?}");
        assert!(lowered.unwrap().value().is_some(), "{name}");
    }

    assert!(compilation.check_diagnostics().is_empty());
}

#[test]
fn replacement_native_fixture_lowers_checked_cleanup_paths() {
    let compilation = compilation(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../xtask/fixtures/native-execution/value-replacement.bray"
    )));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in [
        "ordinary_scope_cleanup",
        "heap_replacement",
        "pending_return",
        "abandoned_return",
        "return_across_catch",
        "exited_catch_does_not_handle_outer_cleanup",
        "owned_cancellation",
        "borrowed_task",
        "borrowed_cancellation",
        "main",
    ] {
        let lowered = compilation.lowered_unit(source_function_body_key(&compilation, name));

        assert!(lowered.is_ok(), "{name}: {lowered:?}");
        assert!(lowered.unwrap().value().is_some(), "{name}");
    }
}

#[test]
fn replacement_lowering_covers_borrowed_projected_and_nullable_destinations() {
    for body in [
        "slot.value = Guard {};",
        "let mut value: Guard? = Guard {}; value = none;",
        "let mut values: [Guard; 2] = [Guard {}, Guard {}]; values[index()] = Guard {};",
    ] {
        let source = format!(
            "module app; struct Guard {{ destruct() {{}} }} struct Slot {{ mut value: Guard; }} func index() -> usize {{ return 1; }} func probe(pos slot: &mut Slot) {{ {body} }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "probe");
        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(
            lowered
                .value()
                .as_ref()
                .and_then(|unit| unit.mir())
                .is_some(),
            "{source}: {lowered:?}"
        );
    }
}
