use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::{
    SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent, SyntaxWalkRoot, walk_syntax_node,
};

/// Rejects guarantee clauses outside the supported, checked callable declarations.
///
/// Checks all callable forms within the supplied source declaration, including callable types
/// and anonymous callables. A guarded group produces one diagnostic for the whole group.
pub fn check_execution_guarantees(
    declaration: &impl SyntaxWalkRoot,
    checked_clauses: &std::collections::BTreeSet<bray_declarations::SyntaxAnchor>,
) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::new();

    walk_syntax_node(declaration, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        let keyword = match node.kind() {
            // TODO(BRA-500): Validate guarantees on callable types and trait declarations.
            SyntaxKind::ExecutesClause => SyntaxKind::ExecutesKeyword,
            SyntaxKind::WhenClause => SyntaxKind::WhenKeyword,
            _ => return SyntaxWalkControl::Continue,
        };

        if checked_clauses.contains(&bray_declarations::SyntaxAnchor::from_node(&node)) {
            return SyntaxWalkControl::SkipChildren;
        }

        let span = SourceSpan::new(node.source().source_id(), node.full_range());

        diagnostics.add(
            Diagnostic::new(
                DiagnosticId::new(node.full_range().start().bytes()),
                DiagnosticKind::CheckingExecutionGuaranteeUnsupported,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::InvalidDeclaration,
                span,
            ))
            .with_arg(DiagnosticArg::actual_syntax_kind(keyword)),
        );

        SyntaxWalkControl::SkipChildren
    });

    diagnostics
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind};
    use bray_parser::parse_source_unit;
    use bray_syntax::SyntaxKind;
    use bray_testing::{
        EXECUTION_GUARANTEES_SOURCE, assert_goal_state_diagnostic_kind, test_source_snapshot,
    };

    use super::check_execution_guarantees;

    #[test]
    fn every_callable_form_rejects_unverified_guarantees() {
        let source = test_source_snapshot(EXECUTION_GUARANTEES_SOURCE);
        let parsed = parse_source_unit(&source);

        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );

        let diagnostics = check_execution_guarantees(parsed.source_unit(), &Default::default());

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::CheckingExecutionGuaranteeUnsupported,
        );

        assert_eq!(diagnostics.len(), 42);

        for (index, diagnostic) in diagnostics.iter().enumerate() {
            let keyword = if index % 2 == 0 {
                SyntaxKind::ExecutesKeyword
            } else {
                SyntaxKind::WhenKeyword
            };

            assert_eq!(
                diagnostic.kind(),
                DiagnosticKind::CheckingExecutionGuaranteeUnsupported
            );

            assert_eq!(
                diagnostic.args(),
                &[DiagnosticArg::actual_syntax_kind(keyword)]
            );

            assert!(diagnostic.primary_span().is_some());
        }
    }
}
