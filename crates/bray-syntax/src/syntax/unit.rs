use std::fmt::{self, Write};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::{SourceSnapshot, TextRange, TextSize};

use super::{SkippedSyntax, SourceSyntaxNode, SyntaxNode};
use crate::builder::{GreenNodeBuilder, RequiredSyntaxSlot, SyntaxListSlot, require_token_kind};
use crate::green::GreenNode;
use crate::{SyntaxKind, SyntaxText, SyntaxToken, SyntaxTokenPresence};

/// Root syntax node for one compiler compilation unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CompilationUnitSyntax {
    source_units: Arc<[SourceUnitSyntax]>,
}

impl CompilationUnitSyntax {
    /// Creates a builder for a compilation-unit node.
    pub const fn builder() -> CompilationUnitSyntaxBuilder {
        CompilationUnitSyntaxBuilder::new()
    }

    fn from_builder(builder: CompilationUnitSyntaxBuilder) -> Self {
        Self {
            source_units: shared_slice(builder.source_units.into_vec()),
        }
    }

    /// Returns this node's stable syntax kind.
    pub const fn kind(&self) -> SyntaxKind {
        SyntaxKind::CompilationUnit
    }

    /// Returns this node's full range.
    ///
    /// Compilation units contain source units from distinct source coordinate
    /// spaces, so the root range is intentionally empty.
    pub const fn full_range(&self) -> TextRange {
        TextRange::EMPTY
    }

    /// Returns the source-unit children in source ID order.
    pub fn source_units(&self) -> &[SourceUnitSyntax] {
        &self.source_units
    }

    /// Returns whether this compilation unit contains no source units.
    pub fn is_empty(&self) -> bool {
        self.source_units.is_empty()
    }
}

/// Builder for a compilation-unit syntax node.
#[derive(Debug, Default)]
pub struct CompilationUnitSyntaxBuilder {
    source_units: SyntaxListSlot<SourceUnitSyntax>,
}

impl CompilationUnitSyntaxBuilder {
    /// Creates an empty compilation-unit builder.
    pub const fn new() -> Self {
        Self {
            source_units: SyntaxListSlot::new(),
        }
    }

    /// Appends a source-unit child to the compilation-unit child list.
    pub fn push_source_unit(&mut self, source_unit: SourceUnitSyntax) {
        self.source_units.push(source_unit);
    }

    /// Appends source-unit children to the compilation-unit child list.
    pub fn source_units(
        mut self,
        source_units: impl IntoIterator<Item = SourceUnitSyntax>,
    ) -> Self {
        self.source_units.extend(source_units);

        self
    }

    /// Builds the compilation-unit node.
    pub fn build(self) -> CompilationUnitSyntax {
        CompilationUnitSyntax::from_builder(self)
    }
}

impl SyntaxText for CompilationUnitSyntax {
    /// Appends the exact source text for every source unit in order.
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        for source_unit in self.source_units() {
            source_unit.write_full_text(writer)?;
        }

        Ok(())
    }
}

impl SyntaxNode for CompilationUnitSyntax {
    fn kind(&self) -> SyntaxKind {
        self.kind()
    }

    fn full_range(&self) -> TextRange {
        self.full_range()
    }
}

/// Root syntax node for one parsed source snapshot.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SourceUnitSyntax {
    source: SourceSnapshot,
    node: GreenNode,
}

impl SourceUnitSyntax {
    /// Creates a builder for a source-unit node.
    pub fn builder(source: SourceSnapshot) -> SourceUnitSyntaxBuilder {
        SourceUnitSyntaxBuilder::new(source)
    }

    fn from_builder(builder: SourceUnitSyntaxBuilder) -> Self {
        let source = builder.source.into_value();

        match builder.node.last_token() {
            Some((kind, SyntaxTokenPresence::Present)) if kind == SyntaxKind::EndOfFileToken => {
                require_token_kind(kind, SyntaxKind::EndOfFileToken, "source_unit.eof_token");
            }
            Some((SyntaxKind::EndOfFileToken, SyntaxTokenPresence::Missing)) => {
                panic!("source unit token stream must end with present EOF");
            }
            _ => panic!("source unit token stream must end with EOF"),
        }

        let node = builder.node.build(SyntaxKind::SourceUnit);

        Self { source, node }
    }

    /// Returns this node's stable syntax kind.
    pub fn kind(&self) -> SyntaxKind {
        self.node.kind()
    }

