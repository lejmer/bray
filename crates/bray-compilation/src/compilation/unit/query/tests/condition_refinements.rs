use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};
use bray_bound_tree::{BoundExpression, PatternPredicate, RefinementKind, StorageProjection};
use bray_symbols::SymbolOrdinal;

#[test]
fn pattern_conditions_publish_both_nullable_outcomes_and_common_alternative_refinements() {
    for condition in [
        "value matches none",
        "!(value matches none)",
        "value matches ?0 | ?1",
    ] {
        let source = format!(
            "module app; func main(pos value: i32?) {{ if {condition} {{ _ = value; }} else {{ _ = value; }} }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let analysis = compilation
            .refinements(source_callable_body_key(&compilation))
            .expect("fixture callable must have checked refinements");

        let present = analysis.value().occurrences().iter().any(|occurrence| {
            occurrence.refinements().iter().any(|refinement| {
                matches!(
                    refinement.kind(),
                    RefinementKind::Pattern {
                        predicate: PatternPredicate::NullablePresent,
                        value: true,
                        ..
                    }
                )
            })
        });

        assert!(present, "{condition}: {analysis:?}");

        if condition != "value matches ?0 | ?1" {
            assert!(
                analysis.value().occurrences().iter().any(|occurrence| {
                    occurrence.refinements().iter().any(|refinement| {
                        matches!(
                            refinement.kind(),
                            RefinementKind::Pattern {
                                predicate: PatternPredicate::NullableAbsent,
                                value: true,
                                ..
                            }
                        )
                    })
                }),
                "{condition}: {analysis:?}"
            );
        }
    }
}

#[test]
fn pattern_conditions_refine_exact_paths_and_preserve_only_common_outcomes() {
    use PatternPredicate::{NullableAbsent, NullablePresent};

    for (condition, present, absent) in [
        (
            "value matches (?_, _)",
            Some(NullablePresent),
            Some(NullableAbsent),
        ),
        ("value matches (?0, _)", Some(NullablePresent), None),
        (
            "value matches (?0, _) | (?1, _)",
            Some(NullablePresent),
            None,
        ),
        (
            "value matches (?0, _) || value matches (?1, _)",
            Some(NullablePresent),
            None,
        ),
        (
            "!(value matches (?_, _))",
            Some(NullableAbsent),
            Some(NullablePresent),
        ),
    ] {
        let source = format!(
            "module app; func main(pos value: (i32?, i32?)) {{ if {condition} {{ _ = 11; }} else {{ _ = 22; }} _ = 33; }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_callable_body_key(&compilation);

        let unit = compilation
            .bound_unit(key.clone())
            .expect("fixture callable must bind successfully");

        let storage = compilation
            .storage_plan(key.clone())
            .expect("fixture callable must have a storage plan");

        let analysis = compilation
            .refinements(key)
            .expect("fixture callable must have checked refinements");

        for (marker, expected) in [("11;", present), ("22;", absent), ("33;", None)] {
            let offset = u32::try_from(
                source
                    .find(marker)
                    .expect("fixture lookup must find its expected value"),
            )
            .expect("fixture value must fit the requested representation");

            let node = unit.value().tree().expressions().find_map(|(id, expression)| {
                    matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
                }).expect("fixture selection must find its expected value");

            let actual = analysis
                .value()
                .refinements_before(node.into())
                .iter()
                .filter_map(|refinement| match refinement.kind() {
                    RefinementKind::Pattern {
                        predicate: predicate @ (NullablePresent | NullableAbsent),
                        access,
                        value: true,
                        ..
                    } => Some((
                        predicate,
                        storage
                            .value()
                            .resolved_projections(access)
                            .expect("fixture `resolved_projections` call must succeed")
                            .to_vec(),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();

            let expected = expected
                .into_iter()
                .map(|predicate| {
                    (
                        predicate,
                        vec![StorageProjection::TupleElement(SymbolOrdinal::new(0))],
                    )
                })
                .collect::<Vec<_>>();

            assert_eq!(actual, expected, "{condition} at {marker}");
        }
    }
}

#[test]
fn boolean_condition_refinements_respect_later_operand_mutation() {
    for (body, expected) in [
        (
            "if !(value matches none || { value = none; yield false; }) { _ = 11; }",
            None,
        ),
        (
            "if !(value matches ?_ && { value = none; yield true; }) {} else { _ = 11; }",
            None,
        ),
        (
            "if value matches ?_ && ({ value = none; yield true; }) { _ = 11; }",
            None,
        ),
        (
            "assert(!(value matches none || { value = none; yield false; })); _ = 11;",
            None,
        ),
        (
            "match true { case true when !(value matches none || { value = none; yield false; }) { _ = 11; } case _ {} }",
            None,
        ),
        (
            "while !(value matches none || { value = none; yield false; }) { _ = 11; break; }",
            None,
        ),
        (
            "let result = (value matches none || { value = none; yield false; }) || { _ = 11; yield false; };",
            None,
        ),
        (
            "if !(value matches none || { other = none; yield false; }) { _ = 11; }",
            Some(PatternPredicate::NullablePresent),
        ),
        (
            "if !(value matches none || false) { _ = 11; }",
            Some(PatternPredicate::NullablePresent),
        ),
    ] {
        let source =
            format!("module app; func main(pos mut value: i32?, pos mut other: i32?) {{ {body} }}");

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_callable_body_key(&compilation);

        let unit = compilation
            .bound_unit(key.clone())
            .expect("fixture callable must bind successfully");

        let analysis = compilation
            .refinements(key)
            .expect("fixture callable must have checked refinements");

        let offset = u32::try_from(
            source
                .find("11;")
                .expect("fixture lookup must find its expected value"),
        )
        .expect("fixture value must fit the requested representation");

        let marker = unit.value().tree().expressions().find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
            }).expect("fixture selection must find its expected value");

        let actual = analysis
            .value()
            .refinements_before(marker.into())
            .iter()
            .filter_map(|refinement| match refinement.kind() {
                RefinementKind::Pattern {
                    predicate:
                        predicate @ (PatternPredicate::NullablePresent
                        | PatternPredicate::NullableAbsent),
                    value: true,
                    ..
                } => Some(predicate),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, expected.into_iter().collect::<Vec<_>>(), "{body}");
    }
}

#[test]
fn calls_preserve_unexposed_scalar_refinements() {
    for (call, expected) in [
        ("touch();", Some(PatternPredicate::NullablePresent)),
        ("opaque();", Some(PatternPredicate::NullablePresent)),
        ("stable();", Some(PatternPredicate::NullablePresent)),
    ] {
        let source = format!(
            r#"
                module app;

                extern func opaque();

                func touch()
                {{
                }}

                func stable() executes(pure, total)
                {{
                }}

                func main(pos value: i32?)
                {{
                    if value matches ?_
                    {{
                        {call}
                        _ = 11;
                    }}
                }}
                "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");

        let key = source_function_body_key(&compilation, "main");

        let unit = compilation
            .bound_unit(key.clone())
            .expect("fixture callable must bind successfully");

        let analysis = compilation
            .refinements(key)
            .expect("fixture callable must have checked refinements");

        let offset = u32::try_from(
            source
                .find("11;")
                .expect("fixture lookup must find its expected value"),
        )
        .expect("fixture value must fit the requested representation");

        let marker = unit.value().tree().expressions().find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
            }).expect("fixture selection must find its expected value");

        let actual = analysis
            .value()
            .refinements_before(marker.into())
            .iter()
            .filter_map(|refinement| match refinement.kind() {
                RefinementKind::Pattern {
                    predicate: PatternPredicate::NullablePresent,
                    value: true,
                    ..
                } => Some(PatternPredicate::NullablePresent),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(actual, expected.into_iter().collect::<Vec<_>>(), "{call}");
    }
}

#[test]
fn copied_pattern_payload_fields_preserve_refinements_between_call_arguments() {
    let compilation = compilation(
        r#"module app;
func produce() -> Result<(i64, u32), unit> {
    let seconds: i64 = 1;
    let nanos: u32 = 2;
    return Ok((seconds, nanos));
}
func combine(seconds: i64, nanos: u32) -> i64 { return seconds + nanos as i64; }
func main() -> i64 {
    return match produce() {
        case Ok(parts) { yield combine(seconds = parts.0, nanos = parts.1); }
        case Error(_) { yield 0; }
    };
}
"#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn shared_nullable_discriminants_survive_independent_mutation() {
    for read in ["selected.value", "selected.callback(owner.frame)"] {
        let source = format!(
            r#"
            module app;

            struct Owner
            {{
                mut frame: usize;
            }}

            struct Service
            {{
                value: usize;
                callback: func(pos frame: usize) -> usize;
            }}

            func run(pos owner: &mut Owner, pos service: &(Service?)) -> usize
            {{
                if let ?selected = service
                {{
                    owner.frame = 0;

                    return {read};
                }}

                return 0;
            }}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{read}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn mutable_payload_loans_preserve_only_their_parent_discriminants() {
    for (declaration, input, subject, pattern) in [
        ("", "&mut(Service?)", "service", "?selected"),
        (
            "struct Holder { mut service: Service?; }",
            "&mut Holder",
            "service.service",
            "?selected",
        ),
        (
            "union Choice { Use(pos mut service: Service); Other; }",
            "&mut Choice",
            "service",
            "Use(selected)",
        ),
    ] {
        let source = format!(
            r#"
            module app;

            struct Cell
            {{
                mut value: usize;
            }}

            struct Owner
            {{
                mut frame: Cell;
            }}

            struct Service
            {{
                mut value: Cell;
                mut other: Cell;
            }}

            {declaration}

            func swap(pos left: &mut Cell, pos right: &mut Cell)
            {{
                let previous: usize = left.value;

                left.value = right.value;
                right.value = previous;
            }}

            func run(pos owner: &mut Owner, pos service: {input}) -> usize
            {{
                if let {pattern} = {subject}
                {{
                    swap(&mut owner.frame, &mut selected.value);
                    swap(&mut owner.frame, &mut selected.other);

                    return selected.value.value;
                }}

                return 0;
            }}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{input}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn replacing_a_nullable_owner_invalidates_its_presence_condition() {
    let source = r#"
        module app;

        struct Service
        {
            value: usize;
        }

        struct Holder
        {
            mut service: Service?;
        }

        func clear(pos holder: &mut Holder)
        {
            holder.service = none;
        }

        func run(pos holder: &mut Holder)
        {
            if holder.service matches ?_
            {
                clear(holder);

                _ = 11;
            }
        }
    "#;

    let compilation = compilation(source);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "run");

    let unit = compilation
        .bound_unit(key.clone())
        .expect("fixture callable must bind successfully");

    let analysis = compilation
        .refinements(key)
        .expect("fixture callable must have checked refinements");

    let offset = u32::try_from(
        source
            .find("11;")
            .expect("fixture lookup must find its expected value"),
    )
    .expect("fixture value must fit the requested representation");

    let marker = unit.value().tree().expressions().find_map(|(id, expression)| {
        matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
    }).expect("fixture selection must find its expected value");

    assert!(
        !analysis
            .value()
            .refinements_before(marker.into())
            .iter()
            .any(|refinement| matches!(
                refinement.kind(),
                RefinementKind::Pattern {
                    predicate: PatternPredicate::NullablePresent,
                    value: true,
                    ..
                }
            )),
        "{analysis:#?}"
    );
}

#[test]
fn replacing_a_borrowed_nullable_invalidates_total_call_evidence() {
    let source = r#"
        module app;

        struct Service
        {
            value: usize;
        }

        struct Holder
        {
            mut service: Service?;
        }

        func clear(pos holder: &mut Holder)
            executes(total)
        {
            holder.service = none;
        }

        func read(pos service: &(Service?)) -> usize
            requires(service matches ?_)
            executes(pure, total)
        {
            return match service
            {
                case ?selected { yield selected.value; }
                case none { yield 0; }
            };
        }

        func run(pos holder: &mut Holder) -> usize
            executes(total)
        {
            if holder.service matches ?_
            {
                clear(holder);

                return read(&holder.service);
            }

            return 0;
        }
    "#;

    let compilation = compilation(source);

    assert!(
        compilation
            .check_diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind()
                == bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn compound_completion_refinements_do_not_prune_effectful_entry_paths() {
    for ty in ["usize?", "&(usize?)", "&mut(usize?)"] {
        for body in [
            "if value matches ?_ { panic(\"unreachable\"); }",
            "if value matches none { panic(\"unreachable\"); }",
            "match value { case ?_ { panic(\"unreachable\"); } case none {} }",
            "while value matches ?_ { panic(\"unreachable\"); }",
        ] {
            let source =
                format!("module app; func run(pos value: {ty}) executes(total) {{ {body} }}");

            let compilation = compilation(&source);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }

    let compilation = compilation(
        r#"
        module app;
        struct Counter { mut value: usize; }
        func run(pos value: usize?, pos counter: &mut Counter) executes(pure)
        {
            if value matches ?_
            {
                counter.value = 1;
                return;
            }
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}

#[test]
fn inexact_tag_free_stores_preserve_unrelated_nullable_discriminants() {
    for assignment in [
        "counters[index].value = 1;",
        "counters[index].value += 1;",
        "counters[index] = { value = 1 };",
    ] {
        let guarantee = if assignment.contains("+=") {
            ""
        } else {
            "executes(total)"
        };

        let source = format!(
            r#"
            module app;
            struct Counter {{ mut value: usize; }}
            struct Service {{ value: usize; }}

            func read(pos service: &mut(Service?)) -> usize
                requires(service matches ?_)
                executes(pure, total)
            {{
                return match service
                {{
                    case ?selected {{ yield selected.value; }}
                    case none {{ yield 0; }}
                }};
            }}

            func run(pos counters: &mut [Counter; 2], pos service: &mut(Service?), pos index: usize) -> usize
                {guarantee}
            {{
                if index < 2
                {{
                    if let ?selected = service
                    {{
                        {assignment}
                        _ = selected.value;

                        return read(service);
                    }}
                }}

                return 0;
            }}
        "#
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{assignment}: {:#?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn inexact_nullable_replacement_invalidates_possibly_aliased_presence() {
    let compilation = compilation(
        r#"
        module app;
        struct Service { value: usize; }
        func read(pos service: &(Service?)) -> usize
            requires(service matches ?_)
            executes(pure, total)
        {
            return match service
            {
                case ?selected { yield selected.value; }
                case none { yield 0; }
            };
        }

        func run(pos services: &mut [Service?; 2], pos index: usize) -> usize executes(total)
        {
            if index < 2 && services[0] matches ?_
            {
                services[index] = none;

                return read(&services[0]);
            }

            return 0;
        }
    "#,
    );

    bray_testing::assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    );
}
