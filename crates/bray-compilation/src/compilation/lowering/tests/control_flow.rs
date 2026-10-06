use bray_ir::{MirAggregateKind, MirOperand, MirOperationKind, MirPanicCause, MirTerminatorKind};

use super::support::lowered_mir;
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};

const CONTROL_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "struct Point\n",
    "{\n",
    "    x: i32;\n",
    "    y: i32;\n",
    "}\n",
    "\n",
    "struct Items\n",
    "{\n",
    "}\n",
    "\n",
    "struct ItemsCursor\n",
    "{\n",
    "}\n",
    "\n",
    "impl &Items(Iterable)\n",
    "{\n",
    "    type Element = bool;\n",
    "    type Cursor = ItemsCursor;\n",
    "\n",
    "    consume func iterate() -> ItemsCursor\n",
    "    {\n",
    "        return ItemsCursor {};\n",
    "    }\n",
    "}\n",
    "\n",
    "impl ItemsCursor(Iterator)\n",
    "{\n",
    "    type Element = bool;\n",
    "\n",
    "    mut func next() -> bool?\n",
    "    {\n",
    "        panic();\n",
    "    }\n",
    "}\n",
    "\n",
    "func main()\n",
    "{\n",
    "    let point: Point = Point { x = 1, y = 2 };\n",
    "    let { x, y }: Point = point;\n",
    "    x;\n",
    "    y;\n",
    "\n",
    "    let logical: bool = true && false;\n",
    "    logical;\n",
    "\n",
    "    while false\n",
    "    {\n",
    "        continue;\n",
    "    }\n",
    "\n",
    "    loop\n",
    "    {\n",
    "        break;\n",
    "    }\n",
    "\n",
    "    match make_boolean()\n",
    "    {\n",
    "        case true\n",
    "        {\n",
    "        }\n",
    "        case false\n",
    "        {\n",
    "        }\n",
    "    }\n",
    "\n",
    "    let items: Items = Items {};\n",
    "    let every: bool = all(items);\n",
    "    let some: bool = any(items);\n",
    "\n",
    "    for item in items\n",
    "    {\n",
    "        item;\n",
    "    }\n",
    "\n",
    "    let branch: bool = if true\n",
    "    {\n",
    "        yield true;\n",
    "    }\n",
    "    else\n",
    "    {\n",
    "        yield false;\n",
    "    };\n",
    "}\n",
    "\n",
    "func make_boolean() -> bool\n",
    "{\n",
    "    return true;\n",
    "}\n",
);

const GENERATOR_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "struct Items\n",
    "{\n",
    "}\n",
    "\n",
    "struct ItemsCursor\n",
    "{\n",
    "}\n",
    "\n",
    "impl &Items(Iterable)\n",
    "{\n",
    "    type Element = bool;\n",
    "    type Cursor = ItemsCursor;\n",
    "\n",
    "    consume func iterate() -> ItemsCursor\n",
    "    {\n",
    "        return ItemsCursor {};\n",
    "    }\n",
    "}\n",
    "\n",
    "impl ItemsCursor(Iterator)\n",
    "{\n",
    "    type Element = bool;\n",
    "\n",
    "    mut func next() -> bool?\n",
    "    {\n",
    "        panic();\n",
    "    }\n",
    "}\n",
    "\n",
    "func main()\n",
    "{\n",
    "    let items: Items = Items {};\n",
    "    let lazy =\n",
    "    {\n",
    "        each item in items\n",
    "        {\n",
    "            if item\n",
    "            {\n",
    "                break;\n",
    "            }\n",
    "\n",
    "            yield item;\n",
    "            yield false;\n",
    "\n",
    "            let nested_items: Items = Items {};\n",
    "\n",
    "            each nested in nested_items\n",
    "            {\n",
    "                false;\n",
    "            }\n",
    "        }\n",
    "    };\n",
    "\n",
    "}\n",
);

const FAILURE_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main()\n",
    "{\n",
    "    assert(true);\n",
    "}\n",
);

