use crate::test_support::{
    compilation, source_callable_body_key, source_function_body_key,
    source_trait_callable_fulfillment_body_key, source_type_callable_member_body_key,
};
use bray_bound_tree::SemanticSelection;
use bray_diagnostics::DiagnosticKind;

use bray_symbols::TypeData;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn mutable_receiver_authority_reaches_mutable_projected_fields() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Writer\n",
        "{\n",
        "    mut func write(pos value: i32);\n",
        "}\n",
        "struct Wrapper<Sink>\n",
        "{\n",
        "    mut sink: Sink;\n",
        "}\n",
        "impl Wrapper<Sink>\n",
        "{\n",
        "    mut func forward()\n",
        "        with(Sink: Writer)\n",
        "    {\n",
        "        self.sink.write(1);\n",
        "    }\n",
        "}\n",
    ));

    let key = source_type_callable_member_body_key(&compilation, "forward");

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("method selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    assert!(selections.value().entries().iter().any(|entry| {
        matches!(
            entry.selection(),
            SemanticSelection::Call(call) if call.resolution().trait_dispatch().is_some()
        )
    }));
}

#[test]
fn implicit_call_reborrows_preserve_local_borrow_values() {
    for body in [
        r#"increment(counter);
    increment(counter);"#,
        r#"let local: &mut Counter = counter;

    increment(local);
    increment(local);"#,
        r#"let local: &mut Counter = counter;
    let mut n: usize = 2;

    while n != 0
    {
        increment(local);
        n -= 1;
    }"#,
        r#"let local: &mut Counter = same(counter);

    increment(local);
    increment(local);"#,
        r#"increment(same(counter));
    increment(same(counter));"#,
        r#"let slot: Slot = Slot
    {
        value = counter
    };

    increment(slot.value);
    increment(slot.value);"#,
        r#"let local: &mut Counter = counter;

    increment(counter = local);
    increment(counter = local);"#,
        r#"let local: &mut Counter = counter;
    let operation: func(pos counter: &mut Counter) = increment;

    operation(local);
    operation(local);"#,
        r#"let mut local: &mut Counter = counter;

    increment(&mut local);
    increment(&mut local);"#,
        r#"let mut local: &mut Counter = counter;
    let outer: &mut &mut Counter = &mut local;

    nested(outer);
    nested(outer);"#,
        r#"let local: &mut Counter = counter;

    generic(local);
    generic(local);"#,
        r#"let mut fresh: Counter = Counter
    {
        value = 0
    };

    take(&mut fresh);
    increment(&mut fresh);"#,
    ] {
        let source = format!(
            r#"module app;

struct Counter
{{
    mut value: usize;
}}

func increment(pos counter: &mut Counter)
{{
    counter.value += 1;
}}

struct Slot
{{
    value: &mut Counter;
}}

func same(pos counter: &mut Counter) -> &mut Counter
{{
    return counter;
}}

func nested(pos counter: &mut &mut Counter)
{{
}}

func generic<T>(pos value: &mut T)
{{
}}

func take<T>(pos value: T)
{{
}}

func run(pos counter: &mut Counter)
{{
    {body}
}}
"#
        );

        let compilation = compilation(&source);
        let key = source_function_body_key(&compilation, "run");

        let flow = compilation
            .storage_flow(key.clone())
            .expect("call reborrow flow");

        assert!(
            !flow.diagnostics().has_errors(),
            "{body}: {:?}",
            flow.diagnostics()
        );

        let lowered = compilation
            .lowered_unit(key)
            .expect("call reborrow lowering");

        assert!(
            !lowered.diagnostics().has_errors(),
            "{body}: {:?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();
        let values = compilation.semantic_value_store().unwrap();

        for operation in mir.operations() {
            let bray_ir::MirOperationKind::Borrow { place, .. } = operation.kind() else {
                continue;
            };

            let result = mir.value(operation.result().unwrap()).unwrap();
            let ty = values.type_data(result.ty());

            assert!(
                matches!(ty.as_ref(), TypeData::Borrow { target, .. } if *target == place.ty()),
                "borrow must preserve the reached layer: {body}: {operation:?}",
            );
        }
    }
}

#[test]
fn implicit_call_reborrows_preserve_owned_transfers_and_escaped_loans() {
    let moved = DiagnosticKind::CheckingUseOfMovedStorage;
    let conflict = DiagnosticKind::CheckingConflictingBorrow;

    for (body, expected) in [
        (
            r#"let local: &mut Counter = counter;
    let moved: &mut Counter = local;

    increment(local);"#,
            moved,
        ),
        (
            r#"let local: &mut Counter = counter;

    take(local);
    increment(local);"#,
            moved,
        ),
        (
            r#"take(counter);
    increment(counter);"#,
            moved,
        ),
        (
            r#"let local: &mut Counter = counter;
    let escaped: &mut Counter = same(local);

    increment(local);
    increment(escaped);"#,
            conflict,
        ),
    ] {
        let source = format!(
            r#"module app;

struct Counter
{{
    mut value: usize;
}}

func increment(pos counter: &mut Counter)
{{
    counter.value += 1;
}}

func take<T>(pos value: T)
{{
}}

func same(pos counter: &mut Counter) -> &mut Counter
{{
    return counter;
}}

func run(pos counter: &mut Counter)
{{
    {body}
}}
"#
        );

        let compilation = compilation(&source);
        let key = source_function_body_key(&compilation, "run");

        let flow = compilation
            .storage_flow(key)
            .unwrap_or_else(|error| panic!("{body}: {error:?}"));

        assert_goal_state_diagnostic_kind(flow.diagnostics(), expected);
    }
}

#[test]
fn implicit_call_reborrows_reject_overlapping_arguments() {
    for body in [
        "pair(counter, counter);",
        r#"let local: &mut Counter = counter;

    pair(local, local);"#,
        r#"let mut local: &mut Counter = counter;

    pair(local, local);"#,
        r#"let mut slot: Slot = Slot
    {
        value = counter
    };

    pair(slot.value, slot.value);"#,
    ] {
        let source = format!(
            r#"module app;

struct Counter
{{
    mut value: usize;
}}

struct Slot
{{
    mut value: &mut Counter;
}}

func pair(pos first: &mut Counter, pos second: &mut Counter)
{{
}}

func run(pos counter: &mut Counter)
{{
    {body}
}}
"#
        );

        let compilation = compilation(&source);
        let key = source_function_body_key(&compilation, "run");

        let flow = compilation
            .storage_flow(key)
            .expect("overlapping argument flow");

        assert_goal_state_diagnostic_kind(
            flow.diagnostics(),
            DiagnosticKind::CheckingConflictingBorrow,
        );
    }
}

#[test]
fn implicit_call_reborrows_preserve_field_authority_and_disjointness() {
    for (body, expected) in [
        (
            r#"func run(pos slots: &mut Slots)
{
    pair(slots.first, slots.second);
    pair(slots.first, slots.second);
}"#,
            None,
        ),
        (
            r#"func run(pos slots: &mut Slots)
{
    slots.first.fixed = 1;
}"#,
            Some(DiagnosticKind::CheckingMissingMutationAuthority),
        ),
        (
            r#"func run(pos slots: &Slots)
{
    pair(slots.first, slots.second);
}"#,
            Some(DiagnosticKind::CheckingMissingMutationAuthority),
        ),
    ] {
        let source = format!(
            r#"module app;

struct Counter
{{
    mut value: usize;
    fixed: usize;
}}

struct Slots
{{
    first: &mut Counter;
    second: &mut Counter;
}}

func pair(pos first: &mut Counter, pos second: &mut Counter)
{{
    first.value += 1;
    second.value += 1;
}}

{body}
"#
        );

        let compilation = compilation(&source);
        let key = source_function_body_key(&compilation, "run");

        let flow = compilation
            .storage_flow(key.clone())
            .expect("field reborrow flow");

        if let Some(expected) = expected {
            assert!(flow.diagnostics().has_errors(), "{body}");
            assert_goal_state_diagnostic_kind(flow.diagnostics(), expected);
        } else {
            assert!(
                !flow.diagnostics().has_errors(),
                "{body}: {:?}",
                flow.diagnostics()
            );

            let lowered = compilation
                .lowered_unit(key)
                .expect("disjoint field reborrows lower");

            assert!(
                !lowered.diagnostics().has_errors(),
                "{body}: {:?}",
                lowered.diagnostics()
            );
        }
    }
}

#[test]
fn mutable_borrow_receivers_reborrow_for_mutable_methods() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Writer\n",
        "{\n",
        "    mut func write(pos value: i32);\n",
        "}\n",
        "func forward<Sink>(pos sink: &mut Sink)\n",
        "    with(Sink: Writer)\n",
        "{\n",
        "    sink.write(1);\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("borrowed receiver selection must publish: {error:?}"));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("borrowed receiver call must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );
}

#[test]
fn mutable_trait_fulfillment_receivers_authorize_field_assignment() {
    let compilation = compilation(concat!(
        "module app;\n",
        "trait Swappable\n",
        "{\n",
        "    mut func swap();\n",
        "}\n",
        "struct Pair\n",
        "{\n",
        "    mut left: i32;\n",
        "    mut right: i32;\n",
        "}\n",
        "impl PairSwappable = Pair(Swappable)\n",
        "{\n",
        "    mut func swap()\n",
        "    {\n",
        "        let left: i32 = self.left;\n",
        "\n",
        "        self.left = self.right;\n",
        "        self.right = left;\n",
        "    }\n",
        "}\n",
    ));

    let key = source_trait_callable_fulfillment_body_key(&compilation, "swap");

    let flow = compilation
        .storage_flow(key)
        .unwrap_or_else(|error| panic!("fulfillment storage flow must publish: {error:?}"));

    assert!(
        flow.value().operations().iter().all(|operation| {
            operation.status() != bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
        }),
        "{flow:?}"
    );
}