    /// Returns the full source text range.
    pub fn full_range(&self) -> TextRange {
        match TextRange::with_len(TextSize::ZERO, self.node.full_width()) {
            Some(range) => range,
            None => panic!("green source-unit width must fit in TextRange"),
        }
    }

    /// Returns the immutable source snapshot this node was parsed from.
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Returns this source unit's syntax tokens in source order, including EOF.
    ///
    /// Tokens are synthesized from immutable green storage and this source
    /// unit's coordinate space. Grammar-aware code should prefer named slots
    /// and child lists on concrete syntax nodes as those nodes are added.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(TextSize::ZERO)
    }

    /// Returns skipped-syntax recovery nodes in source order.
    pub fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        self.node
            .skipped_syntax_nodes(TextSize::ZERO)
            .map(|(node, start)| {
                // SourceSnapshot clones share immutable source text with typed recovery nodes.
                SkippedSyntax::from_green(self.source.clone(), node, start)
            })
    }

    /// Returns the required EOF token for this source unit.
    pub fn eof_token(&self) -> SyntaxToken {
        match self.node.syntax_tokens(TextSize::ZERO).last() {
            Some(token) => token,
            None => panic!("source unit token stream must end with EOF"),
        }
    }
}

impl fmt::Debug for SourceUnitSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceUnitSyntax")
            .field("source", &self.source)
            .field("kind", &self.kind())
            .field("full_range", &self.full_range())
            .finish()
    }
}

/// Builder for a source-unit syntax node.
pub struct SourceUnitSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
}

impl SourceUnitSyntaxBuilder {
    /// Creates an empty source-unit builder for `source`.
    pub fn new(source: SourceSnapshot) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("source_unit.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
        }
    }

    /// Appends a token to the source-unit token list.
    pub fn push_token(&mut self, token: SyntaxToken) {
        self.node.push_token(token);
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    ///
    /// Empty token lists do not add a recovery node.
    ///
    /// Panics when any skipped token is missing.
    pub fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.node.push_skipped_tokens(tokens);
    }

    /// Appends tokens to the source-unit token list.
    pub fn tokens(mut self, tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        self.node.push_tokens(tokens);

        self
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    ///
    /// Empty token lists do not add a recovery node.
    ///
    /// Panics when any skipped token is missing.
    pub fn skipped_tokens(mut self, tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        self.push_skipped_tokens(tokens);

        self
    }

    /// Builds the source-unit node.
    ///
    /// Panics when the token list does not end in EOF.
    pub fn build(self) -> SourceUnitSyntax {
        SourceUnitSyntax::from_builder(self)
    }
}

impl fmt::Debug for SourceUnitSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceUnitSyntaxBuilder")
            .finish_non_exhaustive()
    }
}

impl SyntaxNode for SourceUnitSyntax {
    fn kind(&self) -> SyntaxKind {
        self.kind()
    }

    fn full_range(&self) -> TextRange {
        self.full_range()
    }
}

impl SourceSyntaxNode for SourceUnitSyntax {
    fn write_full_text_from(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        self.node
            .write_source_text(source_text, TextSize::ZERO, writer)
    }
}

impl SyntaxText for SourceUnitSyntax {
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        self.write_full_text_from(self.source.text(), writer)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{
        SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
    };

    use super::{CompilationUnitSyntax, SourceUnitSyntax};
    use crate::{SourceSyntaxNode, SyntaxKind, SyntaxNode, SyntaxText, SyntaxToken, SyntaxTrivia};

    #[test]
    fn source_units_store_source_snapshot_named_tokens_and_full_range() {
        let snapshot = snapshot("func");

        let first = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let eof = SyntaxToken::end_of_file(TextSize::new(4));

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .tokens([first.clone(), eof.clone()])
            .build();

        assert_eq!(source_unit.kind(), SyntaxKind::SourceUnit);

        assert_eq!(
            source_unit.full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(4))
        );

        assert_eq!(
            source_unit.tokens().collect::<Vec<_>>(),
            [first, eof.clone()]
        );

