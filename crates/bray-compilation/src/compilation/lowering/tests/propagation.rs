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
    "    return value;\n",
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

const INCOMPATIBLE_RESULT_PROPAGATION_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main(pos value: Result<i32, i32>) -> i32\n",
    "{\n",
    "    return try value;\n",
    "}\n",
);

const INCOMPATIBLE_NULLABLE_PROPAGATION_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main(pos value: i32?) -> i32\n",
    "{\n",
    "    return value?;\n",
    "}\n",
);

#[test]
fn propagated_nested_scope_exit_publishes_its_cleanup_plan() {
    let compilation = standard_text_compilation(&[
        include_str!("../../../../../../standard-library/std/src/memory.bray"),
        include_str!("../../../../../../standard-library/std/src/bytes/buffer.bray"),
        include_str!("../../../../../../standard-library/std/src/collection/list.bray"),
        include_str!("../../../../../../standard-library/std/src/collection/deque.bray"),
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
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

    assert_goal_state_diagnostic_kind(
        selections.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );
}

#[test]
fn nullable_propagation_requires_a_nullable_lexical_boundary() {
    let compilation = compilation(INCOMPATIBLE_NULLABLE_PROPAGATION_SOURCE);
    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key)
        .unwrap_or_else(|error| panic!("semantic selections must be available: {error:?}"));

    assert_goal_state_diagnostic_kind(
        selections.diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
    );
}
