use bray_ir::{MirOperationKind, MirTerminatorKind};
use bray_testing::assert_goal_state_diagnostic_kind;

use super::support::{lowered_mir, standard_text_compilation};
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};

const RESULT_PROPAGATION_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main(pos value: Result<i32, i32>) -> Result<i32, i32>\n",
    "{\n",
    "    let unwrapped: i32 = try value;\n",
    "\n",
    "    return Ok(unwrapped);\n",
    "}\n",
);

const CONVERTED_RESULT_PROPAGATION_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main(\n",
    "    pos value: Result<i32, i32>,\n",
    "    pos fallback: Result<i32, i64>,\n",
    ") -> Result<i32, i64>\n",
    "{\n",
    "    let unwrapped: i32 = try value;\n",
    "\n",
    "    return fallback;\n",
    "}\n",
);

const INCOMPATIBLE_RESULT_PROPAGATION_SOURCE: &str = r#"
module app;

func main(pos value: Result<i32, i32>) -> i32
{
    return try value;
}
"#;

const INCOMPATIBLE_NULLABLE_PROPAGATION_SOURCE: &str = r#"
module app;

func main(pos value: i32?) -> i32
{
    return value?;
}
"#;

#[test]
fn propagated_nested_scope_exit_publishes_its_cleanup_plan() {
    let compilation = standard_text_compilation(&[
        include_str!("../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../../standard-library/std/src/bytes/buffer.bray"),
        include_str!("../../../../../../standard-library/std/src/collection/slot.bray"),
        include_str!("../../../../../../standard-library/std/src/collection/list.bray"),
        include_str!("../../../../../../standard-library/std/src/collection/deque.bray"),
        include_str!("../../../../../../standard-library/std/src/collection/ring.bray"),
        r#"module app;

using std.collection;
using std.bytes;
using std.memory;

struct Probe
{
    bytes: std.bytes.Buffer;
}

trusted func main() -> Result<unit, std.memory.MemoryLayoutError>
{
    {
        let bytes: [u8; 1] = [7];
        let mut values: std.collection.Deque<Probe> = try trusted std.collection.Deque<Probe>();

        try trusted values.push_back(
            {
                bytes = try std.bytes.Buffer.from_slice(&bytes[..]),
            }
        );
    }

    return Ok(unit);
}
"#,
    ]);

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .unwrap_or_else(|error| panic!("nested propagated cleanup must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(lowered.value().is_some(), "{lowered:#?}");
}

#[test]
fn nullable_nested_scope_exit_publishes_its_cleanup_plan() {
    let compilation = compilation(
        r#"module app;

struct Probe
{
    destruct() {}
}

func main(pos value: i32?) -> i32?
{
    {
        let probe: Probe = Probe {};
        let unwrapped: i32 = value?;
        let mut result: i32? = none;

        result = unwrapped;

        return result;
    }
}
"#,
    );

    let key = source_function_body_key(&compilation, "main");

    let bound = compilation
        .bound_unit(key.clone())
        .unwrap_or_else(|error| panic!("nullable unit must bind: {error:?}"));

    assert!(bound.diagnostics().is_empty(), "{:#?}", bound.diagnostics());

    let types = compilation
        .expression_types(key.clone())
        .unwrap_or_else(|error| panic!("nullable expressions must type: {error:?}"));

    assert!(types.diagnostics().is_empty(), "{:#?}", types.diagnostics());

    let patterns = compilation
        .patterns(key.clone())
        .unwrap_or_else(|error| panic!("nullable patterns must check: {error:?}"));

    assert!(
        patterns.diagnostics().is_empty(),
        "{:#?}",
        patterns.diagnostics()
    );

    let storage = compilation
        .storage_plan(key.clone())
        .unwrap_or_else(|error| panic!("nullable storage must plan: {error:?}"));

    assert!(
        storage.diagnostics().is_empty(),
        "{:#?}",
        storage.diagnostics()
    );

    compilation
        .storage_flow(key.clone())
        .unwrap_or_else(|error| panic!("nullable storage flow must publish: {error:?}"));

    compilation
        .async_analysis(key.clone())
        .unwrap_or_else(|error| panic!("nullable cleanup plans must publish: {error:?}"));

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("nested nullable cleanup must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    assert!(lowered.value().is_some(), "{lowered:#?}");
}

#[test]
fn checked_result_propagation_lowers_success_and_error_paths() {
    let compilation = compilation(RESULT_PROPAGATION_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("result propagation MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::PatternBranch {
            predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(_),
            ..
        }
    )));

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::PatternProjection {
            projection: bray_bound_tree::PatternProjection::ActiveUnionPayloadField { .. },
            ..
        }
    )));
}

