use bray_bound_tree::{BoundBlockItem, BoundUnit, BoundUnitKey, CheckedExpressionSemantics};
use bray_checker::CheckerRequestContext;
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticResult, DiagnosticSourceEdit,
    DiagnosticSuggestion, DiagnosticSuggestionApplicability, DiagnosticSuggestionKind,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_syntax::{
    BlockExpressionSyntax, BlockItemSyntax, ExpressionSyntax, SourceSyntaxNode, SyntaxKind,
    syntax_node_view,
};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn finish_expression_diagnostics(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        result: DiagnosticResult<CheckedExpressionSemantics>,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<CheckedExpressionSemantics>, FactQueryError> {
        let (semantics, mut diagnostics) = result.into_parts();

        let context = self.checker_context_for(key, cancellation)?;

        for (_, block) in bound.tree().blocks() {
            cancellation.check()?;

            let anchor = block.origin().source_anchor().syntax();

            if anchor.syntax_kind() != SyntaxKind::BlockExpression || anchor.is_recovered() {
                continue;
            }

            let mut ignored = block
                .items()
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let BoundBlockItem::Expression(expression) = item else {
                        return None;
                    };

                    let result = semantics
                        .types()
                        .expression(*expression)
                        .expect("block expression has its final checked type");

                    (!result.is_recovered()
                        && !matches!(
                            context
                                .available_compiler_known_symbols()
                                .type_representation(context.semantic_values(), result.ty()),
                            Some(RepresentationRole::Unit | RepresentationRole::Never)
                        ))
                    .then_some((index, result.ty()))
                })
                .peekable();

            if ignored.peek().is_none() {
                continue;
            }

            let syntax = anchor
                .find_descendant::<BlockExpressionSyntax>(self.syntax_tree())
                .expect("source block retains its syntax in the compilation snapshot");

            for (index, syntax) in syntax.block_items().enumerate() {
                let Some(&(expected, ty)) = ignored.peek() else {
                    break;
                };

                if index != expected {
                    continue;
                }

                ignored.next();

                let Some(expression) = syntax.expression() else {
                    continue;
                };

                let span =
                    SourceSpan::new(expression.source().source_id(), expression.full_range());

                let actual = bray_checker::diagnostic_type(&context, ty)?;

                diagnostics.add(
                    Diagnostic::new(
                        DiagnosticId::new(span.start().bytes()),
                        DiagnosticKind::CheckingIgnoredExpressionResult,
                        SeverityKind::Warning,
                    )
                    .with_primary_span(span)
                    .with_label(DiagnosticLabel::primary(
                        DiagnosticLabelKind::IgnoredExpressionResult,
                        span,
                    ))
                    .with_arg(DiagnosticArg::actual_type(actual))
                    .with_note(DiagnosticNote::new(
                        DiagnosticNoteKind::ExplicitDiscardAcknowledgesResult,
                    ))
                    .with_suggestion(discard_suggestion(&syntax, &expression)),
                );
            }
        }

        Ok(DiagnosticResult::new(semantics, diagnostics))
    }
}

