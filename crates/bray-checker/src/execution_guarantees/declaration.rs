use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::{
    ExecutesClauseSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    walk_syntax_node,
};

use super::{DeclaredExecutionProperty, ExecutionDeclaration, ExecutionProperty};

/// Reads unconditional promises owned directly by this declaration, without certifying them.
pub fn declared_execution_properties(
    declaration: SyntaxNodeView<'_>,
) -> DiagnosticResult<ExecutionDeclaration> {
    let mut result = ExecutionDeclaration::default();
    let mut diagnostics = DiagnosticBag::new();

    walk_syntax_node(&declaration, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        if node == declaration {
            return SyntaxWalkControl::Continue;
        }

        result.has_requirements |= node.kind() == SyntaxKind::RequiresClause;

        if node.kind() == SyntaxKind::ExecutesClause
            && let Some(clause) = node.cast::<ExecutesClauseSyntax>()
        {
            result
                .clauses
                .push(bray_declarations::SyntaxAnchor::from_node(&node));

            for syntax in clause.properties() {
                let token = syntax.identifier_token();
                let source = SourceSpan::new(node.source().source_id(), token.range());
                let name = token.text(node.source().text()).unwrap_or_default();

                match ExecutionProperty::from_name(name) {
                    Some(property) => result
                        .properties
                        .push(DeclaredExecutionProperty { property, source }),
                    None => diagnostics.add(
                        Diagnostic::new(
                            DiagnosticId::new(source.start().bytes()),
                            DiagnosticKind::CheckingUnknownExecutionProperty,
                            SeverityKind::Error,
                        )
                        .with_primary_span(source)
                        .with_label(DiagnosticLabel::primary(
                            DiagnosticLabelKind::InvalidDeclaration,
                            source,
                        ))
                        .with_arg(DiagnosticArg::referenced_name(name)),
                    ),
                }
            }
        }

        SyntaxWalkControl::SkipChildren
    });

    DiagnosticResult::new(result, diagnostics)
}
