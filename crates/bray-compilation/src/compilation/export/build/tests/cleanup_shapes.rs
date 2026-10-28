use bray_diagnostics::DiagnosticKind;

use super::fixtures::{compilation, execution_consumer};

#[test]
fn imported_growing_cleanup_shapes_report_the_analysis_limit() {
    let provider = compilation(
        r#"
        module api;

        struct Node<T>
        {
            next: Result<(box Node<(T,)>,), i32>?;
        }

        func discard<T>(pos value: T) {}
        "#,
    );

    for consume in [
        "",
        "example.package.api.discard<example.package.api.Node<i32>>(node);",
    ] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                module app;

                using example.package.api;

                func main()
                {{
                    let node: example.package.api.Node<i32> =
                    {{
                        next = none
                    }};

                    {consume}
                }}
                "#,
            ),
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            consumer.check_diagnostics(),
            DiagnosticKind::CheckingCleanupRecursionLimitExceeded,
        );
    }
}

#[test]
fn imported_recursive_cleanup_keeps_later_substitutions_obligations() {
    let provider = compilation(
        r#"
        module api;

        struct Node<T, U>
        {
            value: T;
            next: (box Node<U, U>)?;
        }

        func discard<T>(pos value: T) {}
        "#,
    );

    for payload in ["i32", "Owner", "Future<unit>"] {
        let consumer = execution_consumer(
            &provider,
            &format!(
                r#"
                module app;

                using example.package.api;

                async func pause() {{}}

                struct Owner
                {{
                    async finalize()
                    {{
                        await pause();
                    }}
                }}

                async func main()
                {{
                    let node: example.package.api.Node<i32, {payload}> =
                    {{
                        value = 1,
                        next = none
                    }};

                    example.package.api.discard<example.package.api.Node<i32, {payload}>>(node);
                }}
                "#,
            ),
        );

        let diagnostics = consumer.check_diagnostics();

        if payload == "i32" {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                diagnostics,
                DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
            );
        }
    }
}

#[test]
fn growing_raw_buffer_cleanup_shapes_report_the_analysis_limit() {
    let source = r#"
        trusted module app;

        using core.memory;
        using std.memory;

        struct Node<T>
        {
            children: std.memory.RawBuffer<Node<(T,)>>?;
        }

        func main()
        {
            let node: Node<i32> =
            {
                children = none
            };
        }
    "#;

    let application = super::fixtures::standard_library_compilation([
        include_str!("../../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../../../standard-library/std/src/memory/function.bray"),
        source,
    ]);

    bray_testing::assert_goal_state_diagnostic_kind(
        application.check_diagnostics(),
        DiagnosticKind::CheckingCleanupRecursionLimitExceeded,
    );
}
