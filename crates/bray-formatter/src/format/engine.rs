use bray_parser::parse_source_unit;
use bray_source::{
    SourceIdentity, SourceLoadError, SourceLoader, SourceOrigin, SourceSnapshot, SourceVersion,
    TextSizeOverflow, leading_utf8_bom_len,
};
use bray_syntax::{
    SourceSyntaxNode, SourceUnitSyntax, SyntaxKind, SyntaxNodeView, SyntaxText, SyntaxToken,
    SyntaxTrivia, SyntaxWalkControl, SyntaxWalkEvent, walk_source_unit,
};

use super::context::{
    clears_pending_space_before, comma_uses_line_break, is_directive, is_generic_delimiter,
    is_line_comment, is_operator, is_prefix_operator, list_layout, needs_space_before,
    semicolon_stays_inline, should_separate_after,
};
use super::line_ending;
use super::model::FormattedSource;
use super::writer::FormatWriter;
use crate::{FormatterConfiguration, FormatterRule};

/// Parses and formats one UTF-8 Bray source text.
pub fn format_text(
    source_text: &str,
    configuration: &FormatterConfiguration,
) -> Result<FormattedSource, TextSizeOverflow> {
    let has_byte_order_mark = leading_utf8_bom_len(source_text.as_bytes()) > 0;
    let mut loader = SourceLoader::new();

    let snapshot = loader
        .load_snapshot(
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("formatter"),
            SourceVersion::new(0),
            source_text,
        )
        .map_err(|error| match error {
            SourceLoadError::TextTooLarge(error) => error,
            SourceLoadError::InvalidUtf8(_) => {
                unreachable!("owned UTF-8 formatter text cannot fail UTF-8 validation")
            }
            SourceLoadError::TooManySources { .. } => {
                unreachable!("a fresh formatter source loader must accept its first source")
            }
        })?;

    let formatted = format_snapshot(&snapshot, configuration);

    Ok(if has_byte_order_mark {
        formatted.with_leading_byte_order_mark()
    } else {
        formatted
    })
}

pub(crate) fn format_snapshot(
    snapshot: &SourceSnapshot,
    configuration: &FormatterConfiguration,
) -> FormattedSource {
    let result = parse_source_unit(snapshot);

    format_source_unit(result.source_unit(), configuration)
}

/// Formats one parsed Bray source unit.
///
/// Token spellings, comments, and skipped recovery nodes are copied from the
/// owning source snapshot. A source unit containing recovery is returned
/// unchanged so malformed regions remain lossless and idempotent.
pub fn format_source_unit(
    source_unit: &SourceUnitSyntax,
    configuration: &FormatterConfiguration,
) -> FormattedSource {
    let source_text = source_unit.source().text();

    if source_unit.is_recovered() {
        return FormattedSource {
            text: source_text.to_owned(),
            changed: false,
        };
    }

    let mut formatter =
        Formatter::new(source_text, line_ending::detect(source_text), configuration);

    walk_source_unit(source_unit, |event| formatter.visit(event));

    let text = formatter.finish();
    let changed = text != source_text;

    FormattedSource { text, changed }
}

struct Formatter<'source, 'configuration> {
    source_text: &'source str,
    configuration: &'configuration FormatterConfiguration,
    writer: FormatWriter,
    nodes: Vec<SyntaxKind>,
    previous_token: Option<SyntaxKind>,
    previous_operator_was_prefix: bool,
    previous_was_generic_delimiter: bool,
}

impl<'source, 'configuration> Formatter<'source, 'configuration> {
    fn new(
        source_text: &'source str,
        line_ending: &'static str,
        configuration: &'configuration FormatterConfiguration,
    ) -> Self {
        Self {
            source_text,
            configuration,
            writer: FormatWriter::new(line_ending, configuration.maximum_line_width().into()),
            nodes: Vec::new(),
            previous_token: None,
            previous_operator_was_prefix: false,
            previous_was_generic_delimiter: false,
        }
    }

