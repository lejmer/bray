use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn repeated_arrays_keep_nested_constructor_element_context() {
    for count in [0, 2, 1288] {
        for head in ["", "Buffer"] {
            let source = format!(r#"
                module app;

                struct Buffer
                {{
                    values: [u32; {count}];
                }}

                struct Outer
                {{
                    buffer: Buffer;
                }}

                func make() -> Outer
                {{
                    return
                    {{
                        buffer = {head}
                        {{
                            values = [0; {count}],
                        }},
                    }};
                }}
            "#);

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "count={count}, head={head}: {diagnostics:#?}");
        }
    }
}

#[test]
fn repeated_array_constructor_context_does_not_override_the_checked_extent() {
    let compilation = compilation(r#"
        module app;

        struct Buffer
        {
            values: [u32; 2];
        }

        func make() -> Buffer
        {
            return
            {
                values = [0; 3],
            };
        }
    "#);

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingIncompatibleExpressionType,
    );
}

#[test]
fn repeated_arrays_require_copyable_elements_at_every_extent() {
    for count in [0, 1, 3] {
        let source = format!(
            r#"
            module app;

            struct Owner
            {{
                value: i32;
            }}

            func main()
            {{
                let items: [Owner; {count}] = [
                    Owner
                    {{
                        value = 7,
                    }}; {count}
                ];
            }}
            "#
        );

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingRepeatedArrayElementNotCopyable,
        );

        let result = compilation
            .lowered_unit(source_function_body_key(&compilation, "main"))
            .expect("rejected repetition must return source diagnostics");

        assert!(result.diagnostics().iter().any(|diagnostic| {
            diagnostic.kind() == DiagnosticKind::CheckingRepeatedArrayElementNotCopyable
        }));
    }
}

#[test]
fn repeated_arrays_reject_nested_ownership_and_mutable_borrows() {
    for ty in ["box i32", "&mut i32", "(box i32,)", "(box i32)?"] {
        let source = format!(
            r#"
            module app;

            func repeat(pos value: {ty}) -> [{ty}; 2]
            {{
                return [value; 2];
            }}
            "#
        );

        let compilation = compilation(&source);

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingRepeatedArrayElementNotCopyable,
        );
    }
}

