use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::{
    SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent, SyntaxWalkRoot, walk_syntax_node,
};

/// Rejects execution guarantee declarations until their promises can be verified.
///
/// Checks all callable forms within the supplied source declaration, including callable types
/// and anonymous callables. A guarded group produces one diagnostic for the whole group.
pub fn check_execution_guarantees(declaration: &impl SyntaxWalkRoot) -> DiagnosticBag {
    // TODO: Replace blanket rejection with execution guarantee verification under BRA-482.
    let mut diagnostics = DiagnosticBag::new();

    walk_syntax_node(declaration, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        let keyword = match node.kind() {
            SyntaxKind::ExecutesClause => SyntaxKind::ExecutesKeyword,
            SyntaxKind::WhenClause => SyntaxKind::WhenKeyword,
            _ => return SyntaxWalkControl::Continue,
        };

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
        EXECUTION_GUARANTEES_SOURCE, assert_goal_state_diagnostics, test_source_snapshot,
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

        let diagnostics = check_execution_guarantees(parsed.source_unit());

        assert_goal_state_diagnostics(&diagnostics);
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
