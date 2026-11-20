use bray_diagnostics::{DiagnosticKind};
use crate::test_support::{source_function_body_key};
use super::fixtures::{assert_production_probes, production_host_compilation};

#[test]
#[ignore = "tier:proof"]
fn source_attachment_owner_requires_the_native_thread_protocol() {
    let missing = Some(DiagnosticKind::CheckingTrustedObligationNotProven);

    assert_production_probes(
        &[
            ("probe_attachment_cleanup", None),
            ("probe_attachment_finish", None),
            ("probe_attachment_mutated", missing),
            ("probe_attachment_copied", missing),
        ],
        &[],
    );
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
                r#"trusted module app;

using core.target;
 trusted func production_probe(pos {parameter}) -> {result}
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
