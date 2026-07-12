use bray_symbols::SymbolName;
use bray_syntax::SyntaxToken;

pub(super) fn symbol_name(
    source: &bray_source::SourceSnapshot,
    token: &SyntaxToken,
) -> Option<SymbolName> {
    if token.is_missing() {
        return None;
    }

    SymbolName::try_new(token.text(source.text())?)
}
