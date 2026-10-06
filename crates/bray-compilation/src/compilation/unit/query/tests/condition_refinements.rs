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
            "module app; func main(pos value: i32?) {{ if {condition} {{ value; }} else {{ value; }} }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let analysis = compilation
            .refinements(source_callable_body_key(&compilation))
            .unwrap();

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
            "module app; func main(pos value: (i32?, i32?)) {{ if {condition} {{ 11; }} else {{ 22; }} 33; }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_callable_body_key(&compilation);
        let unit = compilation.bound_unit(key.clone()).unwrap();
        let storage = compilation.storage_plan(key.clone()).unwrap();
        let analysis = compilation.refinements(key).unwrap();

        for (marker, expected) in [("11;", present), ("22;", absent), ("33;", None)] {
            let offset = u32::try_from(source.find(marker).unwrap()).unwrap();

            let node = unit.value().tree().expressions().find_map(|(id, expression)| {
                    matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
                }).unwrap();

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
                            .unwrap()
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
            "if !(value matches none || { value = none; yield false; }) { 11; }",
            None,
        ),
        (
            "if !(value matches ?_ && { value = none; yield true; }) {} else { 11; }",
            None,
        ),
        (
            "if value matches ?_ && ({ value = none; yield true; }) { 11; }",
            None,
        ),
        (
            "assert(!(value matches none || { value = none; yield false; })); 11;",
            None,
        ),
        (
            "match true { case true when !(value matches none || { value = none; yield false; }) { 11; } case _ {} }",
            None,
        ),
        (
            "while !(value matches none || { value = none; yield false; }) { 11; break; }",
            None,
        ),
        (
            "let result = (value matches none || { value = none; yield false; }) || { 11; yield false; };",
            None,
        ),
        (
            "if !(value matches none || { other = none; yield false; }) { 11; }",
            Some(PatternPredicate::NullablePresent),
        ),
        (
            "if !(value matches none || false) { 11; }",
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
        let unit = compilation.bound_unit(key.clone()).unwrap();
        let analysis = compilation.refinements(key).unwrap();
        let offset = u32::try_from(source.find("11;").unwrap()).unwrap();

        let marker = unit.value().tree().expressions().find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
            }).unwrap();

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
fn opaque_calls_invalidate_storage_refinements_while_pure_calls_preserve_them() {
    for (call, expected) in [
        ("touch();", None),
        ("stable();", Some(PatternPredicate::NullablePresent)),
    ] {
        let source = format!(
            r#"
                module app;

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
                        11;
                    }}
                }}
                "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");

        let key = source_function_body_key(&compilation, "main");
        let unit = compilation.bound_unit(key.clone()).unwrap();
        let analysis = compilation.refinements(key).unwrap();
        let offset = u32::try_from(source.find("11;").unwrap()).unwrap();

        let marker = unit.value().tree().expressions().find_map(|(id, expression)| {
                matches!(expression, BoundExpression::Literal(literal) if literal.spelling_range().start().bytes() == offset).then_some(id)
            }).unwrap();

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
