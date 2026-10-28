use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticLabelKind, DiagnosticNoteKind};

use crate::test_support::{compilation, compilation_with_options};
use crate::{CompilationOptions, SemanticAnalysisLimits};

#[test]
fn growing_owned_cleanup_shapes_report_the_configured_limit() {
    for maximum in [32, SemanticAnalysisLimits::default().recursion_depth()] {
        for consume in ["", "discard<Node<i32>>(node);"] {
            let application = compilation_with_options(
                &format!(
                    r#"
                    module app;

                    struct Node<T>
                    {{
                        next: (box Node<(T,)>)?;
                    }}

                    func discard<T>(pos value: T) {{}}

                    func main()
                    {{
                        let node: Node<i32> =
                        {{
                            next = none
                        }};

                        {consume}
                    }}
                    "#,
                ),
                CompilationOptions::default()
                    .with_semantic_analysis_limits(SemanticAnalysisLimits::new(maximum, 100_000)),
            );

            let diagnostics = application.check_diagnostics();

            let limit = diagnostics
                .iter()
                .find(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::CheckingCleanupRecursionLimitExceeded
                })
                .unwrap_or_else(|| {
                    panic!("growing cleanup must report its limit: {diagnostics:?}")
                });

            assert_eq!(
                limit.args(),
                [
                    DiagnosticArg::declaration_name("main"),
                    DiagnosticArg::actual_count(
                        u64::try_from(maximum + 1).expect("test depth fits")
                    ),
                    DiagnosticArg::maximum_count(u64::try_from(maximum).expect("test limit fits")),
                ]
            );

            assert!(limit.primary_span().is_some());

            assert_eq!(
                limit.labels()[0].kind(),
                DiagnosticLabelKind::CleanupRecursionLimitExceeded
            );

            assert_eq!(
                limit.notes()[0].kind(),
                DiagnosticNoteKind::SimplifyRecursiveCleanup
            );
        }
    }
}

#[test]
fn growing_cleanup_shapes_follow_represented_owners() {
    for owner in ["Result<(box Node<(T,)>,), i32>", "Uninit<box Node<(T,)>>"] {
        let application = compilation(&format!(
            r#"
            module app;

            struct Node<T>
            {{
                next: {owner}?;
            }}

            func main()
            {{
                let node: Node<i32> =
                {{
                    next = none
                }};
            }}
            "#,
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingCleanupRecursionLimitExceeded,
        );
    }
}

#[test]
fn exact_recursive_cleanup_shapes_close_without_expanding_forever() {
    let application = compilation(
        r#"
        module app;

        struct Node<T>
        {
            next: (box Node<T>)?;
        }

        func discard<T>(pos value: T) {}

        func main()
        {
            let node: Node<i32> =
            {
                next = none
            };

            discard<Node<i32>>(node);
        }
        "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn recursive_cleanup_keeps_later_substitutions_obligations() {
    for (payload, contract, body, valid) in [
        ("Owner", "", "await pause();", false),
        ("Owner", " executes(pure, total)", "", true),
        ("Future<unit>", "", "await pause();", false),
    ] {
        let application = compilation(&format!(
            r#"
            module app;

            async func pause() {{}}

            struct Owner
            {{
                async finalize(){contract}
                {{
                    {body}
                }}
            }}

            struct Node<T, U>
            {{
                value: T;
                next: (box Node<U, U>)?;
            }}

            func discard<T>(pos value: T) {{}}

            async func main()
            {{
                let node: Node<i32, {payload}> =
                {{
                    value = 1,
                    next = none
                }};

                discard<Node<i32, {payload}>>(node);
            }}
            "#,
        ));

        let diagnostics = application.check_diagnostics();

        if valid {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}