#[test]
fn repeated_arrays_preserve_copyable_sources_and_borrow_dependencies() {
    let compilation = compilation(
        r#"
        module app;

        @copy
        struct Point
        {
            value: i32;
        }

        const COUNT: usize = 3;

        func repeat<T, const N: usize>(pos value: T) -> [T; N]
            with(T: Copyable)
        {
            return [value; N];
        }

        func main()
        {
            let point: Point = Point
            {
                value = 5,
            };

            let empty: [Point; 0] = [point; 0];
            let one: [Point; 1] = [point; 1];
            let many: [Point; COUNT] = repeat<Point, COUNT>(point);
            let borrowed: [&Point; 2] = [&point; 2];

            assert(point.value == 5);
        }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn repeated_arrays_do_not_infer_a_generic_copy_contract() {
    let compilation = compilation(
        r#"
        module app;

        func repeat<T>(pos value: T) -> [T; 2]
        {
            return [value; 2];
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingRepeatedArrayElementNotCopyable,
    );
}

#[test]
fn repeated_arrays_of_shared_borrows_prevent_overlapping_mutation() {
    let compilation = compilation(
        r#"
        module app;

        func observe(pos value: &i32)
        {
        }

        func main()
        {
            let mut value: i32 = 1;
            let copies: [&i32; 2] = [&value; 2];

            value = 2;
            observe(copies[0]);
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingConflictingBorrow,
    );
}

#[test]
fn repeated_arrays_reject_noncopyable_constant_elements_before_evaluation() {
    let compilation = compilation(
        r#"
        module app;

        struct Owner
        {
            value: i32;
        }

        const VALUES: [Owner; 2] = [
            Owner
            {
                value = 7,
            }; 2
        ];
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingRepeatedArrayElementNotCopyable,
    );
}

#[test]
fn repeated_arrays_check_and_infer_constant_counts_in_their_lexical_context() {
    let compilation = compilation(r#"
        module app;

        const COUNT: usize = 3;

        const func count() -> usize
        {
            return COUNT;
        }

        func generic<const N: usize>(pos value: i32) -> [i32; N]
        {
            return [value; N];
        }

        func arithmetic<const N: usize>() -> [i32; N + 1]
        {
            const LOCAL: usize = N + 1;

            return [7; LOCAL];
        }

        func main()
        {
            const LOCAL: usize = COUNT;
            const NEXT: usize = LOCAL + 1;
            let literal = [7; 3];
            let nested: [[i32; 3]; 2] = [[7; COUNT]; 2];
            let arithmetic_values: [i32; 4] = arithmetic<3>();
            let named = [7; COUNT];
            let called: [i32; 3] = [7; count()];
            let local: [i32; 4] = [7; NEXT];
            let inferred_local = [7; NEXT];
            let generic_values: [i32; 3] = generic<3>(7);

            {
                const INNER_COUNT: usize = 1;
                let nested_local: [i32; 1] = [7; INNER_COUNT];
            }

            let outer: [i32; 3] = [7; LOCAL];
        }
    "#);

    assert!(compilation.check_diagnostics().is_empty(), "{:#?}", compilation.check_diagnostics());

    let lowered = compilation.lowered_unit(source_function_body_key(&compilation, "main"))
        .expect("valid repeated arrays lower");

    assert!(lowered.diagnostics().is_empty(), "{:#?}", lowered.diagnostics());
}

#[test]
fn repeated_arrays_reject_counts_that_disagree_with_the_destination() {
    for count in ["3", "COUNT", "count()"] {
        let source = format!(r#"
            module app;

            const COUNT: usize = 3;

            const func count() -> usize
            {{
                return COUNT;
            }}

            func main()
            {{
                let values: [i32; 2] = [7; {count}];
            }}
        "#);

        let compilation = compilation(&source);

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(), DiagnosticKind::CheckingIncompatibleExpressionType,
        );
    }
}

#[test]
fn repeated_arrays_reject_runtime_counts_despite_a_fixed_destination() {
    for source in [
        r#"
        module app;

        func repeat(pos count: usize)
        {
            let values: [i32; 3] = [7; count];
        }
        "#,
        r#"
        module app;

        func main()
        {
            let count: usize = 3;
            let values: [i32; 3] = [7; count];
        }
        "#,
    ] {
        let compilation = compilation(source);

        assert_goal_state_diagnostic_kind(compilation.check_diagnostics(), DiagnosticKind::CheckingInvalidConstantExpression);
    }
}

#[test]
fn repeated_arrays_initialize_checked_constant_templates() {
    let compilation = compilation(r#"
        module app;

        const COUNT: usize = 3;

        struct Buffer<const N: usize>
        {
            const VALUES: [i32; N] = [7; N];
        }

        const func fill<const N: usize>() -> [i32; N]
        {
            return [7; N];
        }

        const EMPTY: [i32; 0] = [7; 0];

        const ONE: [i32; 1] = [7; 1];

        const MANY: [i32; COUNT] = [7; COUNT];

        const GENERIC: [i32; COUNT] = fill<COUNT>();

        const FIRST: i32 = GENERIC[0];

        func main()
        {
            assert(FIRST == 7);
        }
    "#);

    assert!(compilation.check_diagnostics().is_empty(), "{:#?}", compilation.check_diagnostics());
}

#[test]
fn repeated_arrays_reject_invalid_count_types_and_representations() {
    for count in ["true", "-1", "18446744073709551616"] {
        let compilation = compilation(&format!(r#"
            module app;

            func main()
            {{
                let values: [i32; 3] = [7; {count}];
            }}
        "#));

        assert!(compilation.check_diagnostics().has_errors(), "invalid count {count} must be rejected");
    }
}

#[test]
fn repeated_arrays_preserve_recovery_for_incomplete_counts() {
    for expression in ["[7;]", "[; 3]", "[7; unknown]"] {
        let compilation = compilation(&format!(r#"
            module app;

            func main()
            {{
                let values: [i32; 3] = {expression};
            }}
        "#));

        assert!(compilation.check_diagnostics().has_errors(), "{expression} must retain its source error");
    }
}

#[test]
fn repeated_arrays_do_not_execute_count_calls_in_the_runtime_contract() {
    let compilation = compilation(
        r#"
        module app;

        const func count() -> usize
        {
            let mut value: usize = 0;

            while value < 3
            {
                value += 1;
            }

            return value;
        }

        func repeat() -> [i32; 3]
            executes(pure, total)
        {
            return [7; count()];
        }
        "#,
    );

    assert!(compilation.check_diagnostics().is_empty(), "{:#?}", compilation.check_diagnostics());
}

#[test]
fn repeated_arrays_reject_counts_that_return_from_the_enclosing_function() {
    let compilation = compilation(
        r#"
        module app;

        func main() -> usize
        {
            let values: [i32; 3] = [
                7;
                {
                    return 3;
                }
            ];

            return 0;
        }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(), DiagnosticKind::CheckingInvalidConstantExpression,
    );
}

#[test]
fn repeated_arrays_check_inline_pattern_counts() {
    for count in [
        r#"
        match true
        {
            case true { yield 3; }
            case false { yield 1; }
        }
        "#,
        r#"
        if true matches true
        {
            yield 3;
        }
        else
        {
            yield 1;
        }
        "#,
        r#"
        if let true = true
        {
            yield 3;
        }
        else
        {
            yield 1;
        }
        "#,
        r#"
        match true
        {
            case FLAG { yield 3; }
            case _ { yield 1; }
        }
        "#,
        r#"
        match true
        {
            case LOCAL_FLAG { yield 3; }
            case _ { yield 1; }
        }
        "#,
        r#"
        match true
        {
            case true when true matches true { yield 3; }
            case _ { yield 1; }
        }
        "#,
    ] {
        let source = format!(
            r#"
            module app;

            const FLAG: bool = true;

            func main()
            {{
                const LOCAL_FLAG: bool = true;
                let values: [i32; 3] = [7; {count}];
            }}
            "#,
        );

        let compilation = compilation(&source);

        assert!(compilation.check_diagnostics().is_empty(), "{count}: {:#?}", compilation.check_diagnostics());

        let lowered = compilation.lowered_unit(source_function_body_key(&compilation, "main"))
            .expect("pattern count retains its checked unit");

        assert!(lowered.diagnostics().is_empty(), "{count}: {:#?}", lowered.diagnostics());
    }
}

#[test]
fn repeated_arrays_check_patterns_in_local_constant_dependencies() {
    let compilation = compilation(
        r#"
        module app;

        const FLAG: bool = true;

        func main()
        {
            const COUNT: usize = match true
            {
                case FLAG { yield 3; }
                case _ { yield 1; }
            };

            const SIZE: usize = COUNT + 1;

            let values: [i32; 4] = [7; SIZE];
            let selected: [i32; 3] =
            [
                7;
                match true
                {
                    case true when true matches true { yield COUNT; }
                    case _ { yield 1; }
                }
            ];
        }
        "#,
    );

    assert!(compilation.check_diagnostics().is_empty(), "{:#?}", compilation.check_diagnostics());
}

#[test]
fn repeated_arrays_check_patterns_in_named_counts_and_type_extents() {
    let compilation = compilation(
        r#"
        module app;

        const COUNT: usize = match true
        {
            case true { yield 3; }
            case false { yield 1; }
        };

        const VALUES: [i32; COUNT] = [7; COUNT];

        func main()
        {
            let named: [i32; 3] = [7; COUNT];
            let typed:
            [
                i32;
                match true
                {
                    case true { yield 3; }
                    case false { yield 1; }
                }
            ] = [7; 3];

            assert(VALUES[2] == 7);
        }
        "#,
    );

    assert!(compilation.check_diagnostics().is_empty(), "{:#?}", compilation.check_diagnostics());
}