fn discard_suggestion(
    item: &BlockItemSyntax,
    expression: &ExpressionSyntax,
) -> DiagnosticSuggestion {
    let source = expression.source().source_id();
    let range = expression.full_range();
    let start = syntax_node_view(expression)
        .tokens()
        .next()
        .expect("a valid source expression has its first token")
        .range()
        .start();

    let mut edits = vec![DiagnosticSourceEdit::new(
        SourceSpan::empty(source, start),
        "_ = ",
    )];

    if item.sequenced_expression().is_none() {
        edits.push(DiagnosticSourceEdit::new(
            SourceSpan::empty(source, range.end()),
            ";",
        ));
    }

    DiagnosticSuggestion::try_edits(
        DiagnosticSuggestionKind::ExplicitlyDiscardResult,
        DiagnosticSuggestionApplicability::MaybeApplicable,
        edits,
    )
    .expect(
        "a discard correction inserts a prefix and an optional terminator at distinct boundaries",
    )
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, diagnostic_kinds};
    use bray_diagnostics::DiagnosticKind;
    use bray_messages::DiagnosticRenderer;

    #[test]
    fn ignored_results_publish_typed_warnings_and_applicable_discard_edits() {
        let text = r#"
            module app;
            func value() -> i32 { return 7; }
            func main()
            {
                value();
                _ = value();
                let _ = value();
                let _: u8 = 7;
                unit;
            }
        "#;

        let compilation = compilation(text);
        let diagnostics = compilation.check_diagnostics();

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingIgnoredExpressionResult,
        );

        assert_eq!(
            diagnostic_kinds(diagnostics),
            [DiagnosticKind::CheckingIgnoredExpressionResult]
        );

        let diagnostic = diagnostics
            .by_kind(DiagnosticKind::CheckingIgnoredExpressionResult)
            .next()
            .unwrap();

        let rendered = DiagnosticRenderer::english().render(diagnostic);

        assert_eq!(
            diagnostic.severity(),
            bray_diagnostics::SeverityKind::Warning
        );

        assert!(rendered.message().contains("i32"), "{rendered:?}");
        assert!(rendered.message().contains("is ignored"));
        assert_eq!(diagnostic.suggestions().len(), 1);

        let suggestion = &diagnostic.suggestions()[0];

        assert_eq!(
            suggestion.applicability(),
            bray_diagnostics::DiagnosticSuggestionApplicability::MaybeApplicable
        );

        assert_eq!(suggestion.edits().len(), 1);

        let edit = &suggestion.edits()[0];
        let mut fixed = text.to_owned();

        fixed.insert_str(
            usize::try_from(edit.span().start().bytes()).unwrap(),
            edit.replacement(),
        );

        assert!(fixed.contains("\n                _ = value();"));

        let fixed_compilation = crate::test_support::compilation(&fixed);

        assert!(
            fixed_compilation.check_diagnostics().is_empty(),
            "{:#?}",
            fixed_compilation.check_diagnostics()
        );
    }

    #[test]
    fn discard_edits_preserve_indentation_and_leading_comments() {
        for (body, corrected) in [
            ("    value();", "    _ = value();"),
            (
                "    // acknowledgement\n    value();",
                "    // acknowledgement\n    _ = value();",
            ),
            (
                "    /* acknowledgement */ value();",
                "    /* acknowledgement */ _ = value();",
            ),
        ] {
            let prefix = "module app; func value() -> i32 { return 7; } func main() {\n";
            let text = format!("{prefix}{body}\n}}");
            let compilation = compilation(&text);
            let diagnostic = compilation
                .check_diagnostics()
                .by_kind(DiagnosticKind::CheckingIgnoredExpressionResult)
                .next()
                .unwrap();

            let edit = &diagnostic.suggestions()[0].edits()[0];
            let mut fixed = text.clone();

            fixed.insert_str(
                usize::try_from(edit.span().start().bytes()).unwrap(),
                edit.replacement(),
            );

            assert_eq!(fixed, format!("{prefix}{corrected}\n}}"));
        }
    }

    #[test]
    fn discard_fixes_cover_whole_expressions_and_required_terminators() {
        for statement in [
            "1;",
            "(1 + 2);",
            "value() + value();",
            "value() /* acknowledgement */;",
            "{ yield 1; } // acknowledgement\n",
            "{ yield 1; } /* acknowledgement */",
            "trusted value();",
            "{ yield 1; }",
            "{ yield 1; };",
            "if true { yield 1; } else { yield 2; }",
            "loop { break 1; }",
        ] {
            let text = format!(
                r#"
                module app;
                func value() -> i32 {{ return 7; }}
                func main()
                {{
                    {statement}
                }}
            "#
            );

            let compilation = compilation(&text);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{statement}: {diagnostics:#?}");

            let warnings = diagnostics
                .by_kind(DiagnosticKind::CheckingIgnoredExpressionResult)
                .collect::<Vec<_>>();

            assert_eq!(warnings.len(), 1, "{statement}: {diagnostics:#?}");

            let mut fixed = text.clone();

            for edit in warnings[0].suggestions()[0].edits().iter().rev() {
                let range = usize::try_from(edit.span().start().bytes()).unwrap()
                    ..usize::try_from(edit.span().end().bytes()).unwrap();

                fixed.replace_range(range, edit.replacement());
            }

            let fixed_compilation = crate::test_support::compilation(&fixed);

            assert!(
                fixed_compilation.check_diagnostics().is_empty(),
                "{fixed}: {:#?}",
                fixed_compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn unit_and_never_statements_do_not_warn_about_results() {
        let compilation = compilation(
            r#"
            module app;
            func done() {}
            func main()
            {
                done();
                unit;
                return;
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }
}