    fn visit(&mut self, event: SyntaxWalkEvent<'_>) -> SyntaxWalkControl {
        match event {
            SyntaxWalkEvent::EnterNode(node) => self.enter_node(node),
            SyntaxWalkEvent::Token(token) => self.write_token(&token),
            SyntaxWalkEvent::ExitNode(node) => self.exit_node(node),
        }
    }

    fn enter_node(&mut self, node: SyntaxNodeView<'_>) -> SyntaxWalkControl {
        if node.kind() == SyntaxKind::SkippedSyntax {
            self.write_skipped_syntax(node);

            return SyntaxWalkControl::SkipChildren;
        }

        self.nodes.push(node.kind());

        SyntaxWalkControl::Continue
    }

    fn exit_node(&mut self, node: SyntaxNodeView<'_>) -> SyntaxWalkControl {
        if node.kind() == SyntaxKind::SkippedSyntax {
            return SyntaxWalkControl::Continue;
        }

        let popped = self.nodes.pop();

        assert_eq!(
            popped,
            Some(node.kind()),
            "formatter syntax traversal must remain balanced"
        );

        if is_directive(node.kind()) {
            self.writer.request_newlines(1);
        }

        if should_separate_after(node.kind(), self.nodes.last().copied()) {
            self.writer.request_newlines(2);
        }

        SyntaxWalkControl::Continue
    }

    fn write_skipped_syntax(&mut self, node: SyntaxNodeView<'_>) {
        let last_token = node.tokens().filter(|token| token.is_present()).last();
        let text = node.full_text();

        self.writer.write_exact(&text);
        self.previous_token = last_token.map(|token| token.kind());
        self.previous_operator_was_prefix = false;
        self.previous_was_generic_delimiter = false;
    }

    fn write_token(&mut self, token: &SyntaxToken) -> SyntaxWalkControl {
        if token.is_missing() {
            return SyntaxWalkControl::Continue;
        }

        self.write_trivia(token.leading_trivia());

        if token.is_end_of_file() {
            self.write_trivia(token.trailing_trivia());

            return SyntaxWalkControl::Continue;
        }

        let kind = token.kind();
        let parent = self.nodes.last().copied();
        let generic_delimiter = is_generic_delimiter(kind, parent);
        let list_layout = self.enabled_list_layout(parent);

        let operator_is_prefix = is_operator(kind)
            && !generic_delimiter
            && self
                .previous_token
                .is_none_or(|previous| is_prefix_operator(kind, previous));

        self.prepare_token(kind, operator_is_prefix, generic_delimiter, list_layout);

        if matches!(
            kind,
            SyntaxKind::OpenParenToken | SyntaxKind::OpenBracketToken
        ) && list_layout
        {
            self.writer.begin_group();
        }

        self.writer
            .write(required_token_text(token, self.source_text));

        self.previous_token = Some(kind);
        self.previous_operator_was_prefix = operator_is_prefix;
        self.previous_was_generic_delimiter = generic_delimiter;

        self.write_trivia(token.trailing_trivia());
        self.finish_token(kind, parent, list_layout);

        if matches!(
            kind,
            SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
        ) && list_layout
        {
            self.writer.end_group();
        }

        SyntaxWalkControl::Continue
    }

    fn prepare_token(
        &mut self,
        kind: SyntaxKind,
        operator_is_prefix: bool,
        generic_delimiter: bool,
        list_layout: bool,
    ) {
        if clears_pending_space_before(kind) {
            self.writer.clear_space();
        }

        if matches!(
            kind,
            SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken
        ) && list_layout
        {
            self.writer.decrease_indent();
            self.writer.request_optional_break(false);
        } else if kind == SyntaxKind::CloseBraceToken {
            self.writer.decrease_indent();
            self.writer.set_newlines(1);
        } else if kind == SyntaxKind::OpenBraceToken
            || matches!(self.previous_token, Some(SyntaxKind::CloseBraceToken))
                && !matches!(
                    kind,
                    SyntaxKind::SemicolonToken
                        | SyntaxKind::CommaToken
                        | SyntaxKind::CloseBraceToken
                )
        {
            self.writer.request_newlines(1);
        }

        if needs_space_before(
            self.previous_token,
            self.previous_operator_was_prefix,
            self.previous_was_generic_delimiter,
            kind,
            operator_is_prefix,
            generic_delimiter,
        ) {
            self.writer.request_space();
        }
    }

