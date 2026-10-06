use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticBag, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelStyle, DiagnosticNote, DiagnosticNoteKind, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, DiagnosticSuggestion, DiagnosticSuggestionKind, SeverityKind,
};
use bray_source::{SourceLocation, SourceSpan};

use crate::argument::{ArgumentFormatter, format_source_location, format_source_span};
use crate::catalog::{MessageCatalog, MessageTemplate, MessageTemplatePart};
use crate::locale::DiagnosticLocale;
use crate::rendered_diagnostic::{
    RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote,
    RenderedDiagnosticNoteKind, RenderedDiagnosticRelatedLocation, RenderedDiagnosticSuggestion,
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

        let related_locations = diagnostic
            .related_locations()
            .iter()
            .map(|location| self.render_related_location(location))
            .collect();

        let suggestions = diagnostic
            .suggestions()
            .iter()
            .map(|suggestion| self.render_suggestion(suggestion))
            .collect();

        RenderedDiagnostic::new(
            diagnostic.id(),
            diagnostic.kind(),
            diagnostic.severity(),
            diagnostic.primary_span(),
            self.render_template(template, diagnostic.args()),
            labels,
            notes,
            related_locations,
            suggestions,
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

    /// Renders the heading for an ordinary source label.
    pub fn render_label_heading(self) -> &'static str {
        MessageCatalog::new(self.locale).label_heading()
    }

    /// Renders the heading for a supporting source location.
    pub fn render_related_location_heading(self) -> &'static str {
        MessageCatalog::new(self.locale).related_location_heading()
    }

    /// Returns typed arguments referenced by a diagnostic's primary message template.
    pub fn diagnostic_argument_names(self, kind: DiagnosticKind) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).diagnostic_template(kind))
    }

    /// Returns whether the primary template contains recovery or next-action prose.
    ///
    /// This English-catalog guardrail complements semantic review. A primary diagnostic message
    /// identifies what failed and why. Recovery belongs in structured notes or suggestions.
    pub fn primary_message_contains_recovery_instruction(self, kind: DiagnosticKind) -> bool {
        MessageCatalog::new(self.locale)
            .diagnostic_template(kind)
            .contains_recovery_instruction()
    }

    /// Returns typed arguments referenced by one note template.
    pub fn note_argument_names(self, kind: DiagnosticNoteKind) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).note_template(kind))
    }

    /// Returns typed arguments referenced by one related-location template.
    pub fn related_location_argument_names(
        self,
        kind: DiagnosticRelatedLocationKind,
    ) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).related_location_template(kind))
    }

    /// Returns typed arguments referenced by one suggestion template.
    pub fn suggestion_argument_names(
        self,
        kind: DiagnosticSuggestionKind,
    ) -> Vec<DiagnosticArgName> {
        template_argument_names(MessageCatalog::new(self.locale).suggestion_template(kind))
    }

    /// Returns every typed argument name consumed by an actually present rendered component.
    pub fn component_argument_names(self, diagnostic: &Diagnostic) -> Vec<DiagnosticArgName> {
        let catalog = MessageCatalog::new(self.locale);
        let mut names = template_argument_names(catalog.diagnostic_template(diagnostic.kind()));

        for label in diagnostic.labels() {
            names.extend(template_argument_names(
                catalog.label_template(label.kind()),
            ));
        }

        for note in diagnostic.notes() {
            names.extend(template_argument_names(catalog.note_template(note.kind())));
        }

        for location in diagnostic.related_locations() {
            names.extend(template_argument_names(
                catalog.related_location_template(location.kind()),
            ));
        }

        for suggestion in diagnostic.suggestions() {
            names.extend(template_argument_names(
                catalog.suggestion_template(suggestion.kind()),
            ));
        }

        names.sort_unstable();
        names.dedup();

        names
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

    fn render_related_location(
        self,
        location: &DiagnosticRelatedLocation,
    ) -> RenderedDiagnosticRelatedLocation {
        let template = MessageCatalog::new(self.locale).related_location_template(location.kind());

        RenderedDiagnosticRelatedLocation::new(
            location.kind(),
            location.span(),
            self.render_template(template, location.args()),
        )
    }

    fn render_suggestion(self, suggestion: &DiagnosticSuggestion) -> RenderedDiagnosticSuggestion {
        let template = MessageCatalog::new(self.locale).suggestion_template(suggestion.kind());

        RenderedDiagnosticSuggestion::new(
            suggestion.kind(),
            suggestion.applicability(),
            suggestion.edits().to_vec(),
            self.render_template(template, suggestion.args()),
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

fn template_argument_names(template: MessageTemplate) -> Vec<DiagnosticArgName> {
    template
        .parts()
        .iter()
        .filter_map(|part| match part {
            MessageTemplatePart::Text(_) => None,
            MessageTemplatePart::Arg(name) => Some(*name),
        })
        .collect()
}
