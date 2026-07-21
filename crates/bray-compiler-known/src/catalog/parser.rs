use std::sync::Arc;

use bray_parser::LexerTokenSource;
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange,
};
use bray_syntax::{SyntaxKind, SyntaxToken};

use super::diagnostic::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogEntryKind, CatalogExpectation,
};
use super::entry::{
    Anchored, ParsedCatalogSource, ParsedDeclaration, ParsedDeclarationField,
    ParsedDeclarationIdentity, ParsedEntry, ParsedScope, ParsedScopeLocation, ParsedValue,
    ParsedValueField,
};
use super::{
    CatalogDeclarationSurface, CatalogKind, CatalogSource, CatalogSourceAnchor,
    CatalogTokenSpelling, CatalogTypeSurface,
};

const CATALOG_WORD: &str = "catalog";
const SCOPE_WORD: &str = "scope";
const AT_WORD: &str = "at";
const DECLARATION_WORD: &str = "declaration";
const VALUE_WORD: &str = "value";
const COMPILER_KNOWN_WORD: &str = "compiler_known";
const RECOGNIZED_STANDARD_LIBRARY_WORD: &str = "recognized_standard_library";
const AMBIENT_WORD: &str = "ambient";
const OWNER_WORD: &str = "owner";
const IDENTITY_WORD: &str = "identity";
const NAME_WORD: &str = "name";
const ORDINAL_WORD: &str = "ordinal";
const AVAILABILITY_WORD: &str = "availability";
const REPRESENTATION_WORD: &str = "representation";
const IMPLEMENTATION_WORD: &str = "implementation";
const OPERATION_WORD: &str = "operation";
const SURFACE_WORD: &str = "surface";
const SPELLING_WORD: &str = "spelling";
const TYPE_WORD: &str = "type";

pub(super) fn parse_catalog_source(
    source: CatalogSource,
) -> (Option<ParsedCatalogSource>, Vec<CatalogDiagnostic>) {
    let snapshot = match source_snapshot(source) {
        Ok(snapshot) => snapshot,
        Err(diagnostic) => return (None, vec![diagnostic]),
    };

    CatalogParser::new(source, snapshot).parse()
}

struct CatalogParser {
    source: CatalogSource,
    tokens: LexerTokenSource,
    diagnostics: Vec<CatalogDiagnostic>,
}

impl CatalogParser {
    fn new(source: CatalogSource, snapshot: SourceSnapshot) -> Self {
        Self {
            source,
            tokens: LexerTokenSource::new(snapshot),
            diagnostics: Vec::new(),
        }
    }

    fn parse(mut self) -> (Option<ParsedCatalogSource>, Vec<CatalogDiagnostic>) {
        let parsed = self.parse_file();

        self.record_lexical_diagnostics();

        if self.diagnostics.is_empty() {
            (parsed, self.diagnostics)
        } else {
            (None, self.diagnostics)
        }
    }

    fn parse_file(&mut self) -> Option<ParsedCatalogSource> {
        self.expect_word(CATALOG_WORD)?;
        let declared_kind = self.parse_catalog_kind()?;

        self.expect_kind(SyntaxKind::SemicolonToken, CatalogExpectation::Semicolon)?;

        let mut scopes = Vec::new();

        while !self.at(SyntaxKind::EndOfFileToken) {
            match self.parse_scope() {
                Some(scope) => scopes.push(scope),
                None => {
                    self.synchronize_to_scope();

                    if self.at(SyntaxKind::EndOfFileToken) {
                        break;
                    }
                }
            }
        }

        self.expect_kind(SyntaxKind::EndOfFileToken, CatalogExpectation::EndOfFile)?;

        Some(ParsedCatalogSource {
            source_kind: self.source.kind(),
            declared_kind,
            scopes,
        })
    }

    fn parse_catalog_kind(&mut self) -> Option<Anchored<CatalogKind>> {
        let token = self.consume_identifier(CatalogExpectation::Identifier)?;
        let spelling = self.token_text(&token);

        let kind = match spelling.as_ref() {
            COMPILER_KNOWN_WORD => CatalogKind::CompilerKnown,
            RECOGNIZED_STANDARD_LIBRARY_WORD => CatalogKind::RecognizedStandardLibrary,
            _ => {
                self.diagnostics.push(CatalogDiagnostic::new(
                    self.anchor(token.range()),
                    CatalogDiagnosticKind::UnknownCatalogKind { spelling },
                ));

                return None;
            }
        };

        Some(Anchored::new(kind, self.anchor(token.range())))
    }

