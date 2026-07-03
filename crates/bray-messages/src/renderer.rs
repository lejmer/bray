use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticLabel, DiagnosticNote};

use crate::argument::ArgumentFormatter;
use crate::catalog::{MessageCatalog, MessageTemplate, MessageTemplatePart};
use crate::locale::DiagnosticLocale;
use crate::rendered::{RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote};

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
        let template = MessageCatalog::new(self.locale).note_template(note.kind());

        RenderedDiagnosticNote::new(note.kind(), self.render_template(template, note.args()))
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
        Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag,
        DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
        DiagnosticLabelStyle, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
    };
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    use super::DiagnosticRenderer;
    use crate::{DiagnosticLocale, RenderedDiagnostic, RenderedDiagnosticLabel};

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
        assert_eq!(
            note.message(),
            "source files must be readable before compilation"
        );
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

        assert_eq!(
            rendered.message(),
            "source input contains invalid UTF-8 at byte offset 4"
        );

        let [note] = rendered.notes() else {
            panic!("expected one rendered note: {rendered:?}");
        };

        assert_eq!(note.message(), "source inputs must be valid UTF-8");
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
    fn rendered_values_are_send_and_sync() {
        assert_send_sync::<DiagnosticRenderer>();
        assert_send_sync::<RenderedDiagnostic>();
        assert_send_sync::<RenderedDiagnosticLabel>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
