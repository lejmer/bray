use bray_ir::{MirAggregateKind, MirOperand, MirOperationKind, MirPanicCause, MirTerminatorKind};

use super::support::lowered_mir;
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};

#[test]
fn borrowed_nullable_patterns_test_the_payload_discriminant() {
    for parameter in ["&i32?", "&mut i32?", "(&i32)?"] {
        for (input, condition) in [
            (parameter, "let ?present = value"),
            (parameter, "value matches none"),
            ("Wrapper", "let { slot = ?present } = value"),
            ("Wrapper", "value matches { slot = none }"),
        ] {
            let source = format!(
                r#"
                module app;
                struct Wrapper {{ slot: {parameter}; }}
                func inspect(pos value: {input}) -> bool {{
                    if {condition} {{ return true; }}
                    return false;
                }}
            "#
            );

            let compilation = compilation(&source);

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, "inspect"))
                .expect("fixture callable must lower successfully");

            assert!(
                !lowered.diagnostics().has_errors(),
                "{source}: {lowered:#?}"
            );

            let mir = lowered_mir(&lowered);
            let mut tested = false;

            for block in mir.blocks() {
                if let MirTerminatorKind::PatternBranch {
                    subject: MirOperand::Copy(place),
                    predicate:
                        bray_ir::MirPatternPredicate::NullableAbsent
                        | bray_ir::MirPatternPredicate::NullablePresent,
                    ..
                } = block.terminator().kind()
                {
                    assert!(
                        matches!(
                            compilation
                                .semantic_value_store()
                                .expect("fixture compilation must provide its semantic value store")
                                .type_data(place.ty())
                                .as_ref(),
                            bray_symbols::TypeData::Nullable(_)
                        ),
                        "{source}: {place:#?}"
                    );

                    tested = true;
                }
            }

            assert!(tested, "fixture must retain its nullable branch: {source}");
        }
    }
}

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
    "        panic(\"unreachable\");\n",
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

const GENERATOR_LOWERING_SOURCE: &str = r#"
    module app;

    struct Items
    {
    }
    struct ItemsCursor
    {
        mut remaining: u32;
    }

    impl &Items(Iterable)
    {
        type Element = bool;
        type Cursor = ItemsCursor;

        consume func iterate() -> ItemsCursor
        {
            return ItemsCursor { remaining = 2 };
        }
    }

    impl ItemsCursor(Iterator)
    {
        type Element = bool;

        mut func next() -> bool?
        {
            if self.remaining == 0
            {
                return none;
            }

            self.remaining -= 1;
            return self.remaining == 0;
        }
    }

    func main()
    {
        let items: Items = Items {};
        let lazy =
        {
            each item in items
            {
                if item
                {
                    break;
                }

                yield 1;
                yield 2;

                let nested_items: Items = Items {};
                each nested in nested_items
                {
                    false;
                }
            }
        };
    }
"#;

const FAILURE_LOWERING_SOURCE: &str = concat!(
    "module app;\n",
    "\n",
    "func main(pos condition: bool)\n",
    "{\n",
    "    assert(condition);\n",
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
}

#[test]
fn discard_pattern_reads_parameter_initializer() {
    let compilation = compilation(concat!(
        "module app;\n",
        "\n",
        "func discard(pos value: usize)\n",
        "{\n",
        "    _ = value;\n",
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
        MirTerminatorKind::Return {
            value: Some(MirOperand::Value(_)),
            ..
        }
    ));
}

