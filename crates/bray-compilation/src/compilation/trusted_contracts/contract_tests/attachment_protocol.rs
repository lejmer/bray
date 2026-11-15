use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn source_attachment_owner_requires_the_native_thread_protocol() {
    for (body, valid) in [
        ("let owner = trusted acquire();", true),
        (
            "let owner = trusted acquire(); _ = trusted finish(owner);",
            true,
        ),
        (
            "let mut owner = trusted acquire(); owner.identity = 1;",
            false,
        ),
        (
            "let owner = trusted acquire(); let copied: Owner = { status = owner.status, identity = owner.identity };",
            false,
        ),
    ] {
        let source = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.host.attachment;
            func production_probe()
                requires(trusted native_acquisition())
            {{
                {body}
            }}
        "#
        );

        let compilation = production_host_compilation(&source);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{body}: {checked:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_attachment_growth_only_transfers_prepared_backing_under_the_lease() {
    let compilation = production_host_compilation("module app;");
    let key = source_function_body_key(&compilation, "insert_attachment");

    let checked = compilation
        .lowered_unit(key.clone())
        .expect("production attachment growth must lower");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let checked = compilation
        .execution_properties(key)
        .expect("production attachment growth must certify its execution guarantee");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    assert!(
        checked
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total)
    );
}

#[test]
#[ignore = "tier:proof"]
fn bootstrap_module_dependencies_do_not_form_export_cycles() {
    let compilation = crate::test_support::compilation(
        "internal module root; using internal root.host;\n\
         internal module root.host { using internal root; using internal root.thread.destruction; }\n\
         internal module root.thread.destruction { using internal root.host; }",
    );

    let diagnostics = compilation.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
#[ignore = "tier:proof"]
fn opaque_attachment_ordinals_complete_without_becoming_pure_observations() {
    for (parameter, result, body) in [
        (
            "value: usize",
            "DataPointer<u8>",
            "trusted core.target.from_exposed_address<u8>(address = value)",
        ),
        (
            "value: DataPointer<u8>",
            "usize",
            "trusted core.target.expose_address<u8>(value)",
        ),
    ] {
        for (property, expected) in [("total", true), ("pure", false)] {
            let source = format!(
                r#"module app;

using core.target;
 func production_probe(pos {parameter}) -> {result}
     executes({property})
     uses(intrinsic)
                {{
                    return {body};
                }}"#
            );

            let compilation = crate::test_support::compilation(&source);

            let checked = compilation
                .execution_properties(source_function_body_key(&compilation, "production_probe"))
                .expect("selected ordinal conversion must publish its execution result");

            assert_eq!(
                !checked.diagnostics().has_errors(),
                expected,
                "{source}: {checked:#?}"
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_attachment_scan_keeps_its_returned_mutable_borrow_live() {
    let compilation = production_host_compilation("module app;");
    let key = source_function_body_key(&compilation, "find_attachment");

    let checked = compilation
        .lowered_unit(key.clone())
        .expect("production record scan must lower");

    assert!(!checked.diagnostics().has_errors(), "{checked:#?}");

    let properties = compilation
        .execution_properties(key)
        .expect("production record scan must certify its total guarantee");

    assert!(
        properties
            .value()
            .properties
            .contains(&bray_symbols::ExecutionProperty::Total),
        "{properties:#?}"
    );

    assert!(!properties.diagnostics().has_errors(), "{properties:#?}");
}
