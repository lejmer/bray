#![no_main]

use bray_parser::{lex_source_unit, parse_compilation_unit};
use bray_source::SourceId;
use bray_syntax::SyntaxText;
use bray_testing::{
    assert_single_final_eof, assert_tokens_cover_source_text, try_test_source_store,
};

libfuzzer_sys::fuzz_target!(|bytes: &[u8]| {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return,
    };

    let sources = try_test_source_store([text])
        .expect("test source store should accept arbitrary UTF-8 source text");

    let snapshot = sources
        .get(SourceId::new(0))
        .expect("fuzz source store should contain inserted source");

    let lex_result = lex_source_unit(snapshot);

    assert_single_final_eof(lex_result.tokens(), snapshot.text());
    assert_tokens_cover_source_text(snapshot, lex_result.tokens());

    let parse_result = parse_compilation_unit(&sources);
    let syntax_tree = parse_result.syntax_tree();

    assert_eq!(syntax_tree.full_text(), snapshot.text());

    let source_units = syntax_tree.root().source_units();
    assert_eq!(source_units.len(), 1);

    let source_unit = source_units
        .first()
        .expect("parser skeleton should produce one source unit for one input");

    assert_eq!(source_unit.full_text(), snapshot.text());

    let source_unit_tokens = source_unit.tokens().cloned().collect::<Vec<_>>();
    assert_eq!(source_unit_tokens.as_slice(), lex_result.tokens());

    // Parser skeleton currently forwards lexer diagnostics. Once real parsing
    // exists, this should change to "parse diagnostics include lexer diagnostics".
    assert_eq!(parse_result.diagnostics(), lex_result.diagnostics());
});
