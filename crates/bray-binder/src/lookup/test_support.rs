use bray_source::{TextRange, TextSize};
use bray_symbols::{ModuleOwnerId, ModuleSymbolId, SymbolOrigin};
use bray_syntax::{PathSyntax, SyntaxKind, SyntaxToken};
use bray_testing::{test_source_at, test_source_store};

use crate::BindingQueryContext;

pub(super) fn source_module<C: BindingQueryContext + ?Sized>(
    binding_context: &C,
) -> (ModuleSymbolId, ModuleOwnerId) {
    let Some(module) = binding_context
        .symbols()
        .modules()
        .iter()
        .find(|module| module.origin() == SymbolOrigin::Source)
    else {
        panic!("test graph must contain one source module");
    };

    (module.id(), module.owner())
}

pub(super) fn path(text: &str) -> PathSyntax {
    let sources = test_source_store([text]);
    let snapshot = test_source_at(&sources, 0).clone();

    let mut builder = PathSyntax::builder(snapshot);
    let mut segment_start = 0;

    for (index, byte) in text.bytes().enumerate() {
        if byte != b'.' {
            continue;
        }

        builder.push_identifier_token(SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            text_range(segment_start, index),
        ));

        builder.push_dot_token(SyntaxToken::new(
            SyntaxKind::DotToken,
            text_range(index, index + 1),
        ));

        segment_start = index + 1;
    }

    builder.push_identifier_token(SyntaxToken::new(
        SyntaxKind::IdentifierToken,
        text_range(segment_start, text.len()),
    ));

    builder.build()
}

pub(super) fn text_range(start: usize, end: usize) -> TextRange {
    let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) else {
        panic!("test path offsets must fit in TextSize");
    };

    TextRange::new(TextSize::new(start), TextSize::new(end))
}
