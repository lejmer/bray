use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind};
use bray_messages::DiagnosticRenderer;

/// Selects diagnostics with one exact kind while preserving their source order.
pub fn diagnostics_of_kind(diagnostics: &DiagnosticBag, kind: DiagnosticKind) -> DiagnosticBag {
    DiagnosticBag::from(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind() == kind)
            .cloned()
            .collect::<Vec<_>>(),
    )
}

/// Returns the diagnostic at one expected test position.
pub fn diagnostic_at(diagnostics: &DiagnosticBag, index: usize) -> &Diagnostic {
    diagnostics
        .iter()
        .nth(index)
        .unwrap_or_else(|| panic!("expected diagnostic at index {index}"))
}

/// Returns the only diagnostic in a test result.
pub fn single_diagnostic(diagnostics: &DiagnosticBag) -> &Diagnostic {
    assert_eq!(diagnostics.len(), 1, "expected one diagnostic");

    diagnostic_at(diagnostics, 0)
}

/// Asserts that an actual produced diagnostic satisfies its goal-state contract.
pub fn assert_goal_state_diagnostic(diagnostic: &Diagnostic) {
    let contract = diagnostic.kind().quality_contract();
    let issues = contract.unmet_requirements(diagnostic);

    assert!(
        issues.is_empty(),
        "{:?} does not satisfy its quality contract: {issues:?}\n{diagnostic:#?}",
        diagnostic.kind(),
    );

    let rendered = DiagnosticRenderer::english().render(diagnostic);

    assert!(
        !DiagnosticRenderer::english()
            .primary_message_contains_recovery_instruction(diagnostic.kind()),
        "{:?} puts recovery or next-action prose in its primary message; use a typed note or suggestion",
        diagnostic.kind(),
    );

    assert!(
        rendered.is_complete(),
        "{:?} rendered an incomplete user-facing component:\n{rendered:#?}",
        diagnostic.kind(),
    );

    let required = contract.required_rendered_args();
    let renderer = DiagnosticRenderer::english();
    let rendered_args = renderer.component_argument_names(diagnostic);

    assert!(
        required.iter().all(|name| rendered_args.contains(name)),
        "{:?} does not render required context in any user-facing component: required {required:?}, rendered {rendered_args:?}",
        diagnostic.kind(),
    );

    assert_component_args(
        diagnostic,
        "primary message",
        contract.primary_message_args(),
        &renderer.diagnostic_argument_names(diagnostic.kind()),
    );

    for note in diagnostic.notes() {
        if let Some(required) = contract.note_args(note.kind()) {
            assert_component_args(
                diagnostic,
                "note",
                required,
                &renderer.note_argument_names(note.kind()),
            );
        }
    }

    for location in diagnostic.related_locations() {
        if let Some(required) = contract.related_location_args(location.kind()) {
            assert_component_args(
                diagnostic,
                "related location",
                required,
                &renderer.related_location_argument_names(location.kind()),
            );
        }
    }

    for suggestion in diagnostic.suggestions() {
        if let Some(required) = contract.suggestion_args(suggestion.kind()) {
            assert_component_args(
                diagnostic,
                "suggestion",
                required,
                &renderer.suggestion_argument_names(suggestion.kind()),
            );
        }
    }
}

fn assert_component_args(
    diagnostic: &Diagnostic,
    component: &str,
    required: &[bray_diagnostics::DiagnosticArgName],
    rendered: &[bray_diagnostics::DiagnosticArgName],
) {
    assert!(
        required.iter().all(|name| rendered.contains(name)),
        "{:?} does not render required context in its {component}: required {required:?}, rendered {rendered:?}",
        diagnostic.kind(),
    );
}

/// Asserts that every actual produced diagnostic in a bag satisfies its contract.
pub fn assert_goal_state_diagnostics(diagnostics: &DiagnosticBag) {
    for diagnostic in diagnostics.iter() {
        assert_goal_state_diagnostic(diagnostic);
    }
}

/// Asserts that a producer bag contains one exact diagnostic kind and that every matching
/// diagnostic satisfies its goal-state contract.
pub fn assert_goal_state_diagnostic_kind(diagnostics: &DiagnosticBag, kind: DiagnosticKind) {
    let matching = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind() == kind)
        .collect::<Vec<_>>();

    assert!(
        !matching.is_empty(),
        "producer bag does not contain the expected {kind:?} diagnostic: {diagnostics:#?}",
    );

    for diagnostic in matching {
        assert_goal_state_diagnostic(diagnostic);
    }
}