#[test]
fn checked_control_patterns_and_iteration_lower_to_explicit_mir() {
    let compilation = compilation(CONTROL_LOWERING_SOURCE);
    let key = source_callable_body_key(&compilation);

    let result = compilation
        .lowered_unit(key.clone())
        .unwrap_or_else(|error| panic!("control MIR must be available: {error:?}"));

    let mir = lowered_mir(&result);

    let bound = compilation
        .bound_unit(key)
        .expect("fixture callable must bind successfully");

    let owners = bray_bound_tree::StorageScopeOwners::collect(bound.value())
        .expect("fixture `bray_bound_tree::StorageScopeOwners::collect` call must succeed");

    let mut iteration_bodies = 0;

    for (expression, node) in bound.value().tree().expressions() {
        if let bray_bound_tree::BoundExpression::For(loop_) = node {
            assert_eq!(
                owners.scope(Some(bray_bound_tree::StorageIdentity::InvocationResult(
                    bray_bound_tree::BoundExecutionSite::Next(expression)
                ))),
                Some(loop_.body().into())
            );

            assert_eq!(
                owners.scope(Some(bray_bound_tree::StorageIdentity::InvocationResult(
                    bray_bound_tree::BoundExecutionSite::Iterate(expression)
                ))),
                Some(bray_bound_tree::StorageScope::Iteration(expression))
            );

            iteration_bodies += 1;
        }
    }

    assert!(iteration_bodies > 0);

    let symbols = compilation
        .symbol_graph()
        .expect("fixture compilation must build its symbol graph");

    let mut checked_next_calls = 0;

    for block in mir.blocks() {
        let calls_next = block.operations().iter().any(|operation| {
            let operation = mir
                .operation(*operation)
                .expect("MIR block retains its operation");

            let MirOperationKind::Call(call) = operation.kind() else {
                return false;
            };

            let bray_ir::MirCallTarget::Direct(target) = call.target() else {
                return false;
            };

            symbols
                .member_name(target.instance().definition().symbol())
                .is_some_and(|name| name.as_str() == "next")
        });

        if calls_next {
            // This iterator panics. Its call must pass through ordinary failure cleanup,
            // before any result is inspected or yielded as an element.
            assert!(matches!(
                block.terminator().kind(),
                MirTerminatorKind::CheckCallOutcome { .. }
            ));

            checked_next_calls += 1;
        }
    }

    assert!(
        checked_next_calls >= 3,
        "both folds and the for loop must use checked next calls"
    );

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::PatternBranch { .. }
    )));

    assert!(mir.operations().iter().any(|operation|
        matches!(operation.kind(), MirOperationKind::Call(call) if !call.witnesses().is_empty())));

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::PatternBranch {
            predicate: bray_ir::MirPatternPredicate::NullablePresent,
            ..
        }
    )));

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
            MirOperationKind::Memory(memory)
                if matches!(
                    memory.kind(),
                    bray_bound_tree::CheckedMemoryOperationKind::RawBufferPush { .. }
                ) =>
            {
                Some(memory)
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        operations.len(),
        2,
        "general generators publish both yielded elements"
    );

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must be available");

    let buffer_type = values.type_data(operations[0].operand_types()[0]);

    let bray_symbols::TypeData::Borrow { target: owner, .. } = buffer_type.as_ref() else {
        panic!("generator publication borrows its accumulation owner");
    };

    let result_type = values.type_data(*owner);

    let bray_symbols::TypeData::Generator(result_element) = result_type.as_ref() else {
        panic!("general generator must retain its result element type");
    };

    for operation in operations {
        assert_eq!(operation.operand_types()[1], *result_element);

        assert_eq!(
            mir.operand_type(&operation.operands()[1]),
            Some(*result_element)
        );
    }

    assert!(
        mir.operations()
            .iter()
            .any(|operation| matches!(operation.kind(),
        MirOperationKind::AdmitOutgoing { owner: bray_ir::MirOutgoingOwner::Value(ty), .. }
            if ty == owner))
    );

    assert!(mir.blocks().iter().any(|block| matches!(
        block.terminator().kind(),
        MirTerminatorKind::CheckCallOutcome { .. }
    )));
}

#[test]
fn general_generator_retains_its_partial_owner_on_panic() {
    let compilation = compilation(
        r#"
        module app;

        func main()
        {
            let generated =
            {
                each item in 0..3
                {
                    if item == 1
                    {
                        panic("failed element production");
                    }

                    yield item;
                }
            };
        }
        "#,
    );

    let lowered = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .expect("generator must lower");

    let mir = lowered_mir(&lowered);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must be available");

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Cleanup { phase: bray_ir::MirCleanupPhase::LifecycleResolution, place, .. }
            if matches!(values.type_data(place.ty()).as_ref(), bray_symbols::TypeData::Generator(_)))));
}