        assert_eq!(source_unit.eof_token(), eof);
    }

    #[test]
    fn compilation_units_store_named_source_unit_children() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build();

        let compilation_unit = CompilationUnitSyntax::builder()
            .source_units([source_unit.clone()])
            .build();

        assert_eq!(compilation_unit.kind(), SyntaxKind::CompilationUnit);
        assert_eq!(compilation_unit.source_units(), &[source_unit]);
        assert_eq!(compilation_unit.full_range(), TextRange::EMPTY);
    }

    #[test]
    fn typed_nodes_implement_syntax_node_contract() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build();

        let compilation_unit = CompilationUnitSyntax::builder()
            .source_units([source_unit.clone()])
            .build();

        assert_node_kind(&source_unit, SyntaxKind::SourceUnit);
        assert_node_kind(&compilation_unit, SyntaxKind::CompilationUnit);
    }

    #[test]
    fn source_units_reconstruct_exact_source_text_from_tokens() {
        let snapshot = snapshot("  func// tail");
        let leading = SyntaxTrivia::whitespace(TextRange::new(TextSize::ZERO, TextSize::new(2)));

        let trailing =
            SyntaxTrivia::line_comment(TextRange::new(TextSize::new(6), TextSize::new(13)));

        let token = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::new(2), TextSize::new(6)),
        )
        .with_leading_trivia([leading])
        .with_trailing_trivia([trailing]);

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .tokens([token, SyntaxToken::end_of_file(TextSize::new(13))])
            .build();

        assert_eq!(
            source_unit.full_text_from(source_unit.source().text()),
            "  func// tail"
        );

        let compilation_unit = CompilationUnitSyntax::builder()
            .source_units([source_unit])
            .build();

        assert_eq!(compilation_unit.full_text(), "  func// tail");
    }

    #[test]
    fn source_units_preserve_missing_tokens_in_named_slots() {
        let snapshot = snapshot("func");
        let token = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let missing = SyntaxToken::missing(SyntaxKind::SemicolonToken, TextSize::new(4));
        let eof = SyntaxToken::end_of_file(TextSize::new(4));

        let source_unit = SourceUnitSyntax::builder(snapshot)
            .tokens([token.clone(), missing.clone(), eof.clone()])
            .build();

        let tokens = source_unit.tokens().collect::<Vec<_>>();

        assert_eq!(tokens, [token, missing.clone(), eof]);
        assert!(tokens[1].is_missing());
        assert_eq!(tokens[1].kind(), SyntaxKind::SemicolonToken);
        assert_eq!(source_unit.full_text(), "func");
    }

    #[test]
    fn source_units_attach_skipped_syntax_without_losing_source_text() {
        let snapshot = snapshot("func @ main");
        let func = SyntaxToken::new(
            SyntaxKind::FuncKeyword,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        )
        .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(4),
            TextSize::new(5),
        ))]);

        let skipped_token =
            SyntaxToken::invalid(TextRange::new(TextSize::new(5), TextSize::new(6)))
                .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                    TextSize::new(6),
                    TextSize::new(7),
                ))]);

        let main = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(7), TextSize::new(11)),
        );

        let eof = SyntaxToken::end_of_file(TextSize::new(11));
        let mut builder = SourceUnitSyntax::builder(snapshot);

        builder.push_token(func.clone());
        builder.push_skipped_tokens([skipped_token.clone()]);
        builder.push_token(main.clone());
        builder.push_token(eof.clone());

        let source_unit = builder.build();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(source_unit.full_text(), "func @ main");
        assert_eq!(
            source_unit.tokens().collect::<Vec<_>>(),
            [func, skipped_token.clone(), main, eof]
        );

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.kind(), SyntaxKind::SkippedSyntax);
        assert!(skipped.is_recovered());
        assert_eq!(
            skipped.full_range(),
            TextRange::new(TextSize::new(5), TextSize::new(7))
        );
        assert_eq!(skipped.tokens().collect::<Vec<_>>(), [skipped_token]);
        assert_eq!(skipped.full_text(), "@ ");
    }

    #[test]
    fn source_unit_debug_does_not_expose_green_storage() {
        let source_unit = SourceUnitSyntax::builder(snapshot(""))
            .tokens([SyntaxToken::end_of_file(TextSize::ZERO)])
            .build();

        let debug_text = format!("{source_unit:?}");

        assert!(debug_text.contains("SourceUnitSyntax"));
        assert!(debug_text.contains("kind"));
        assert!(!debug_text.contains("GreenNode"));
    }

    #[test]
    fn typed_syntax_nodes_are_send_and_sync() {
        assert_send_sync::<CompilationUnitSyntax>();
        assert_send_sync::<SourceUnitSyntax>();
    }

    #[test]
    #[should_panic]
    fn source_units_reject_token_streams_without_eof() {
        let _ = SourceUnitSyntax::builder(snapshot("func"))
            .tokens(Vec::new())
            .build();
    }

    fn assert_node_kind(node: &impl SyntaxNode, kind: SyntaxKind) {
        assert_eq!(node.kind(), kind);
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("syntax-test"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
