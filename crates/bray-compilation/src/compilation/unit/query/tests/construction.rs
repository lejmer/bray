use super::support::selection::selection_rejections;
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{ConstructionTarget, SelectedOperation, SemanticSelection};
use bray_diagnostics::{
    DiagnosticConstructionInputRejection, DiagnosticKind, DiagnosticSelectionRejectionReason,
};
use bray_messages::DiagnosticRenderer;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn construction_selections_retain_struct_and_union_targets() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "}\n",
        "union Maybe\n",
        "{\n",
        "    Some(value: i32);\n",
        "    None;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let point = Point { x = 1 };\n",
        "    let present: Maybe = Some(value = 1);\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("construction selections must publish: {error:?}"),
    };

    let targets = selections.value().entries().iter().filter_map(|entry| {
        let SemanticSelection::Operation(SelectedOperation::Construction(construction)) =
            entry.selection()
        else {
            return None;
        };

        Some(construction.target())
    });

    assert_eq!(
        targets
            .filter(|target| matches!(target, ConstructionTarget::Struct(_)))
            .count(),
        1
    );

    assert_eq!(
        selections
            .value()
            .entries()
            .iter()
            .filter(|entry| {
                matches!(
                    entry.selection(),
                    SemanticSelection::Operation(SelectedOperation::Construction(
                        construction
                    )) if matches!(
                        construction.target(),
                        ConstructionTarget::UnionVariant(_)
                    )
                )
            })
            .count(),
        1
    );

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );
}

#[test]
fn invalid_source_construction_reports_incompatible_candidate() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let point = Point { y = 1 };\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("invalid construction selections must recover: {error:?}"),
    };

    assert_eq!(
        selections
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingIncompatibleCandidate)
            .count(),
        1,
        "{:?}",
        selections.diagnostics()
    );

    assert_goal_state_diagnostic_kind(
        selections.diagnostics(),
        DiagnosticKind::CheckingIncompatibleCandidate,
    );

    let diagnostic = selections
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingIncompatibleCandidate)
        .next()
        .unwrap_or_else(|| panic!("incompatible construction diagnostic must exist"));

    let rejections = selection_rejections(diagnostic);

    assert_eq!(rejections.rejections().len(), 1);
    assert_eq!(rejections.omitted_count(), 0);

    assert!(matches!(
        rejections.rejections()[0].reason(),
        DiagnosticSelectionRejectionReason::ConstructionInput(
            DiagnosticConstructionInputRejection::UnknownName { provided, accepted }
        ) if provided == "y" && accepted.as_ref() == [String::from("x")]
    ));
}

#[test]
fn operation_selection_reports_invalid_conversions() {
    let compilation = compilation(
        r#"module app;

struct Value
{
}

func convert(pos value: Value) -> i32
{
    return value as i32;
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let semantics = compilation
        .expression_types(key)
        .unwrap_or_else(|error| panic!("invalid conversion must remain checkable: {error:?}"));

    assert!(semantics.diagnostics().has_errors());
}

#[test]
fn narrowing_integer_conversions_reach_the_user_as_conversion_diagnostics() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func narrow(pos value: u64) -> u8\n",
        "{\n",
        "    return value as u8;\n",
        "}\n",
    ));

    let diagnostic = compilation
        .check_diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingNoApplicableCandidate)
        .unwrap_or_else(|| panic!("narrowing conversion diagnostic must be published"));

    assert_eq!(
        DiagnosticRenderer::english().render(diagnostic).message(),
        "no applicable conversion candidate"
    );

    assert!(diagnostic.primary_span().is_some());
}

#[test]
fn box_construction_selection_retains_the_storage_implementation() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let stored: box i32 = box(1);\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("box construction selection must publish: {error:?}"),
    };

    assert!(selections.value().entries().iter().any(|entry| {
        matches!(
            entry.selection(),
            SemanticSelection::Operation(SelectedOperation::Construction(construction))
                if matches!(construction.target(), ConstructionTarget::TypeForm { .. })
        )
    }));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );
}

#[test]
fn construction_inputs_retain_identity_and_one_nullable_insertion() {
    let application = compilation(
        r#"
        module app;

        struct Holder { value: i32?; }
        struct Nested { value: (i32?)?; }
        union Choice { Some(value: i32?); None; }

        func main(pos value: i32, pos optional: i32?)
        {
            let direct = Holder { value = value };
            let identity = Holder { value = optional };
            let nested = Nested { value = optional };
            let variant: Choice = Some(value = value);
        }
        "#,
    );

    assert!(
        application.check_diagnostics().is_empty(),
        "{:?}",
        application.check_diagnostics()
    );

    let selections = application
        .semantic_selections(source_callable_body_key(&application))
        .expect("fixture callable must have semantic selections");

    let conversions = selections
        .value()
        .entries()
        .iter()
        .filter_map(|entry| {
            let SemanticSelection::Operation(SelectedOperation::Construction(construction)) =
                entry.selection()
            else {
                return None;
            };

            Some(
                construction
                    .inputs()
                    .iter()
                    .filter_map(|input| match input {
                        bray_bound_tree::SelectedConstructionInput::Explicit {
                            conversion, ..
                        } => Some(conversion.target()),
                        _ => None,
                    }),
            )
        })
        .flatten()
        .collect::<Vec<_>>();

    assert_eq!(
        conversions,
        vec![
            &bray_bound_tree::ConversionTarget::NullablePresent,
            &bray_bound_tree::ConversionTarget::Identity,
            &bray_bound_tree::ConversionTarget::NullablePresent,
            &bray_bound_tree::ConversionTarget::NullablePresent,
        ]
    );
}

#[test]
fn construction_inputs_reject_nullable_extraction_and_scalar_coercion() {
    for (source, target) in [("i32?", "i32"), ("u32", "i32?"), ("i32", "i64")] {
        let source = format!(
            r#"
            module app;
            struct Holder {{ value: {target}; }}
            func main(pos value: {source}) {{ let holder = Holder {{ value = value }}; }}
        "#
        );

        let application = compilation(&source);

        assert_goal_state_diagnostic_kind(
            application.check_diagnostics(),
            DiagnosticKind::CheckingIncompatibleCandidate,
        );
    }
}