#[test]
fn array_generator_uses_yielded_element_type() {
    let compilation = compilation(
        r#"
        module app;

        func main()
        {
            let generated: [bool; 4] = [each item in 0..4
            {
                yield item > 1;
            }];
        }
        "#,
    );

    let result = compilation
        .lowered_unit(source_callable_body_key(&compilation))
        .expect("array generator must lower");

    let mir = lowered_mir(&result);

    let destination = mir
        .operations()
        .iter()
        .find_map(|operation| match operation.kind() {
            MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination,
                ..
            } if destination.projections().iter().any(|projection| {
                matches!(projection.kind(), bray_ir::MirProjectionKind::Index(_))
            }) =>
            {
                Some(destination)
            }
            _ => None,
        })
        .expect("array generator must initialize its yielded element");

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must be available");

    let projection = destination
        .projections()
        .last()
        .expect("array element has a projection");

    let result_type = values.type_data(projection.source_type());

    let bray_symbols::TypeData::Array {
        element: result_element,
        ..
    } = result_type.as_ref()
    else {
        panic!("array generator must retain its result element type");
    };

    assert_eq!(destination.ty(), *result_element);
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
fn established_assertions_do_not_lower_failure_messages_or_reports() {
    for body in [
        "assert(true);",
        "let value: i32 = 1; assert(value == 1, panic(\"unreachable\"));",
    ] {
        let source = format!("module app; func main() {{ {body} }}");
        let compilation = compilation(&source);
        let key = source_callable_body_key(&compilation);

        let result = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        let mir = lowered_mir(&result);

        assert!(
            !mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::PanicReport(_))),
            "{body}: {mir:#?}"
        );
    }
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

    assert!(mir.blocks().iter().all(|block| !matches!(
        block.terminator().kind(),
        MirTerminatorKind::Return {
            value: None,
            report: None
        }
    )));
}

#[test]
fn contextual_constructions_in_yielded_values_retain_checked_selections() {
    for initializer in [
        "{ yield { value = 1 }; }",
        "if flag { yield { value = 1 }; } else { yield { value = 2 }; }",
        "{ let pair: (Item, Item) = { yield ({ value = 1 }, { value = 2 }); }; yield pair.0; }",
        "{ let mut value: Item = { value = 1 }; value = { value = 2 }; yield value; }",
        "{ let values: [Item; 2] = { yield [{ value = 1 }, { value = 2 }]; }; yield values[0]; }",
    ] {
        let source = format!(
            r#"
            module app;
            struct Item {{ value: i32; }}
            func make(pos flag: bool) -> Item {{
                let value: Item = {initializer};
                return value;
            }}
        "#
        );

        let compilation = compilation(&source);

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "make"))
            .expect("fixture callable must lower successfully");

        assert!(
            !lowered.diagnostics().has_errors(),
            "{source}: {lowered:#?}"
        );
    }
}

#[test]
fn generator_iteration_lowers_selected_cursor_operations() {
    let compilation = compilation(
        r#"
        module app;
        struct Item { value: i32; }
        func main() {
            let values = { each item in 0..3 { yield item; } };
            for value in values { assert(value >= 0); }
            let mut cursor = values(Iterable).iterate();
            let first = cursor(Iterator).next();
            let items = { each item in 0..3 { yield Item { value = item }; } };
            for item in move items { if item.value == 1 { break; } }
            let flags = { each item in 0..3 { yield item == 0; } };
            assert(any(flags));
        }
    "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("selected generator protocols must lower");

    assert!(lowered.diagnostics().is_empty(), "{lowered:#?}");

    let mir = lowered_mir(&lowered);

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Memory(memory) if matches!(memory.kind(), bray_bound_tree::CheckedMemoryOperationKind::RawBufferPop { .. }))));
}

#[test]
fn iteration_sources_preserve_termination_and_normal_completion() {
    let consumers = [
        "for value in produce() {}",
        "for value in move produce() {}",
        "_ = { each value in produce() { yield value; } };",
        "_ = { each value in move produce() { yield value; } };",
        "_ = all(produce());",
        "_ = any(produce());",
    ];

    for (ty, ready, statements) in [
        ("Items", "return Items {};", &consumers[..]),
        ("Range<i32>", "return 0..2;", &consumers[..4]),
    ] {
        for statement in statements {
            for production in [ready, "panic(\"source failure\");"] {
                for execution in ["", "async "] {
                    let source = format!(
                        r#"
                        module app;
                        struct Items {{}}
                        struct IterationCursor {{}}
                        impl &Items(Iterable) {{
                            type Element = bool;
                            type Cursor = IterationCursor;
                            consume func iterate() -> IterationCursor {{ return IterationCursor {{}}; }}
                        }}
                        impl Items(Iterable) {{
                            type Element = bool;
                            type Cursor = IterationCursor;
                            consume func iterate() -> IterationCursor {{ return IterationCursor {{}}; }}
                        }}
                        impl IterationCursor(Iterator) {{
                            type Element = bool;
                            mut func next() -> bool? {{ return none; }}
                        }}
                        func produce() -> {ty} {{ {production} }}
                        {execution}func main() {{ {statement} }}
                        "#
                    );

                    let compilation = compilation(&source);

                    assert!(
                        compilation.check_diagnostics().is_empty(),
                        "{source}: {:?}",
                        compilation.check_diagnostics()
                    );

                    let lowered = compilation
                        .lowered_unit(source_function_body_key(&compilation, "main"))
                        .expect("iteration preserves its source completion");

                    assert!(lowered.diagnostics().is_empty(), "{source}: {lowered:#?}");
                    assert!(lowered_mir(&lowered).is_valid(), "{source}: {lowered:#?}");
                }
            }
        }
    }
}