    fn finish_token(&mut self, kind: SyntaxKind, parent: Option<SyntaxKind>, list_layout: bool) {
        match kind {
            SyntaxKind::OpenParenToken | SyntaxKind::OpenBracketToken if list_layout => {
                self.writer.increase_indent();
                self.writer.request_optional_break(false);
            }
            SyntaxKind::OpenBraceToken => {
                self.writer.increase_indent();
                self.writer.request_newlines(1);
            }
            SyntaxKind::CommaToken if self.enabled_list_layout(parent) => {
                self.writer.request_optional_break(true);
            }
            SyntaxKind::CommaToken if comma_uses_line_break(parent) => {
                self.writer.request_newlines(1);
            }
            SyntaxKind::CommaToken => self.writer.request_space(),
            SyntaxKind::SemicolonToken if !semicolon_stays_inline(parent) => {
                self.writer.request_newlines(1);
            }
            SyntaxKind::SemicolonToken => self.writer.request_space(),
            SyntaxKind::ColonToken
            | SyntaxKind::ArrowToken
            | SyntaxKind::EqualsToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::BangEqualsToken
            | SyntaxKind::LessEqualsToken
            | SyntaxKind::GreaterEqualsToken
            | SyntaxKind::AmpersandAmpersandToken
            | SyntaxKind::PipePipeToken
            | SyntaxKind::LessLessToken
            | SyntaxKind::GreaterGreaterToken => self.writer.request_space(),
            _ => {}
        }
    }

    fn write_trivia(&mut self, trivia: &[SyntaxTrivia]) {
        let mut line_breaks = 0_u8;
        let mut saw_comment = false;

        for item in trivia {
            let text = required_trivia_text(item, self.source_text);

            if item.kind() == SyntaxKind::WhitespaceTrivia {
                line_breaks = line_breaks.saturating_add(line_ending::count(text));

                continue;
            }

            self.apply_trivia_line_breaks(line_breaks);
            line_breaks = 0;
            saw_comment = true;

            if !self.writer.is_line_start() {
                self.writer.request_space();
            }

            self.writer.write_exact(text);

            if is_line_comment(item.kind()) || line_ending::contains(text) {
                self.writer.request_newlines(1);
            }
        }

        if saw_comment {
            self.apply_trivia_line_breaks(line_breaks);
        }
    }

    fn apply_trivia_line_breaks(&mut self, line_breaks: u8) {
        match line_breaks {
            0 => {}
            1 => self.writer.request_newlines(1),
            _ => self.writer.request_newlines(2),
        }
    }

    fn finish(self) -> String {
        self.writer.finish()
    }

    fn enabled_list_layout(&self, parent: Option<SyntaxKind>) -> bool {
        self.configuration.is_enabled(FormatterRule::LineWrapping)
            && list_layout(parent).is_some_and(|rule| self.configuration.is_enabled(rule))
    }
}

fn required_token_text<'source>(token: &SyntaxToken, source_text: &'source str) -> &'source str {
    match token.text(source_text) {
        Some(text) => text,
        None => panic!("syntax token range must be covered by its source snapshot"),
    }
}

fn required_trivia_text<'source>(trivia: &SyntaxTrivia, source_text: &'source str) -> &'source str {
    match trivia.text(source_text) {
        Some(text) => text,
        None => panic!("syntax trivia range must be covered by its source snapshot"),
    }
}
