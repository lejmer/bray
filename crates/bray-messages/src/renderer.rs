use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticLabel, DiagnosticLabelStyle,
    DiagnosticNote, SeverityKind,
};
use bray_source::{SourceLocation, SourceSpan};

use crate::argument::{ArgumentFormatter, format_source_location, format_source_span};
use crate::catalog::{MessageCatalog, MessageTemplate, MessageTemplatePart};
use crate::locale::DiagnosticLocale;
use crate::rendered_diagnostic::{
    RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote, RenderedDiagnosticNoteKind,
};

/// Locale-aware renderer for structured diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticRenderer {
    locale: DiagnosticLocale,
}

impl DiagnosticRenderer {
    /// Creates a diagnostic renderer for `locale`.
    pub const fn new(locale: DiagnosticLocale) -> Self {
        Self { locale }
    }

    /// Creates a diagnostic renderer for English output.
    pub const fn english() -> Self {
        Self::new(DiagnosticLocale::English)
    }

    /// Returns the renderer locale.
    pub const fn locale(self) -> DiagnosticLocale {
        self.locale
    }

    /// Renders one structured diagnostic.
    pub fn render(self, diagnostic: &Diagnostic) -> RenderedDiagnostic {
        let catalog = MessageCatalog::new(self.locale);
        let template = catalog.diagnostic_template(diagnostic.kind());

        let labels = diagnostic
            .labels()
            .iter()
            .map(|label| self.render_label(label))
            .collect();

        let notes = diagnostic
            .notes()
            .iter()
            .map(|note| self.render_note(note))
            .collect();

        RenderedDiagnostic::new(
            diagnostic.id(),
            diagnostic.kind(),
            diagnostic.severity(),
            diagnostic.primary_span(),
            self.render_template(template, diagnostic.args()),
            labels,
            notes,
        )
    }

    /// Renders all diagnostics in insertion order.
    pub fn render_bag(self, bag: &DiagnosticBag) -> Vec<RenderedDiagnostic> {
        bag.iter()
            .map(|diagnostic| self.render(diagnostic))
            .collect()
    }

    /// Renders a source span for terminal diagnostic output.
    pub fn render_source_span(self, span: SourceSpan) -> String {
        format_source_span(self.locale, span)
    }

    /// Renders a resolved source location for terminal diagnostic output.
    pub fn render_source_location(self, location: SourceLocation<'_>) -> String {
        format_source_location(self.locale, location)
    }