#[test]
fn temporary_generator_iteration_evaluates_its_producer() {
    for consume in [
        "_ = all({ each item in 0..9 { produced += 1; yield false; } });",
        "_ = any({ each item in 0..9 { produced += 1; yield true; } });",
        "for value in ({ each item in 0..9 { produced += 1; yield true; } }) {}",
        "for value in move ({ each item in 0..9 { produced += 1; yield true; } }) {}",
    ] {
        let source = format!(
            "module app; func main() {{ let mut produced: i32 = 0; {consume} assert(produced == 9); }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let result = compilation
            .lowered_unit(source_callable_body_key(&compilation))
            .expect("temporary generator consumer lowers");

        let mir = lowered_mir(&result);

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Memory(memory) if matches!(memory.kind(), bray_bound_tree::CheckedMemoryOperationKind::RawBufferPush { .. }))),
            "{consume} evaluates its producer before accessing the iterable: {mir:?}");
    }
}

#[test]
fn native_const_generator_folds_reuse_the_checked_storage_elision() {
    for (modifier, result, returned, elided) in [
        (
            "const ",
            "bool",
            "finished && !all(alias) && (any(alias) == flag)",
            true,
        ),
        (
            "const ",
            "i32",
            "if finished && !all(alias) && (any(alias) == flag) { yield 7; } else { yield 3; }",
            true,
        ),
        (
            "",
            "bool",
            "finished && !all(alias) && (any(alias) == flag)",
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            {modifier}func probe(pos flag: bool) -> {result} {{
                let mut finished = false;
                let values = {{ each item in 0..3 {{
                    yield false;
                    finished = item == 2;
                    yield flag;
                }} }};
                let alias = values;
                return {returned};
            }}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "probe"))
            .expect("checked generator observation lowers");

        let mir = lowered_mir(&lowered);
        let allocates = mir.operations().iter().any(|operation| matches!(operation.kind(), MirOperationKind::Memory(memory) if *memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawBufferAllocate));

        assert_eq!(!allocates, elided, "{source}: {mir:?}");

        if elided {
            let values = compilation
                .semantic_value_store()
                .expect("checked types publish");

            assert!(
                !mir.storages().iter().any(|storage| matches!(
                    values.type_data(storage.ty()).as_ref(),
                    bray_symbols::TypeData::Generator(_)
                )),
                "scalar observations do not retain a generator cleanup owner: {mir:?}"
            );
        }
    }
}

#[test]
fn native_generator_default_reuses_its_checked_observation() {
    for (yielded, requirement, elided) in [
        ("item < count", "requires(observed(count))", true),
        ("fail()", "", false),
    ] {
        let source = format!(
            r#"
            module app;
            func fail() -> bool {{ panic("default production"); }}
            const func observed(pos count: i32,
                complete: bool = all({{ each item in 0..count {{ yield {yielded}; }} }})) -> bool {{
                return complete;
            }}
            func demand(pos count: i32) {requirement} {{}}
            "#,
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = super::support::declared_unit_key(
            &compilation,
            bray_bound_tree::BoundUnitKind::RuntimeDefault,
        );

        let lowered = compilation
            .lowered_unit(key)
            .expect("checked generator default lowers");

        let mir = lowered_mir(&lowered);

        let allocates = mir.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Memory(memory) if *memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawBufferAllocate));

        assert_eq!(!allocates, elided, "{source}: {mir:?}");

        if elided {
            let values = compilation
                .semantic_value_store()
                .expect("checked types publish");

            assert!(!mir.storages().iter().any(|storage| matches!(
                values.type_data(storage.ty()).as_ref(),
                bray_symbols::TypeData::Generator(_)
            )));
        }
    }
}