const DIVERGING_ASSERTION_MESSAGE_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main()\n",
    "{\n",
    "    assert(false, panic(\"message\"));\n",
    "}\n",
);

const TERMINATING_ASSERTION_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func value() -> i32\n",
    "{\n",
    "    assert(false);\n",
    "}\n",
);

const RANGE_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "struct RangeSource\n",
    "{\n",
    "}\n",
    "\n",
    "impl &RangeSource(Iterable)\n",
    "{\n",
    "    type Element = i32;\n",
    "    type Cursor = Range<i32>;\n",
    "\n",
    "    consume func iterate() -> Range<i32>\n",
    "    {\n",
    "        return 1..3;\n",
    "    }\n",
    "}\n",
    "\n",
    "func main() -> i32\n",
    "{\n",
    "    let mut total: i32 = 0;\n",
    "\n",
    "    for value in (0..4)\n",
    "    {\n",
    "        total += value;\n",
    "    }\n",
    "\n",
    "    let source: RangeSource = RangeSource {};\n",
    "\n",
    "    for value in source\n",
    "    {\n",
    "        total += value;\n",
    "    }\n",
    "\n",
    "    let mut cursor: Range<i32> = 4..4;\n",
    "    let exhausted: i32? = cursor(Iterator).next();\n",
    "    let range: Range<i32> = 0..4;\n",
    "    let shared_cursor: Range<i32> = range(Iterable).iterate();\n",
    "    let moved_cursor: Range<i32> = (0..4)(Iterable).iterate();\n",
    "\n",
    "    return total;\n",
    "}\n",
);

#[test]
fn half_open_ranges_lower_to_aggregate_cursors_and_direct_advancement() {
    let compilation = compilation(RANGE_LOWERING_SOURCE);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("range MIR must be available: {error:?}"));

    let mir = lowered_mir(&lowered);

    assert!(mir.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Aggregate(aggregate)
            if aggregate.kind() == MirAggregateKind::Range
    )));

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(), MirOperationKind::Call(_)))
    );

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::RangeIterate { .. }
    )));

    assert!(
        !mir.blocks()
            .iter()
            .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Iterate { .. }))
    );
}

#[test]
fn discard_pattern_reads_parameter_initializer() {
    let compilation = compilation(concat!(
        "module app;\n",
        "\n",
        "func discard(pos value: usize)\n",
        "{\n",
        "    let _: usize = value;\n",
        "}\n",
    ));

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "discard"))
        .unwrap_or_else(|error| panic!("discard-pattern MIR must be available: {error:?}"));

    assert!(lowered.value().is_some(), "{lowered:#?}");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );
}

#[test]
fn value_producing_blocks_lower_through_a_result_join() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main() -> bool\n",
        "{\n",
        "    return\n",
        "    {\n",
        "        yield true;\n",
        "    };\n",
        "}\n",
    ));

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .unwrap_or_else(|error| panic!("value-producing block must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let mir = lowered_mir(&lowered);

    assert_eq!(mir.blocks().len(), 2);

    assert!(matches!(
        mir.blocks()[0].terminator().kind(),
        MirTerminatorKind::Goto(_)
    ));

    assert!(matches!(
        mir.blocks()[1].terminator().kind(),
        MirTerminatorKind::Return(Some(MirOperand::Value(_)))
    ));
}

#[test]
fn checked_control_patterns_and_iteration_lower_to_explicit_mir() {
    let compilation = compilation(CONTROL_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("control MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::PatternBranch { .. }
    )));

    assert!(
        mir.blocks()
            .iter()
            .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Iterate { .. }))
    );

    assert!(
        mir.blocks()
            .iter()
            .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Branch { .. }))
    );

    assert!(mir.operations().iter().any(|operation| {
        let mut projected_move = false;

        operation.kind().for_each_operand(|operand| {
            projected_move |=
                matches!(operand, MirOperand::Move(place) if !place.projections().is_empty());
        });

        projected_move
    }));

    assert!(mir.operations().iter().any(|operation| {
        let MirOperationKind::Call(call) = operation.kind() else {
            return false;
        };

        !call.witnesses().is_empty()
    }));
}

