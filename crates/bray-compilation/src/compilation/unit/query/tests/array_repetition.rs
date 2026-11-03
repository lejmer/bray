use crate::test_support::{compilation, source_function_body_key};
use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

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