    fn parse_scope(&mut self) -> Option<ParsedScope> {
        self.expect_word(SCOPE_WORD)?;
        let key = self.parse_stable_key()?;

        self.expect_word(AT_WORD)?;

        let location = self.parse_scope_location()?;

        self.expect_kind(SyntaxKind::OpenBraceToken, CatalogExpectation::OpenBrace)?;

        let mut entries = Vec::new();

        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFileToken) {
            match self.parse_entry() {
                Some(entry) => entries.push(entry),
                None => self.synchronize_to_entry(),
            }
        }

        self.expect_kind(SyntaxKind::CloseBraceToken, CatalogExpectation::CloseBrace)?;

        Some(ParsedScope {
            key,
            location,
            entries,
        })
    }

    fn parse_scope_location(&mut self) -> Option<Anchored<ParsedScopeLocation>> {
        let first = self.consume_identifier(CatalogExpectation::ScopeLocation)?;
        let start = first.start();
        let first_text = self.token_text(&first);

        if first_text.as_ref() == AMBIENT_WORD && !self.at(SyntaxKind::DotToken) {
            return Some(Anchored::new(
                ParsedScopeLocation::Ambient,
                self.anchor(first.range()),
            ));
        }

        let mut end = first.end();
        let mut segments = vec![first_text];

        while self.at(SyntaxKind::DotToken) {
            self.tokens.consume();
            let segment = self.consume_identifier(CatalogExpectation::Identifier)?;

            end = segment.end();

            segments.push(self.token_text(&segment));
        }

        Some(Anchored::new(
            ParsedScopeLocation::Path(segments),
            self.anchor(TextRange::new(start, end)),
        ))
    }

    fn parse_entry(&mut self) -> Option<ParsedEntry> {
        let token = self.peek();
        let spelling = self.token_text(&token);

        match spelling.as_ref() {
            DECLARATION_WORD => self.parse_declaration().map(ParsedEntry::Declaration),
            VALUE_WORD => self.parse_value().map(ParsedEntry::Value),
            _ => {
                if is_catalog_word(&token) {
                    self.diagnostics.push(CatalogDiagnostic::new(
                        self.anchor(token.range()),
                        CatalogDiagnosticKind::UnknownEntryKind { spelling },
                    ));
                } else {
                    self.record_unexpected(&token, CatalogExpectation::Entry);
                }

                None
            }
        }
    }

    fn parse_declaration(&mut self) -> Option<ParsedDeclaration> {
        self.expect_word(DECLARATION_WORD)?;
        let key = self.parse_stable_key()?;

        self.expect_kind(SyntaxKind::OpenBraceToken, CatalogExpectation::OpenBrace)?;

        let mut fields = Vec::new();

        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFileToken) {
            if let Some(field) = self.parse_declaration_field() {
                fields.push(field);
            }
        }

        self.expect_kind(SyntaxKind::CloseBraceToken, CatalogExpectation::CloseBrace)?;

        Some(ParsedDeclaration { key, fields })
    }

    fn parse_declaration_field(&mut self) -> Option<ParsedDeclarationField> {
        let field = self.peek();
        let spelling = self.token_text(&field);

        match spelling.as_ref() {
            OWNER_WORD => self
                .parse_identifier_field(OWNER_WORD)
                .map(ParsedDeclarationField::Owner),
            IDENTITY_WORD => self
                .parse_declaration_identity()
                .map(ParsedDeclarationField::Identity),
            AVAILABILITY_WORD => self
                .parse_identifier_field(AVAILABILITY_WORD)
                .map(ParsedDeclarationField::Availability),
            REPRESENTATION_WORD => self
                .parse_identifier_field(REPRESENTATION_WORD)
                .map(ParsedDeclarationField::Representation),
            IMPLEMENTATION_WORD => self
                .parse_identifier_field(IMPLEMENTATION_WORD)
                .map(ParsedDeclarationField::Implementation),
            OPERATION_WORD => self
                .parse_identifier_field(OPERATION_WORD)
                .map(ParsedDeclarationField::Operation),
            SURFACE_WORD => {
                self.expect_word(SURFACE_WORD)?;
                let anchor = self.parse_braced_fragment()?;
                let surface = CatalogDeclarationSurface(anchor);

                Some(ParsedDeclarationField::Surface(Anchored::new(
                    surface, anchor,
                )))
            }
            _ => {
                if is_catalog_word(&field) {
                    self.record_unknown_field(CatalogEntryKind::Declaration, &field, spelling);
                } else {
                    self.record_unexpected(&field, CatalogExpectation::Field);
                }

                self.tokens.consume();

                if field.kind() != SyntaxKind::SemicolonToken {
                    self.skip_unknown_field();
                }

                None
            }
        }
    }

    fn parse_declaration_identity(&mut self) -> Option<Anchored<ParsedDeclarationIdentity>> {
        self.expect_word(IDENTITY_WORD)?;
        let identity_kind = self.consume_identifier(CatalogExpectation::DeclarationIdentityKind)?;
        let spelling = self.token_text(&identity_kind);

        let identity = match spelling.as_ref() {
            NAME_WORD => {
                let name = self.consume_identifier(CatalogExpectation::DeclarationIdentityValue)?;

                Anchored::new(
                    ParsedDeclarationIdentity::Name(self.token_text(&name)),
                    self.anchor(name.range()),
                )
            }
            ORDINAL_WORD => {
                let ordinal = self.peek();
                let spelling = self.token_text(&ordinal);

                let Ok(value) = spelling.parse::<u32>() else {
                    self.diagnostics.push(CatalogDiagnostic::new(
                        self.anchor(ordinal.range()),
                        CatalogDiagnosticKind::InvalidDeclarationIdentityOrdinal { spelling },
                    ));
                    self.tokens.consume();

                    return None;
                };

                self.tokens.consume();

                Anchored::new(
                    ParsedDeclarationIdentity::Ordinal(value),
                    self.anchor(ordinal.range()),
                )
            }
            _ => {
                self.diagnostics.push(CatalogDiagnostic::new(
                    self.anchor(identity_kind.range()),
                    CatalogDiagnosticKind::UnknownDeclarationIdentityKind { spelling },
                ));

                return None;
            }
        };

        self.expect_kind(SyntaxKind::SemicolonToken, CatalogExpectation::Semicolon)?;

        Some(identity)
    }

    fn parse_value(&mut self) -> Option<ParsedValue> {
        self.expect_word(VALUE_WORD)?;
        let key = self.parse_stable_key()?;

        self.expect_kind(SyntaxKind::OpenBraceToken, CatalogExpectation::OpenBrace)?;

        let mut fields = Vec::new();

        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFileToken) {
            if let Some(field) = self.parse_value_field() {
                fields.push(field);
            }
        }

        self.expect_kind(SyntaxKind::CloseBraceToken, CatalogExpectation::CloseBrace)?;

        Some(ParsedValue { key, fields })
    }

    fn parse_value_field(&mut self) -> Option<ParsedValueField> {
        let field = self.peek();
        let spelling = self.token_text(&field);

        match spelling.as_ref() {
            SPELLING_WORD => {
                self.expect_word(SPELLING_WORD)?;

                let token = self.peek();

                if matches!(
                    token.kind(),
                    SyntaxKind::EndOfFileToken
                        | SyntaxKind::SemicolonToken
                        | SyntaxKind::OpenBraceToken
                        | SyntaxKind::CloseBraceToken
                ) {
                    self.record_unexpected(&token, CatalogExpectation::TokenSpelling);

                    return None;
                }

                let token = self.tokens.consume();
                let anchor = self.anchor(token.range());
                let spelling = CatalogTokenSpelling::new(self.token_text(&token));

                self.expect_kind(SyntaxKind::SemicolonToken, CatalogExpectation::Semicolon)?;

                Some(ParsedValueField::Spelling(Anchored::new(spelling, anchor)))
            }
            TYPE_WORD => {
                self.expect_word(TYPE_WORD)?;
                let anchor = self.parse_braced_fragment()?;
                let surface = CatalogTypeSurface(anchor);

                Some(ParsedValueField::Type(Anchored::new(surface, anchor)))
            }
            AVAILABILITY_WORD => self
                .parse_identifier_field(AVAILABILITY_WORD)
                .map(ParsedValueField::Availability),
            REPRESENTATION_WORD => self
                .parse_identifier_field(REPRESENTATION_WORD)
                .map(ParsedValueField::Representation),
            _ => {
                if is_catalog_word(&field) {
                    self.record_unknown_field(CatalogEntryKind::Value, &field, spelling);
                } else {
                    self.record_unexpected(&field, CatalogExpectation::Field);
                }

                self.tokens.consume();

                if field.kind() != SyntaxKind::SemicolonToken {
                    self.skip_unknown_field();
                }

                None
            }
        }
    }

    fn parse_identifier_field(&mut self, word: &'static str) -> Option<Anchored<Arc<str>>> {
        self.expect_word(word)?;

        let value = self.consume_identifier(CatalogExpectation::Identifier)?;
        let anchored = Anchored::new(self.token_text(&value), self.anchor(value.range()));

        self.expect_kind(SyntaxKind::SemicolonToken, CatalogExpectation::Semicolon)?;

        Some(anchored)
    }

    fn parse_braced_fragment(&mut self) -> Option<CatalogSourceAnchor> {
        let open = self.expect_kind(SyntaxKind::OpenBraceToken, CatalogExpectation::OpenBrace)?;
        let start = open.end();

        let mut depth = 1_u32;

        loop {
            let token = self.peek();

            match token.kind() {
                SyntaxKind::OpenBraceToken => {
                    depth += 1;
                    self.tokens.consume();
                }
                SyntaxKind::CloseBraceToken => {
                    depth -= 1;

                    if depth == 0 {
                        let close = self.tokens.consume();

                        return Some(self.anchor(TextRange::new(start, close.start())));
                    }

                    self.tokens.consume();
                }
                SyntaxKind::EndOfFileToken => {
                    self.record_unexpected(&token, CatalogExpectation::CloseBrace);

                    return None;
                }
                _ => {
                    self.tokens.consume();
                }
            }
        }
    }

    fn parse_stable_key(&mut self) -> Option<Anchored<Arc<str>>> {
        let token = self.consume_identifier(CatalogExpectation::StableKey)?;

        Some(Anchored::new(
            self.token_text(&token),
            self.anchor(token.range()),
        ))
    }

    fn expect_word(&mut self, word: &'static str) -> Option<SyntaxToken> {
        let token = self.peek();

        if self.token_text(&token).as_ref() == word {
            return Some(self.tokens.consume());
        }

        self.record_unexpected(&token, CatalogExpectation::Word(word));

        None
    }

    fn consume_identifier(&mut self, expected: CatalogExpectation) -> Option<SyntaxToken> {
        let token = self.peek();

        if token.kind() == SyntaxKind::IdentifierToken {
            return Some(self.tokens.consume());
        }

        self.record_unexpected(&token, expected);

        None
    }

    fn expect_kind(
        &mut self,
        kind: SyntaxKind,
        expected: CatalogExpectation,
    ) -> Option<SyntaxToken> {
        let token = self.peek();

        if token.kind() == kind {
            return Some(self.tokens.consume());
        }

        self.record_unexpected(&token, expected);

        None
    }

    fn skip_unknown_field(&mut self) {
        if self.at(SyntaxKind::OpenBraceToken) {
            let _ = self.parse_braced_fragment();
            return;
        }

        while !matches!(
            self.peek().kind(),
            SyntaxKind::SemicolonToken | SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken
        ) {
            self.tokens.consume();
        }

        if self.at(SyntaxKind::SemicolonToken) {
            self.tokens.consume();
        }
    }

    fn synchronize_to_scope(&mut self) {
        while !self.at(SyntaxKind::EndOfFileToken) && !self.at_word(SCOPE_WORD) {
            self.tokens.consume();
        }
    }

    fn synchronize_to_entry(&mut self) {
        while !matches!(
            self.peek().kind(),
            SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken
        ) && !self.at_word(DECLARATION_WORD)
            && !self.at_word(VALUE_WORD)
        {
            self.tokens.consume();
        }
    }

    fn record_unknown_field(
        &mut self,
        entry: CatalogEntryKind,
        token: &SyntaxToken,
        spelling: Arc<str>,
    ) {
        self.diagnostics.push(CatalogDiagnostic::new(
            self.anchor(token.range()),
            CatalogDiagnosticKind::UnknownField { entry, spelling },
        ));
    }

    fn record_unexpected(&mut self, token: &SyntaxToken, expected: CatalogExpectation) {
        let kind = if token.kind() == SyntaxKind::EndOfFileToken {
            CatalogDiagnosticKind::UnexpectedEndOfFile { expected }
        } else {
            CatalogDiagnosticKind::UnexpectedToken {
                expected,
                actual: token.kind(),
            }
        };

        self.diagnostics
            .push(CatalogDiagnostic::new(self.anchor(token.range()), kind));
    }

    fn record_lexical_diagnostics(&mut self) {
        for diagnostic in self.tokens.diagnostics() {
            let range = diagnostic
                .primary_span()
                .map_or(TextRange::EMPTY, |span| span.range());

            self.diagnostics.push(CatalogDiagnostic::new(
                self.anchor(range),
                CatalogDiagnosticKind::Lexical(diagnostic.kind()),
            ));
        }
    }

    fn peek(&mut self) -> SyntaxToken {
        self.tokens.peek()
    }

    fn at(&mut self, kind: SyntaxKind) -> bool {
        self.peek().kind() == kind
    }

    fn at_word(&mut self, word: &str) -> bool {
        let token = self.peek();
        self.token_text(&token).as_ref() == word
    }

    fn token_text(&self, token: &SyntaxToken) -> Arc<str> {
        match token.text(self.source.text()) {
            Some(text) => Arc::from(text),
            None => Arc::from(""),
        }
    }

    fn anchor(&self, range: TextRange) -> CatalogSourceAnchor {
        CatalogSourceAnchor {
            source: self.source.id(),
            range,
        }
    }
}

