use std::fmt::{self, Write};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_source::{SourceSnapshot, TextRange};

use super::{SourceOrderElements, SourceSyntaxNode, SyntaxNode, SyntaxText};
use crate::{SyntaxKind, SyntaxToken};

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
            source_units: shared_slice(builder.source_units),
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
    source_units: Vec<SourceUnitSyntax>,
}

impl CompilationUnitSyntaxBuilder {
    /// Creates an empty compilation-unit builder.
    pub const fn new() -> Self {
        Self {
            source_units: Vec::new(),
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceUnitSyntax {
    source: SourceSnapshot,
    tokens: Arc<[SyntaxToken]>,
    elements: SourceOrderElements,
}

impl SourceUnitSyntax {
    /// Creates a builder for a source-unit node.
    pub fn builder(source: SourceSnapshot) -> SourceUnitSyntaxBuilder {
        SourceUnitSyntaxBuilder::new(source)
    }

    fn from_builder(builder: SourceUnitSyntaxBuilder) -> Self {
        let tokens = shared_slice(builder.tokens);

        match tokens.last() {
            Some(token) if token.is_end_of_file() => {}
            _ => panic!("source unit token stream must end with EOF"),
        }

        let elements = elements_from_tokens(tokens.as_ref());

        Self {
            source: builder.source,
            tokens,
            elements,
        }
    }

    /// Returns this node's stable syntax kind.
    pub const fn kind(&self) -> SyntaxKind {
        SyntaxKind::SourceUnit
    }

    /// Returns the full source text range.
    pub const fn full_range(&self) -> TextRange {
        self.source.full_range()
    }

    /// Returns the immutable source snapshot this node was parsed from.
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Returns this source unit's lexical token list, including EOF.
    ///
    /// This list is useful for source reconstruction and token-stream
    /// inspection. Grammar-aware code should prefer named slots and child lists
    /// on concrete syntax nodes as those nodes are added.
    pub fn tokens(&self) -> impl Iterator<Item = &SyntaxToken> + '_ {
        self.tokens.iter()
    }

    /// Returns the required EOF token for this source unit.
    pub fn eof_token(&self) -> &SyntaxToken {
        match self.tokens.last() {
            Some(token) => token,
            None => panic!("source unit token stream must end with EOF"),
        }
    }
}

fn elements_from_tokens(tokens: &[SyntaxToken]) -> SourceOrderElements {
    // SyntaxToken clones share immutable trivia storage. The element storage is
    // derived from the named token list for reconstruction infrastructure.
    SourceOrderElements::from_tokens(tokens.iter().cloned())
}

/// Builder for a source-unit syntax node.
#[derive(Debug)]
pub struct SourceUnitSyntaxBuilder {
    source: SourceSnapshot,
    tokens: Vec<SyntaxToken>,
}

impl SourceUnitSyntaxBuilder {
    /// Creates an empty source-unit builder for `source`.
    pub fn new(source: SourceSnapshot) -> Self {
        Self {
            source,
            tokens: Vec::new(),
        }
    }

    /// Appends a token to the source-unit token list.
    pub fn push_token(&mut self, token: SyntaxToken) {
        self.tokens.push(token);
    }

    /// Appends tokens to the source-unit token list.
    pub fn tokens(mut self, tokens: impl IntoIterator<Item = SyntaxToken>) -> Self {
        self.tokens.extend(tokens);

        self
    }

    /// Builds the source-unit node.
    ///
    /// Panics when the token list does not end in EOF.
    pub fn build(self) -> SourceUnitSyntax {
        SourceUnitSyntax::from_builder(self)
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
        self.elements.write_source_text(source_text, writer)
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
            source_unit.tokens().cloned().collect::<Vec<_>>(),
            [first, eof.clone()]
        );

        assert_eq!(source_unit.eof_token(), &eof);
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
