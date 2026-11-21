use crate::test_support::compilation;
use bray_diagnostics::{DiagnosticArgValue, DiagnosticArrayGeneratorCardinalityProblem, DiagnosticKind};
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn fixed_array_generators_report_divergent_yield_cardinality() {
    assert_generator_problem(r#"
        module app;

        func main()
        {
            let generated: [i32; 2] = [each item in 0..2
            {
                yield item;
                yield item;
            }];
        }
    "#, |problem| matches!(problem, DiagnosticArrayGeneratorCardinalityProblem::YieldCountNotExact { .. }));
}

#[test]
fn fixed_array_generators_require_a_statically_known_source_count() {
    assert_generator_problem(r#"
        module app;

        func build(pos count: i32)
        {
            let generated: [i32; 2] = [each item in 0..count { yield item; }];
        }
    "#, |problem| matches!(problem, DiagnosticArrayGeneratorCardinalityProblem::SourceCountUnavailable { .. }));
}

#[test]
fn fixed_array_generators_use_literal_range_cardinality() {
    let matching = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let generated: [i32; 4] = [each item in 0..4\n",
        "    {\n",
        "        yield item;\n",
        "    }];\n",
        "}\n",
    ));

    assert!(
        matching.check_diagnostics().is_empty(),
        "{:#?}",
        matching.check_diagnostics()
    );

    let mismatched = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let generated: [i32; 3] = [each item in 0..4\n",
        "    {\n",
        "        yield item;\n",
        "    }];\n",
        "}\n",
    ));

    assert_goal_state_diagnostic_kind(
        mismatched.check_diagnostics(),
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
    );
}

#[track_caller]
fn assert_generator_problem(source: &str, expected: impl Fn(&DiagnosticArrayGeneratorCardinalityProblem) -> bool) {
    let compilation = compilation(source);
    let diagnostics = compilation.check_diagnostics();
    let errors = diagnostics.errors().collect::<Vec<_>>();

    let [error] = errors.as_slice() else { panic!("expected one generator error: {diagnostics:#?}"); };

    assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable);

    assert!(error.args().iter().any(|argument| matches!(argument.value(),
        DiagnosticArgValue::ArrayGeneratorCardinalityProblem(problem) if expected(problem))), "{error:#?}");

    let range = error.primary_span().expect("generator diagnostic attributes the expression").range();

    assert!(range.slice_str(source).is_some_and(|text| text.starts_with("[each")));
}