fn source_snapshot(source: CatalogSource) -> Result<SourceSnapshot, CatalogDiagnostic> {
    let source_id = SourceId::new(source.id().raw());
    let identity = SourceIdentity::new(source.id().raw());
    let origin = SourceOrigin::generated(source.relative_path());

    SourceSnapshot::new(
        source_id,
        identity,
        origin,
        SourceVersion::new(0),
        source.text(),
    )
    .map_err(|error| {
        CatalogDiagnostic::new(
            CatalogSourceAnchor {
                source: source.id(),
                range: TextRange::EMPTY,
            },
            CatalogDiagnosticKind::SourceTooLarge {
                bytes: error.bytes(),
            },
        )
    })
}

fn is_catalog_word(token: &SyntaxToken) -> bool {
    token.kind() == SyntaxKind::IdentifierToken || token.kind().is_keyword()
}

#[cfg(test)]
mod tests {
    use super::parse_catalog_source;
    use crate::catalog::diagnostic::{CatalogDiagnosticKind, CatalogEntryKind};
    use crate::catalog::entry::{
        ParsedDeclarationField, ParsedEntry, ParsedScopeLocation, ParsedValueField,
    };
    use crate::catalog::{CatalogKind, CatalogSource, CatalogSourceId};

    #[test]
    fn parser_retains_catalog_metadata_and_exact_fragment_ranges() {
        let text = concat!(
            "catalog compiler_known;\n",
            "scope Ambient at ambient {\n",
            "  declaration RawPointerRead {\n",
            "    owner RawPointer;\n",
            "    availability RawMemory;\n",
            "    implementation RawPointerRead;\n",
            "    operation PlainConversion;\n",
            "    surface { trusted func read() -> u8; }\n",
            "  }\n",
            "  value True {\n",
            "    spelling true;\n",
            "    type { bool }\n",
            "    representation BooleanTrue;\n",
            "  }\n",
            "}\n",
        );

        let (parsed, diagnostics) = parse_catalog_source(source(text));

        assert!(diagnostics.is_empty());

        let parsed = match parsed {
            Some(parsed) => parsed,
            None => panic!("test catalog should parse"),
        };

        assert_eq!(parsed.declared_kind.value, CatalogKind::CompilerKnown);
        assert_eq!(parsed.scopes.len(), 1);
        assert_eq!(parsed.scopes[0].key.value.as_ref(), "Ambient");

        assert_eq!(
            parsed.scopes[0].location.value,
            ParsedScopeLocation::Ambient
        );

        assert_eq!(parsed.scopes[0].entries.len(), 2);

        let ParsedEntry::Declaration(declaration) = &parsed.scopes[0].entries[0] else {
            panic!("first entry should be a declaration");
        };

        let surface = declaration.fields.iter().find_map(|field| match field {
            ParsedDeclarationField::Surface(surface) => Some(surface.value),
            _ => None,
        });

        let operation = declaration.fields.iter().find_map(|field| match field {
            ParsedDeclarationField::Operation(operation) => Some(operation.value.as_ref()),
            _ => None,
        });

        assert_eq!(
            surface_text(text, surface),
            Some(" trusted func read() -> u8; ")
        );

        assert_eq!(operation, Some("PlainConversion"));

        let ParsedEntry::Value(value) = &parsed.scopes[0].entries[1] else {
            panic!("second entry should be a value");
        };

        let type_surface = value.fields.iter().find_map(|field| match field {
            ParsedValueField::Type(surface) => Some(surface.value),
            _ => None,
        });

        assert_eq!(
            type_surface.and_then(|surface| surface.anchor().range().slice_str(text)),
            Some(" bool ")
        );
    }

