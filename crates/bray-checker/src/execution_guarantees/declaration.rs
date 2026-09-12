use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::{
    ExecutesClauseSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    walk_syntax_node,
};

use super::{DeclaredExecutionProperty, ExecutionDeclaration, ExecutionDomain, ExecutionProperty};

/// Reads execution obligations and entry domains without certifying their implementations.
pub fn declared_execution_properties(
    declaration: SyntaxNodeView<'_>,
) -> DiagnosticResult<ExecutionDeclaration> {
    let mut result = ExecutionDeclaration::default();
    let mut diagnostics = DiagnosticBag::new();
    let mut domains = vec![ExecutionDomain::default()];
    let root = SyntaxAnchor::from_node(&declaration);

    walk_syntax_node(&declaration, |event| match event {
        SyntaxWalkEvent::EnterNode(node) => {
            let anchor = SyntaxAnchor::from_node(&node);

            if anchor == root {
                return SyntaxWalkControl::Continue;
            }

            let Some(domain) = domains.last_mut() else {
                return SyntaxWalkControl::Stop;
            };

            match node.kind() {
                SyntaxKind::RequiresClause if domain.guards.is_empty() => {
                    result.requirements.push(anchor)
                }
                SyntaxKind::EnsuresClause => domain.postconditions.push(anchor),
                SyntaxKind::ExecutesClause => {
                    result.clauses.push(anchor);
                    read_properties(node, &mut domain.properties, &mut diagnostics);
                }
                SyntaxKind::WhenClause => {
                    result.clauses.push(anchor);
                    let mut guards = domain.guards.to_vec();
                    guards.push(anchor);

                    domains.push(ExecutionDomain {
                        guards,
                        ..ExecutionDomain::default()
                    });

                    return SyntaxWalkControl::Continue;
                }
                _ => {}
            }

            SyntaxWalkControl::SkipChildren
        }
        SyntaxWalkEvent::ExitNode(node)
            if node.kind() == SyntaxKind::WhenClause || SyntaxAnchor::from_node(&node) == root =>
        {
            if let Some(domain) = domains.pop() {
                result.domains.push(domain);
            }

            SyntaxWalkControl::Continue
        }
        _ => SyntaxWalkControl::Continue,
    });

    DiagnosticResult::new(result, diagnostics)
}

fn read_properties(
    node: SyntaxNodeView<'_>,
    properties: &mut Vec<DeclaredExecutionProperty>,
    diagnostics: &mut DiagnosticBag,
) {
    let Some(clause) = node.cast::<ExecutesClauseSyntax>() else {
        return;
    };

    for syntax in clause.properties() {
        let token = syntax.identifier_token();
        let source = SourceSpan::new(node.source().source_id(), token.range());
        let name = token.text(node.source().text()).unwrap_or_default();

        match ExecutionProperty::from_name(name) {
            Some(property) => properties.push(DeclaredExecutionProperty { property, source }),
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
