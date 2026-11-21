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

        let flow = compilation
            .storage_flow(key.clone())
            .expect("fixture callable must complete storage-flow analysis");

        let [decision] = flow.value().replacements() else {
            panic!("one assignment must publish one old-state decision: {flow:?}");
        };

        assert_eq!(decision.state(), expected, "{source}");

        assert_eq!(decision.parts().is_some(), partial, "{source}");

        assert_eq!(
            decision.cleanup() == AsyncStorageCleanupRequirement::None,
            expected == StorageReplacementState::Absent,
            "{source}"
        );

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

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

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

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
