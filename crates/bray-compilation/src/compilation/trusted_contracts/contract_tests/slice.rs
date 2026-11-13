use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{compilation};

#[test]
fn slice_descriptor_preservation_does_not_preserve_element_authority() {
    for (operation, valid) in [
        ("", true),
        ("if slice.length() > 0 { slice[0] = 1; }", false),
    ] {
        let source = format!(
            r#"
            trusted module app;
            trusted predicate marked(slice: &[u8]);
            trusted func establish(pos slice: &mut [u8])
                ensures(trusted marked(slice = &slice))
            {{
            }}
            func observe(pos slice: &[u8])
                requires(trusted marked(slice = &slice))
            {{
            }}
            func caller(pos slice: &mut [u8])
            {{
                establish(slice);
                {operation}
                observe(&slice);
            }}
        "#
        );

        let application = compilation(&source);

        assert_eq!(
            !application.check_diagnostics().has_errors(),
            valid,
            "{source}: {:?}",
            application.check_diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                application.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn slice_bounds_follow_descriptor_identity_through_element_writes_and_replacement() {
    for (body, valid) in [
        (
            "let count = slice.length(); let mut index: usize = 0; while index < count { slice[index] = 1; _ = slice[index]; index += 1; }",
            true,
        ),
        (
            "let count = slice.length(); let selected: &mut [u8] = if replace { yield &mut slice[..]; } else { yield &mut other[..]; }; if count > 0 { selected[0] = 1; }",
            false,
        ),
    ] {
        let source = format!(
            r#"
            module app;
            func caller(pos slice: &mut [u8], pos other: &mut [u8], pos replace: bool)
                    executes(total)
            {{
                {body}
            }}
        "#
        );

        let application = compilation(&source);
        let key = crate::test_support::source_function_body_key(&application, "caller");

        let checked = application
            .execution_properties(key)
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn projected_slices_keep_their_descriptor_in_downstream_contracts() {
    for (input, borrow, change, observed, valid) in [
        ("&[u8]", "&", "", "selected", true),
        ("&[u8; 4]", "&", "", "selected", true),
        ("&mut [u8]", "&mut ", "if count > 0 { selected[0] = 1; }", "&selected", true),
        ("&[u8]", "&", "", "other", false),
    ] {
        let source = format!(r#"
            trusted module app;

            trusted predicate copied(count: usize, extent: usize);

            trusted func observe(pos values: &[u8], pos count: usize)
                requires(count <= values.length())
                ensures(trusted copied(count, values.length()))
                executes(pure, total)
            {{
            }}

            func caller(pos values: {input}, pos other: &[u8], pos offset: usize, pos length: usize) -> u32
                ensures(result != 0 || trusted copied(length, length))
            {{
                if offset > values.length() || length > values.length() - offset
                {{
                    return 3;
                }}

                let selected = {borrow}values[offset..offset + length];
                let count: usize = selected.length();

                if count != length
                {{
                    return 3;
                }}

                {change}
                observe({observed}, count);
                return 0;
            }}
        "#);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert_eq!(!diagnostics.has_errors(), valid, "{source}: {diagnostics:#?}");

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingTrustedObligationNotProven);
        }
    }
}

#[test]
fn slice_contents_calls_are_not_reused_after_element_mutation() {
    for (operation, valid) in [("", true), ("slice[0] = 1;", false)] {
        let source = format!(
            r#"
            module app;
            const func first(pos slice: &[u8]) -> u8
                executes(pure, total)
            {{
                if slice.length() > 0
                {{
                    return slice[0];
                }}
                return 0;
            }}
            func observe(pos slice: &[u8])
                requires(first(slice) == 0)
                executes(total)
            {{
            }}
            func caller(pos slice: &mut [u8])
                executes(total)
            {{
                if slice.length() > 0 && first(&slice) == 0
                {{
                    {operation}
                    observe(&slice);
                }}
            }}
        "#
        );

        let application = compilation(&source);
        let key = crate::test_support::source_function_body_key(&application, "caller");

        let checked = application
            .execution_properties(key)
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}

#[test]
fn nested_slice_bounds_require_descriptor_storage_separation() {
    for (parameter, prepare, valid) in [
        ("incoming: Envelope", "let mut holder = incoming;", true),
        (
            "outer: Outer",
            "let middle = outer.inner; let mut holder = middle;",
            true,
        ),
        ("holder: &mut Envelope", "", false),
    ] {
        // Owned local descriptors cannot be reached by incoming references. Two distinct
        // incoming reference paths do not establish that separation on their own.
        let source = format!(
            r#"
            module app;
            struct Envelope
            {{
                values: &mut [u8];
                unrelated: usize;
            }}
            struct Outer
            {{
                inner: Envelope;
            }}
            func caller(pos {parameter})
                executes(total)
            {{
                {prepare}
                let count = holder.values.length();
                let mut index: usize = 0;
                while index < count
                {{
                    holder.values[index] = 1;
                    _ = holder.values[index];
                    index += 1;
                }}
            }}
        "#
        );

        let application = compilation(&source);
        let key = crate::test_support::source_function_body_key(&application, "caller");

        let checked = application
            .execution_properties(key)
            .expect("fixture callable must resolve its execution properties");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{source}: {checked:?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            );
        }
    }
}
