use bray_diagnostics::DiagnosticBag;
use bray_parser::{lex_source_unit, parse_compilation_unit};
use bray_source::SourceSnapshot;
use bray_syntax::SyntaxText;
use bray_testing::{
    assert_single_final_eof, assert_tokens_cover_source_text, test_source_snapshot,
    test_source_store,
};

#[test]
fn lexer_conformance_cases_consume_all_text_and_end_with_eof() {
    for text in conformance_sources() {
        let snapshot = test_source_snapshot(&text);
        let result = lex_source_unit(&snapshot);

        assert_single_final_eof(result.tokens(), snapshot.text());
        assert_tokens_cover_source_text(&snapshot, result.tokens());
    }
}

#[test]
fn token_draining_parser_preserves_lexer_output_and_reconstructs_sources() {
    let sources = test_source_store(conformance_sources());
    let expected_text = sources.iter().map(SourceSnapshot::text).collect::<String>();

    let lex_results = sources.iter().map(lex_source_unit).collect::<Vec<_>>();

    let expected_diagnostic_bags = lex_results
        .iter()
        .map(|result| result.diagnostics().clone())
        .collect::<Vec<_>>();

    let expected_diagnostics = DiagnosticBag::merged_all(&expected_diagnostic_bags);
    let result = parse_compilation_unit(&sources);
    let source_units = result.syntax_tree().root().source_units();

    assert_eq!(source_units.len(), sources.len());
    assert_eq!(result.diagnostics(), &expected_diagnostics);
    assert_eq!(result.syntax_tree().full_text(), expected_text);

    for ((snapshot, lex_result), source_unit) in sources.iter().zip(lex_results).zip(source_units) {
        let source_unit_tokens = source_unit.tokens().cloned().collect::<Vec<_>>();

        assert_eq!(source_unit_tokens, lex_result.tokens());
        assert_single_final_eof(&source_unit_tokens, snapshot.text());
        assert_eq!(source_unit.full_text(), snapshot.text());
    }
}

fn conformance_sources() -> Vec<String> {
    let mut sources = base_sources()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let fragments = generated_case_fragments();

    for index in 0..64 {
        let first = fragments[index % fragments.len()];
        let second = fragments[(index * 5 + 3) % fragments.len()];
        let third = fragments[(index * 11 + 7) % fragments.len()];

        sources.push(format!("{first}{second}{third}"));
    }

    sources
}

fn base_sources() -> [&'static str; 16] {
    [
        "",
        " \t\n",
        "module main\nfunc main() {}\n",
        "/// docs\n/** block docs */\nfunc main",
        "/* outer /* inner */ end */ value",
        "let value = 1_000\n0b1010 0xCAFE 1.25i\n",
        r#"'a' '\'' '\n' '\u{1F600}' "text\n\u{41}""#,
        "$",
        "é aé",
        "_ _bad __ _1",
        "=== ->> +- :: ...",
        "1u8 0b102 0x 1e+",
        r#"""bad\q" "\u{}" "\u{110000}""#,
        r#"'' 'ab' '\u{110000}'"#,
        "\"unterminated",
        "value /* unterminated",
    ]
}

fn generated_case_fragments() -> [&'static str; 28] {
    [
        "func",
        " main",
        "()",
        " {}",
        "\n",
        "\r\n",
        "\r",
        " ",
        "\t",
        "// comment\n",
        "/// doc\r\n",
        "/* block */",
        "/** doc block */",
        "identifier",
        "_",
        "123",
        "0xFF",
        "1.25",
        "1i",
        "\"string\"",
        "\"bad\\q\"",
        "'x'",
        "'xy'",
        "$",
        "é",
        "===",
        "/* open",
        "a\u{feff}",
    ]
}