#[test]
fn result_propagation_applies_the_checked_error_conversion() {
    let compilation = compilation(CONVERTED_RESULT_PROPAGATION_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("result propagation MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(), MirOperationKind::Convert { .. }))
    );
}

#[test]
fn result_propagation_requires_a_compatible_lexical_boundary() {
    let compilation = compilation(INCOMPATIBLE_RESULT_PROPAGATION_SOURCE);
    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

    assert_goal_state_diagnostic_kind(
        selections.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );

    let lowered = compilation
        .lowered_unit(key)
        .expect("rejected propagation retains its source diagnostic");

    assert!(lowered.value().is_none());

    assert_goal_state_diagnostic_kind(
        lowered.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );
}

#[test]
fn nullable_propagation_requires_a_nullable_lexical_boundary() {
    let compilation = compilation(INCOMPATIBLE_NULLABLE_PROPAGATION_SOURCE);
    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

    assert_goal_state_diagnostic_kind(
        selections.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );

    let lowered = compilation
        .lowered_unit(key)
        .expect("rejected propagation retains its source diagnostic");

    assert!(lowered.value().is_none());

    assert_goal_state_diagnostic_kind(
        lowered.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );
}

#[test]
fn patterns_observe_the_transferred_propagation_payload() {
    for (input, propagated, output, success, failure) in [
        (
            "Result<Choice, i32>",
            "try input",
            "Result<i32, i32>",
            "Ok(first.read() + second.read())",
            "Ok(0)",
        ),
        (
            "RunResult<Choice>",
            "try input",
            "i32",
            "first.read() + second.read()",
            "0",
        ),
        (
            "Choice?",
            "input?",
            "i32?",
            "first.read() + second.read()",
            "none",
        ),
    ] {
        let application = compilation(&format!(
            r#"
            module app;

            struct Value
            {{
                number: i32;

                destruct() {{}}

                func read() -> i32
                {{
                    return self.number;
                }}
            }}

            union Choice
            {{
                Pair(pos first: Value, pos second: Value);
                Empty;
            }}

            func inspect(pos input: {input}) -> {output}
            {{
                if let Pair(first, second) = {propagated}
                {{
                    return {success};
                }}

                return {failure};
            }}
        "#
        ));

        let diagnostics = application.check_diagnostics();

        assert!(diagnostics.is_empty(), "{input}: {diagnostics:?}");

        let result = application
            .lowered_unit(source_function_body_key(&application, "inspect"))
            .expect("propagated pattern subject must lower");

        let mir = lowered_mir(&result);

        let observed = mir
            .operations()
            .iter()
            .filter_map(|operation| {
                let MirOperationKind::Borrow { place, .. } = operation.kind() else {
                    return None;
                };

                place
                    .projections()
                    .last()
                    .is_some_and(|projection| {
                        matches!(
                            projection.kind(),
                            bray_ir::MirProjectionKind::ActiveUnionPayloadField { .. }
                        )
                    })
                    .then_some(place)
            })
            .collect::<Vec<_>>();

        assert_eq!(
            observed.len(),
            2,
            "both payload observers must be present: {mir:?}"
        );

        for place in observed {
            assert_eq!(
                place.projections().len(),
                1,
                "borrow must start at the transferred Choice, not its old container: {mir:?}"
            );

            assert!(mir.operations().iter().any(|operation| matches!(
                operation.kind(), MirOperationKind::Store { destination, .. }
                    if destination.storage() == place.storage() && destination.projections().is_empty()
            )), "the observed result storage must be initialized: {mir:?}");
        }
    }
}

#[test]
fn propagation_does_not_leave_the_consumed_container_available() {
    let application = compilation(
        r#"
        module app;

        struct Value
        {
            number: i32;

            destruct() {}
        }

        func inspect(pos input: Result<Value, i32>) -> Result<Value, i32>
        {
            let extracted = try input;

            return input;
        }
    "#,
    );

    assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingUseOfMovedStorage,
    );
}
