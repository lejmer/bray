use bray_diagnostics::DiagnosticBag;
use bray_source::{LineIndex, SourceLocation, SourceSnapshot, SourceSpan, TextRange};

use crate::diagnostic_output::DiagnosticJson;
use crate::source_location_output::{SourceLocationOutput, TextRangeOutput};

pub(crate) struct InspectionOutput {
    stdout: String,
    diagnostics: DiagnosticBag,
}

impl InspectionOutput {
    pub(crate) const fn new(stdout: String, diagnostics: DiagnosticBag) -> Self {
        Self {
            stdout,
            diagnostics,
        }
    }

    pub(crate) fn into_parts(self) -> (String, DiagnosticBag) {
        (self.stdout, self.diagnostics)
    }
}

#[derive(Default)]
pub(crate) struct TreeWriter {
    output: String,
    prefix: String,
    continuations: Vec<bool>,
}

impl TreeWriter {
    pub(crate) fn new(prefix: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            prefix: prefix.into(),
            continuations: Vec::new(),
        }
    }

    pub(crate) fn push_line(&mut self, is_last: bool, text: &str) {
        self.output.push_str(&self.prefix);

        for continues in &self.continuations {
            self.output
                .push_str(if *continues { "│  " } else { "   " });
        }

        self.output.push_str(if is_last { "└─ " } else { "├─ " });
        self.output.push_str(text);
        self.output.push('\n');
    }

    pub(crate) fn enter_children(&mut self, parent_is_last: bool) {
        self.continuations.push(!parent_is_last);
    }

    pub(crate) fn leave_children(&mut self) {
        if self.continuations.pop().is_none() {
            panic!("tree writer cannot leave a missing child level");
        }
    }

    pub(crate) fn into_string(self) -> String {
        self.output
    }
}

pub(crate) fn location_for_range(
    snapshot: &SourceSnapshot,
    line_index: &LineIndex,
    range: TextRange,
) -> Option<SourceLocationOutput> {
    let span = SourceSpan::new(snapshot.source_id(), range);
    let location = SourceLocation::resolve(snapshot, line_index, span)?;

    Some(SourceLocationOutput::from_location(location))
}

pub(crate) fn location_start_text(location: SourceLocationOutput) -> String {
    let start = location.start();

    format!("{}:{}", start.line(), start.column())
}

pub(crate) fn location_range_text(location: SourceLocationOutput) -> String {
    let start = location.start();
    let end = location.end();

    if start == end {
        return format!("{}:{}", start.line(), start.column());
    }

    format!(
        "{}:{}..{}:{}",
        start.line(),
        start.column(),
        end.line(),
        end.column()
    )
}

pub(crate) fn range_text(range: TextRangeOutput) -> String {
    format!("{}..{}", range.start(), range.end())
}

pub(crate) fn escaped_text(text: &str) -> String {
    text.escape_debug().to_string()
}

pub(crate) fn quoted_text(text: &str) -> String {
    format!("\"{text}\"")
}

pub(crate) fn push_text_diagnostic(output: &mut String, diagnostic: &DiagnosticJson) {
    output.push_str("    ");
    output.push_str(diagnostic.severity());
    output.push('[');
    output.push_str(&diagnostic.code().to_string());
    output.push_str("]: ");
    output.push_str(diagnostic.kind());

    if let Some(primary_span) = diagnostic.primary_span() {
        if let Some(location) = primary_span.location() {
            output.push(' ');
            output.push_str(&location_start_text(location));
        }

        output.push(' ');
        output.push_str(&primary_span.start().to_string());
        output.push_str("..");
        output.push_str(&primary_span.end().to_string());
    }

    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::TreeWriter;

    #[test]
    fn tree_writer_keeps_ancestor_guides_connected() {
        let mut writer = TreeWriter::default();

        writer.push_line(false, "first");
        writer.enter_children(false);
        writer.push_line(true, "nested");
        writer.leave_children();
        writer.push_line(true, "second");

        assert_eq!(
            writer.into_string(),
            "├─ first\n│  └─ nested\n└─ second\n"
        );
    }
}