    /// Renders a severity heading for terminal diagnostic output.
    pub fn render_severity(self, severity: SeverityKind) -> &'static str {
        MessageCatalog::new(self.locale).severity_label(severity)
    }

    /// Renders a label style for terminal diagnostic output.
    pub fn render_label_style(self, style: DiagnosticLabelStyle) -> &'static str {
        MessageCatalog::new(self.locale).label_style(style)
    }

    /// Renders a note heading for terminal diagnostic output.
    pub fn render_note_heading(self, kind: RenderedDiagnosticNoteKind) -> &'static str {
        MessageCatalog::new(self.locale).note_heading(kind)
    }

    fn render_label(self, label: &DiagnosticLabel) -> RenderedDiagnosticLabel {
        let template = MessageCatalog::new(self.locale).label_template(label.kind());

        RenderedDiagnosticLabel::new(
            label.kind(),
            label.style(),
            label.span(),
            self.render_template(template, label.args()),
        )
    }

    fn render_note(self, note: &DiagnosticNote) -> RenderedDiagnosticNote {
        let catalog = MessageCatalog::new(self.locale);
        let template = catalog.note_template(note.kind());

        RenderedDiagnosticNote::new(
            note.kind(),
            catalog.note_kind(note.kind()),
            self.render_template(template, note.args()),
        )
    }

    fn render_template(self, template: MessageTemplate, args: &[DiagnosticArg]) -> String {
        let formatter = ArgumentFormatter::new(self.locale);

        let mut message = String::new();

        for part in template.parts() {
            match part {
                MessageTemplatePart::Text(text) => message.push_str(text),
                MessageTemplatePart::Arg(name) => {
                    message.push_str(&formatter.format_named_arg(args, *name));
                }
            }
        }

        message
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticAlignmentKind, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue,
        DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind,
        DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticLabel,
        DiagnosticLabelKind, DiagnosticLabelStyle, DiagnosticModuleTrust, DiagnosticNameKind,
        DiagnosticNote, DiagnosticNoteKind, DiagnosticOutputSink, DiagnosticSelectionKind,
        DiagnosticVisibility, SeverityKind,
    };
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_syntax::SyntaxKind;

    use super::DiagnosticRenderer;
    use crate::{
        DiagnosticLocale, RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNoteKind,
    };

    #[test]
    fn renderer_renders_diagnostic_messages_from_structured_catalog() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(7),
            DiagnosticKind::SourceFileReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("main.bray"))
        .with_arg(DiagnosticArg::io_error_kind(
            DiagnosticIoErrorKind::NotFound,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::SourceFileMustBeReadable,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.id(), DiagnosticId::new(7));
        assert_eq!(rendered.kind(), DiagnosticKind::SourceFileReadFailed);
        assert_eq!(rendered.severity(), SeverityKind::Error);

        assert_eq!(
            rendered.message(),
            "could not read source file main.bray: not found"
        );

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.kind(), DiagnosticNoteKind::SourceFileMustBeReadable);
        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);

        assert_eq!(
            note.message(),
            "source files must be readable before compilation"
        );
    }

    #[test]
    fn renderer_localizes_checker_diagnostics() {
        let incompatible = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingIncompatibleExpressionType,
            SeverityKind::Error,
        )
        .with_arg(bray_diagnostics::DiagnosticArg::expected_type(
            bray_diagnostics::DiagnosticType::Boolean,
        ))
        .with_arg(bray_diagnostics::DiagnosticArg::actual_type(
            bray_diagnostics::DiagnosticType::Tuple(2),
        ));

        let unresolved = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingCannotInferExpressionType,
            SeverityKind::Error,
        );

        let array_length = Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::CheckingArrayLengthNotPositive,
            SeverityKind::Error,
        );

        let generator_cardinality = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
            SeverityKind::Error,
        );

        let ambiguous = Diagnostic::new(
            DiagnosticId::new(4),
            DiagnosticKind::CheckingAmbiguousCandidate,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::selection_kind(
            DiagnosticSelectionKind::Operator,
        ));

        let target_alignment = Diagnostic::new(
            DiagnosticId::new(5),
            DiagnosticKind::CheckingTargetAlignmentUnsupported,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::alignment_kind(
            DiagnosticAlignmentKind::Storage,
        ))
        .with_arg(DiagnosticArg::required_alignment(64))
        .with_arg(DiagnosticArg::maximum_alignment(16));

        let incompatible_pattern = Diagnostic::new(
            DiagnosticId::new(6),
            DiagnosticKind::CheckingIncompatiblePattern,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_type(
            bray_diagnostics::DiagnosticType::Boolean,
        ));

        let non_exhaustive_match = Diagnostic::new(
            DiagnosticId::new(7),
            DiagnosticKind::CheckingNonExhaustiveMatch,
            SeverityKind::Error,
        );

        let renderer = DiagnosticRenderer::english();

        assert_eq!(
            renderer.render(&incompatible).message(),
            "expected bool, but found tuple type with 2 elements"
        );

        assert_eq!(
            renderer.render(&unresolved).message(),
            "cannot infer expression type"
        );

        assert_eq!(
            renderer.render(&array_length).message(),
            "array length must be greater than zero"
        );

        assert_eq!(
            renderer.render(&generator_cardinality).message(),
            "array generator element count cannot be proven"
        );

        assert_eq!(
            renderer.render(&ambiguous).message(),
            "operator selection is ambiguous"
        );

        assert_eq!(
            renderer.render(&target_alignment).message(),
            "required storage alignment 64 exceeds the selected target maximum of 16"
        );

        assert_eq!(
            renderer.render(&incompatible_pattern).message(),
            "pattern is incompatible with bool"
        );

        assert_eq!(
            renderer.render(&non_exhaustive_match).message(),
            "match does not cover every possible value"
        );
    }

    #[test]
    fn renderer_localizes_structured_emission_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionArtifactWriteFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_kind(
            DiagnosticArtifactKind::PackageInterface,
        ))
        .with_arg(DiagnosticArg::artifact_ordinal(0))
        .with_arg(DiagnosticArg::output_sink(DiagnosticOutputSink::Memory(
            "host.output".to_owned(),
        )))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::Other));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "could not write artifact output memory collector 'host.output': other I/O error"
        );
    }

    #[test]
    fn renderer_localizes_artifact_digest_mismatch_facts() {
        let expected =
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [0_u8; 32]);

        let actual =
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Sha256, [1_u8; 32]);

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionArtifactDigestMismatch,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::artifact_kind(
            DiagnosticArtifactKind::PackageInterface,
        ))
        .with_arg(DiagnosticArg::expected_artifact_digest(expected))
        .with_arg(DiagnosticArg::actual_artifact_digest(actual));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        let expected = format!(
            "package interface artifact declared digest SHA-256 {}, but content digest was SHA-256 {}",
            "00".repeat(32),
            "01".repeat(32)
        );

        assert_eq!(rendered.message(), expected);
    }

    #[test]
    fn renderer_formats_typed_arguments_for_utf8_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::SourceInvalidUtf8,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

        let rendered = DiagnosticRenderer::new(DiagnosticLocale::English).render(&diagnostic);

        assert_eq!(rendered.message(), "source input contains invalid UTF-8");

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);
        assert_eq!(note.message(), "source inputs must be valid UTF-8");
    }

    #[test]
    fn renderer_localizes_dependency_interface_context() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::InterfaceInvalidMagic,
            SeverityKind::Error,
        )
        .with_note(
            DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
                .with_arg(DiagnosticArg::expected_package_identity(
                    "example.dependency",
                ))
                .with_arg(DiagnosticArg::expected_product_identity("library"))
                .with_arg(DiagnosticArg::artifact_path("dependency.brayi")),
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Note);

        assert_eq!(
            note.message(),
            "while loading package 'example.dependency' product 'library' from dependency.brayi"
        );
    }

    #[test]
    fn renderer_renders_labels_with_spans_styles_and_typed_args() {
        let span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(3), TextSize::new(4)),
        );

        let label = DiagnosticLabel::primary(DiagnosticLabelKind::InvalidCharacter, span).with_arg(
            DiagnosticArg::new(
                DiagnosticArgName::Character,
                DiagnosticArgValue::Character('\u{7f}'),
            ),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::LexicalInvalidCharacter,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{7f}'),
        ))
        .with_label(label);

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), "invalid character U+007F");

        let [label] = rendered.labels() else {
            panic!("expected one rendered label: {rendered:?}");
        };

        assert_eq!(label.kind(), DiagnosticLabelKind::InvalidCharacter);
        assert_eq!(label.style(), DiagnosticLabelStyle::Primary);
        assert_eq!(label.span(), span);
        assert_eq!(label.message(), "invalid character U+007F");
    }

    #[test]
    fn renderer_renders_syntax_diagnostics_from_catalog() {
        let span = SourceSpan::empty(SourceId::new(0), TextSize::new(5));
        let expected = DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword);

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(5),
            DiagnosticKind::SyntaxExpectedToken,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(expected.clone())
        .with_label(
            DiagnosticLabel::primary(DiagnosticLabelKind::ExpectedTokenInsertionPoint, span)
                .with_arg(expected),
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), "expected func keyword");

        let [label] = rendered.labels() else {
            panic!("expected one rendered label: {rendered:?}");
        };

        assert_eq!(label.message(), "insert func keyword here");
    }

    #[test]
    fn renderer_renders_declaration_diagnostics_from_catalog() {
        let first_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(5)),
        );

        let duplicate_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(6), TextSize::new(11)),
        );

        let duplicate = Diagnostic::new(
            DiagnosticId::new(6),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::declaration_name("Point"))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::DuplicateDeclaration,
            duplicate_span,
        ))
        .with_label(DiagnosticLabel::secondary(
            DiagnosticLabelKind::FirstDeclaration,
            first_span,
        ));

        let visibility = Diagnostic::new(
            DiagnosticId::new(12),
            DiagnosticKind::DeclarationConflictingModuleVisibility,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::declaration_name("core"))
        .with_arg(DiagnosticArg::expected_visibility(
            DiagnosticVisibility::Public,
        ))
        .with_arg(DiagnosticArg::actual_visibility(
            DiagnosticVisibility::Internal,
        ));

        let trust = Diagnostic::new(
            DiagnosticId::new(13),
            DiagnosticKind::DeclarationConflictingModuleTrust,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::declaration_name("core"))
        .with_arg(DiagnosticArg::expected_module_trust(
            DiagnosticModuleTrust::Trusted,
        ))
        .with_arg(DiagnosticArg::actual_module_trust(
            DiagnosticModuleTrust::Ordinary,
        ));

        let modifier = Diagnostic::new(
            DiagnosticId::new(14),
            DiagnosticKind::DeclarationInvalidModifier,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::modifier_kind(SyntaxKind::PublicKeyword));

        let directives = Diagnostic::new(
            DiagnosticId::new(15),
            DiagnosticKind::DeclarationIncompatibleDirectives,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::directive_kind(
            SyntaxKind::EntrypointDirective,
        ))
        .with_arg(DiagnosticArg::conflicting_directive_kind(
            SyntaxKind::TestDirective,
        ));

        let renderer = DiagnosticRenderer::english();
        let duplicate = renderer.render(&duplicate);

        assert_eq!(duplicate.message(), "duplicate declaration of 'Point'");

        let [duplicate_label, first_label] = duplicate.labels() else {
            panic!("expected duplicate declaration labels: {duplicate:?}");
        };

        assert_eq!(duplicate_label.message(), "duplicate declaration");
        assert_eq!(first_label.message(), "first declared here");

        assert_eq!(
            renderer.render(&visibility).message(),
            "module 'core' has conflicting visibility: expected public, found internal"
        );

        assert_eq!(
            renderer.render(&trust).message(),
            "module 'core' has conflicting trust state: expected trusted, found non-trusted"
        );

        assert_eq!(
            renderer.render(&modifier).message(),
            "public modifier is not valid on this declaration"
        );

        assert_eq!(
            renderer.render(&directives).message(),
            "entrypoint and test directives cannot be combined"
        );
    }

    #[test]
    fn renderer_renders_expected_expression_diagnostics_from_catalog() {
        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::ZERO, TextSize::new(1)),
        );

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SyntaxExpectedExpression,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(DiagnosticArg::expected_syntax_kind(SyntaxKind::Expression))
        .with_arg(DiagnosticArg::actual_syntax_kind(SyntaxKind::AtToken))
        .with_arg(DiagnosticArg::token_text("@"))
        .with_label(
            DiagnosticLabel::primary(DiagnosticLabelKind::ExpectedExpression, span)
                .with_arg(DiagnosticArg::expected_syntax_kind(SyntaxKind::Expression)),
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), "expected expression");

        let [label] = rendered.labels() else {
            panic!("expected one rendered label: {rendered:?}");
        };

        assert_eq!(label.message(), "expected expression here");
    }

    #[test]
    fn renderer_renders_syntax_nesting_limits_from_typed_counts() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::SyntaxNestingLimitExceeded,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::maximum_count(128));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "syntax nesting exceeds the maximum depth of 128"
        );
    }

    #[test]
    fn renderer_renders_binding_diagnostics_from_structured_arguments() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(14),
            DiagnosticKind::BindingWrongNameKind,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("Size"))
        .with_arg(DiagnosticArg::expected_name_kind(DiagnosticNameKind::Type));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), "name 'Size' does not refer to a type");

        let shadowing = Diagnostic::new(
            DiagnosticId::new(15),
            DiagnosticKind::BindingNameAlreadyDefined,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("value"));

        let incoherent = Diagnostic::new(
            DiagnosticId::new(16),
            DiagnosticKind::BindingIncoherentAlternativePattern,
            SeverityKind::Error,
        );

        let missing_predicate_body = Diagnostic::new(
            DiagnosticId::new(17),
            DiagnosticKind::BindingPredicateBodyRequired,
            SeverityKind::Error,
        );

        let trusted_predicate_body = Diagnostic::new(
            DiagnosticId::new(18),
            DiagnosticKind::BindingTrustedPredicateBodyNotAllowed,
            SeverityKind::Error,
        );

        assert_eq!(
            DiagnosticRenderer::english().render(&shadowing).message(),
            "name is already defined: 'value'"
        );

        assert_eq!(
            DiagnosticRenderer::english().render(&incoherent).message(),
            "alternative patterns must bind the same names"
        );

        assert_eq!(
            DiagnosticRenderer::english()
                .render(&missing_predicate_body)
                .message(),
            "ordinary predicate declaration requires a body"
        );

        assert_eq!(
            DiagnosticRenderer::english()
                .render(&trusted_predicate_body)
                .message(),
            "trusted predicate declaration cannot have a body"
        );
    }

    #[test]
    fn renderer_keeps_diagnostic_bag_order() {
        let first = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RequestMissingSourceInput,
            SeverityKind::Error,
        );

        let second = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestInvalidWorkerBudget,
            SeverityKind::Error,
        );

        let bag = DiagnosticBag::from(vec![first, second]);
        let rendered = DiagnosticRenderer::english().render_bag(&bag);

        let ids = rendered
            .iter()
            .map(RenderedDiagnostic::id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![DiagnosticId::new(0), DiagnosticId::new(1)]);
    }

    #[test]
    fn renderer_exposes_missing_arguments_as_deterministic_placeholders() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceTextTooLarge,
            SeverityKind::Error,
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(
            rendered.message(),
            "source text is too large: {byte_count} bytes"
        );
    }

    #[test]
    fn renderer_formats_terminal_output_headings() {
        let renderer = DiagnosticRenderer::english();

        assert_eq!(renderer.render_severity(SeverityKind::Error), "error");

        assert_eq!(
            renderer.render_label_style(DiagnosticLabelStyle::Secondary),
            "secondary source"
        );

        assert_eq!(
            renderer.render_note_heading(RenderedDiagnosticNoteKind::Note),
            "note"
        );

        assert_eq!(
            renderer.render_note_heading(RenderedDiagnosticNoteKind::Help),
            "help"
        );
    }

    #[test]
    fn rendered_values_are_send_and_sync() {
        assert_send_sync::<DiagnosticRenderer>();
        assert_send_sync::<RenderedDiagnostic>();
        assert_send_sync::<RenderedDiagnosticLabel>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
