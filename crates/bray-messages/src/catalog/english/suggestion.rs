use bray_diagnostics::{DiagnosticArgName, DiagnosticSuggestionKind};

use crate::catalog::{MessageTemplate, MessageTemplatePart};

const INSERT_EXPECTED_SYNTAX: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("insert "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];
const REPLACE_WITH_LINE_FEED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "replace this carriage return with a line feed",
)];
const ADD_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("add the missing terminator")];
const FORMAT_SOURCE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "format this source with `bray fmt`",
)];

pub(crate) const fn template(kind: DiagnosticSuggestionKind) -> MessageTemplate {
    let parts = match kind {
        DiagnosticSuggestionKind::InsertExpectedSyntax => INSERT_EXPECTED_SYNTAX,
        DiagnosticSuggestionKind::ReplaceWithLineFeed => REPLACE_WITH_LINE_FEED,
        DiagnosticSuggestionKind::AddTerminator => ADD_TERMINATOR,
        DiagnosticSuggestionKind::FormatSource => FORMAT_SOURCE,
    };

    MessageTemplate::new(parts)
}