#[test]
fn else_if_conditions_lower_to_short_circuit_branches() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(pos first: bool, pos second: bool)\n",
        "{\n",
        "    if first\n",
        "    {\n",
        "    }\n",
        "    else if second\n",
        "    {\n",
        "    }\n",
        "    else\n",
        "    {\n",
        "    }\n",
        "}\n",
    ));

    let result = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("else-if MIR must be available: {error:?}"));

    let branch_count = lowered_mir(&result)
        .blocks()
        .iter()
        .filter(|block| matches!(block.terminator().kind(), MirTerminatorKind::Branch { .. }))
        .count();

    assert_eq!(branch_count, 2);
    assert!(result.diagnostics().is_empty(), "{result:?}");
}

#[test]
fn contextual_variant_patterns_do_not_require_superseded_binding_candidates() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    match make_choice()\n",
        "    {\n",
        "        case First\n",
        "        {\n",
        "        }\n",
        "    }\n",
        "}\n",
        "func make_choice() -> Choice\n",
        "{\n",
        "    return First;\n",
        "}\n",
    ));

    let result = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .unwrap_or_else(|error| panic!("contextual variant match must lower: {error:?}"));

    assert!(result.value().is_some(), "{:#?}", result.diagnostics());

    assert!(
        result.diagnostics().is_empty(),
        "{:#?}",
        result.diagnostics()
    );
}

#[test]
fn consumed_match_subject_uses_the_unprojected_union_storage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Outcome\n",
        "{\n",
        "    Value(pos value: usize);\n",
        "    Error;\n",
        "}\n",
        "func main(input: Outcome) -> usize\n",
        "{\n",
        "    match consume input\n",
        "    {\n",
        "        case Outcome.Value(value)\n",
        "        {\n",
        "            return value;\n",
        "        }\n",
        "\n",
        "        case Outcome.Error\n",
        "        {\n",
        "            return 0;\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let result = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("consumed union match must lower: {error:?}"));

    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );

    lowered_mir(&result);
}

#[test]
fn checked_general_generators_lower_to_accumulation_operations() {
    let compilation = compilation(GENERATOR_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("generator MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    let operations = mir
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Generator(operation) => Some(operation),
            _ => None,
        })
        .collect::<Vec<_>>();

    let [
        bray_ir::MirGeneratorOperation::Begin {
            kind,
            destination: begin,
            exact_count: None,
            ..
        },
        bray_ir::MirGeneratorOperation::Push {
            destination: push, ..
        },
        bray_ir::MirGeneratorOperation::Push {
            destination: second_push,
            ..
        },
        bray_ir::MirGeneratorOperation::Finish {
            destination: finish,
        },
    ] = operations.as_slice()
    else {
        panic!("general generators must initialize, push, and finish in order");
    };

    assert_eq!(*kind, bray_ir::MirGeneratorKind::General);
    assert_eq!(begin, push);
    assert_eq!(push, second_push);
    assert_eq!(second_push, finish);
}

#[test]
fn checked_assertion_lowers_to_explicit_failure_control() {
    let compilation = compilation(FAILURE_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("failure MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(mir.operations().iter().any(|operation| {
        matches!(
            operation.kind(),
            MirOperationKind::PanicReport(MirPanicCause::Assertion(None))
        )
    }));

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::BeginCleanup(_) | MirTerminatorKind::Panic { .. }
    )));
}

#[test]
fn diverging_assertion_messages_leave_the_success_path_available() {
    let compilation = compilation(DIVERGING_ASSERTION_MESSAGE_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("assertion MIR must be available: {error:?}"));

    assert!(
        result.value().is_some(),
        "a diverging failure message must not terminate the assertion success path: {:?}",
        result.diagnostics()
    );
}

#[test]
fn constant_false_assertions_terminate_non_unit_callables() {
    let compilation = compilation(TERMINATING_ASSERTION_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("assertion MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    assert!(
        mir.blocks()
            .iter()
            .all(|block| !matches!(block.terminator().kind(), MirTerminatorKind::Return(None)))
    );
}