#[test]
fn predicate_generator_runtime_checks_preserve_private_loop_state() {
    let checked = compilation(
        r#"
        module app;
        predicate generated(count: i32) = all({ each item in 0..count {
            yield if item == 0 { yield true; } else { yield item < count; };
        } });
        func demand(pos count: i32) requires(count >= 0, generated(count = count)) {}
        func main(pos count: i32) { demand(count); }
    "#,
    );

    assert!(
        !checked.check_diagnostics().has_errors(),
        "{:#?}",
        checked.check_diagnostics()
    );

    let lowered = checked
        .lowered_unit(source_function_body_key(&checked, "main"))
        .expect("generator predicate runtime check must lower");

    assert!(lowered.diagnostics().is_empty(), "{lowered:#?}");
    lowered_mir(&lowered);

    let key = checked
        .declared_unit_keys_for_test()
        .expect("predicate has a source unit")
        .into_iter()
        .find(|key| key.kind() == bray_bound_tree::BoundUnitKind::PredicateDefinition)
        .expect("fixture has its generator predicate");

    let template = checked
        .constant_body_template(key, &crate::CancellationToken::new())
        .expect("generator predicate has its checked template");

    let template = template
        .result()
        .value()
        .as_ref()
        .expect("predicate template is executable");

    assert!(
        template.nodes().iter().any(|node| matches!(
            node.operation(),
            bray_bound_tree::CheckedTemplateOperation::MachineBinary {
                operation: bray_symbols::ConstantBinaryOperation::Add,
                ..
            }
        )),
        "guarded cursor advancement retains its machine arithmetic"
    );

    assert!(
        !template.nodes().iter().any(|node| matches!(
            node.operation(),
            bray_bound_tree::CheckedTemplateOperation::Binary {
                operation: bray_symbols::ConstantBinaryOperation::Add,
                ..
            }
        )),
        "cursor state never stores a widened exact integer"
    );
}

#[test]
fn shared_generator_values_keep_their_original_reference_dependency() {
    let checked = compilation(
        r#"
        module app;
        func first(pos original: &i32) -> &i32 {
            let references = { each item in 0..3 { yield original; } };
            for reference in references { return reference; }
            return original;
        }
        func first_cursor(pos original: &i32) -> &i32 {
            let references = { each item in 0..3 { yield original; } };
            let mut cursor = (&references)(Iterable).iterate();
            if let ?reference = cursor(Iterator).next() { return reference; }
            return original;
        }
        func step<I>(pos mut cursor: I) -> I(Iterator).Element? with(I: Iterator) {
            return cursor(Iterator).next();
        }
        func first_generic_cursor(pos original: &i32) -> &i32 {
            let references = { each item in 0..3 { yield original; } };
            if let ?reference = step((&references)(Iterable).iterate()) { return reference; }
            return original;
        }
        func first_generic_move_cursor(pos original: &i32) -> &i32 {
            let references = { each item in 0..3 { yield original; } };
            if let ?reference = step(references(Iterable).iterate()) { return reference; }
            return original;
        }
    "#,
    );

    assert!(
        !checked.check_diagnostics().has_errors(),
        "{:#?}",
        checked.check_diagnostics()
    );
}

#[test]
fn generator_elements_cannot_escape_their_original_local_storage() {
    for mode in ["", "move "] {
        let source = format!(
            r#"
            module app;
            func first() -> &i32 {{
                let original: i32 = 42;
                let references = {{ each item in 0..3 {{ yield &original; }} }};
                for reference in {mode}references {{ return reference; }}
                panic("unreachable");
            }}
        "#
        );

        let checked = compilation(&source);

        assert!(
            checked
                .check_diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency),
            "{mode}: {:#?}",
            checked.check_diagnostics()
        );
    }

    for receiver in ["references", "(&references)"] {
        for next in ["cursor(Iterator).next()", "step(cursor)"] {
            let source = format!(
                r#"
                module app;
                func step<I>(pos mut cursor: I) -> I(Iterator).Element? with(I: Iterator) {{
                    return cursor(Iterator).next();
                }}
                func first() -> &i32 {{
                    let original: i32 = 42;
                    let references = {{ each item in 0..3 {{ yield &original; }} }};
                    let mut cursor = {receiver}(Iterable).iterate();
                    if let ?reference = {next} {{ return reference; }}
                    panic("unreachable");
                }}
            "#
            );

            let checked = compilation(&source);

            assert!(
                checked
                    .check_diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.kind()
                        == bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency),
                "{receiver} / {next}: {:#?}",
                checked.check_diagnostics()
            );
        }
    }
}