    #[test]
    fn parser_handles_nested_braces_inside_declaration_fragments() {
        let text = concat!(
            "catalog compiler_known;\n",
            "scope Ambient at ambient {\n",
            "  declaration Bool { surface { struct bool {} } }\n",
            "}\n",
        );

        let (parsed, diagnostics) = parse_catalog_source(source(text));

        assert!(diagnostics.is_empty());

        let parsed = match parsed {
            Some(parsed) => parsed,
            None => panic!("test catalog should parse"),
        };

        let ParsedEntry::Declaration(declaration) = &parsed.scopes[0].entries[0] else {
            panic!("entry should be a declaration");
        };

        let surface = declaration.fields.iter().find_map(|field| match field {
            ParsedDeclarationField::Surface(surface) => Some(surface.value),
            _ => None,
        });

        assert_eq!(surface_text(text, surface), Some(" struct bool {} "));
    }

    #[test]
    fn parser_reports_unknown_fields_without_publishing_a_partial_source() {
        let text = concat!(
            "catalog compiler_known;\n",
            "scope Ambient at ambient {\n",
            "  declaration Bool { mystery ScalarBool; surface { struct bool {} } }\n",
            "}\n",
        );

        let (parsed, diagnostics) = parse_catalog_source(source(text));

        assert_eq!(parsed, None);

        assert!(diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind(),
                CatalogDiagnosticKind::UnknownField {
                    entry: CatalogEntryKind::Declaration,
                    spelling,
                } if spelling.as_ref() == "mystery"
            )
        }));
    }

    #[test]
    fn parser_rejects_out_of_range_declaration_identity_ordinals() {
        let text = concat!(
            "catalog recognized_standard_library;\n",
            "scope Standard at std {\n",
            "  declaration Item {\n",
            "    identity ordinal 4294967296;\n",
            "    surface { func item(); }\n",
            "  }\n",
            "}\n",
        );

        let (parsed, diagnostics) = parse_catalog_source(source(text));

        assert_eq!(parsed, None);

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.kind(),
            CatalogDiagnosticKind::InvalidDeclarationIdentityOrdinal { spelling }
                if spelling.as_ref() == "4294967296"
        )));
    }

    #[test]
    fn parser_retains_unknown_entry_kind_spellings() {
        let text = concat!(
            "catalog compiler_known;\n",
            "scope Ambient at ambient { unsupported Item {} }\n",
        );

        let (parsed, diagnostics) = parse_catalog_source(source(text));

        assert_eq!(parsed, None);

        assert!(diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind(),
                CatalogDiagnosticKind::UnknownEntryKind { spelling }
                    if spelling.as_ref() == "unsupported"
            )
        }));
    }

    #[test]
    fn parser_reports_unclosed_fragments_at_end_of_file() {
        let text = concat!(
            "catalog compiler_known;\n",
            "scope Ambient at ambient {\n",
            "  declaration Bool { surface { struct bool {}\n",
        );

        let (parsed, diagnostics) = parse_catalog_source(source(text));

        assert_eq!(parsed, None);

        assert!(diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind(),
                CatalogDiagnosticKind::UnexpectedEndOfFile { .. }
            )
        }));
    }

    fn source(text: &'static str) -> CatalogSource {
        CatalogSource::new(
            CatalogSourceId::new(0),
            CatalogKind::CompilerKnown,
            "catalog/test.braydef",
            text,
        )
    }

    fn surface_text(
        text: &str,
        surface: Option<crate::catalog::CatalogDeclarationSurface>,
    ) -> Option<&str> {
        surface.and_then(|surface| surface.anchor().range().slice_str(text))
    }
}
