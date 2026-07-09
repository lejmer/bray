use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct CoverageRow {
    name: String,
    parser: String,
    tests: String,
}

#[test]
fn parser_coverage_matrix_matches_syntax_grammar_nonterminals() {
    let root = workspace_root();
    let grammar = read_workspace_file(&root, "docs/language/syntax-grammar.ebnf");
    let coverage = read_workspace_file(&root, "docs/design/parser-coverage.md");

    let grammar_nonterminals = grammar_nonterminals(&grammar);
    let rows = coverage_rows(&coverage);
    let allowed_extra_rows = lexical_terminal_rows();

    assert_matrix_has_no_unknown_rows(&rows, &grammar_nonterminals, &allowed_extra_rows);
    assert_matrix_has_no_duplicate_rows(&rows);
    assert_matrix_covers_grammar_in_order(&rows, &grammar_nonterminals);
    assert_matrix_rows_have_parser_and_test_anchors(&rows);
}

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let Some(root) = manifest_dir.parent().and_then(Path::parent) else {
        panic!("could not resolve workspace root from {manifest_dir:?}");
    };

    root.to_path_buf()
}

fn read_workspace_file(root: &Path, relative_path: &str) -> String {
    let path = root.join(relative_path);

    match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => panic!("could not read {}: {error}", path.display()),
    }
}

fn grammar_nonterminals(grammar: &str) -> Vec<String> {
    grammar
        .lines()
        .filter_map(grammar_nonterminal)
        .collect::<Vec<_>>()
}

fn grammar_nonterminal(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let (candidate, _) = trimmed.split_once('=')?;
    let candidate = candidate.trim();

    if !is_nonterminal_name(candidate) {
        return None;
    }

    Some(candidate.to_owned())
}

fn coverage_rows(coverage: &str) -> Vec<CoverageRow> {
    coverage
        .lines()
        .filter_map(coverage_row)
        .collect::<Vec<_>>()
}

fn coverage_row(line: &str) -> Option<CoverageRow> {
    let trimmed = line.trim();

    if !trimmed.starts_with('|') {
        return None;
    }

    let cells = trimmed
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect::<Vec<_>>();

    let [name, parser, tests] = cells.as_slice() else {
        return None;
    };

    if *name == "---" || *name == "Grammar nonterminal" || *name == "Lexical terminal" {
        return None;
    }

    let name = backticked_name(name)?;

    Some(CoverageRow {
        name,
        parser: (*parser).to_owned(),
        tests: (*tests).to_owned(),
    })
}

fn backticked_name(cell: &str) -> Option<String> {
    let without_prefix = cell.strip_prefix('`')?;
    let (name, _) = without_prefix.split_once('`')?;

    if !is_nonterminal_name(name) {
        return None;
    }

    Some(name.to_owned())
}

fn is_nonterminal_name(candidate: &str) -> bool {
    let mut chars = candidate.chars();

    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() {
        return false;
    }

    chars.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
}

fn lexical_terminal_rows() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "identifier",
        "tuple-element-index",
        "integer-literal",
        "real-literal",
        "imaginary-literal",
        "character-literal",
        "string-literal",
    ])
}

fn assert_matrix_has_no_unknown_rows(
    rows: &[CoverageRow],
    grammar_nonterminals: &[String],
    allowed_extra_rows: &BTreeSet<&str>,
) {
    let grammar_names = grammar_nonterminals
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let unknown_rows = rows
        .iter()
        .filter(|row| {
            !grammar_names.contains(row.name.as_str())
                && !allowed_extra_rows.contains(row.name.as_str())
        })
        .map(|row| row.name.as_str())
        .collect::<Vec<_>>();

    assert!(
        unknown_rows.is_empty(),
        "parser coverage matrix has rows that are not syntax grammar nonterminals or allowed lexical terminals: {unknown_rows:?}"
    );
}

fn assert_matrix_has_no_duplicate_rows(rows: &[CoverageRow]) {
    let mut counts = BTreeMap::<&str, usize>::new();

    for row in rows {
        *counts.entry(row.name.as_str()).or_default() += 1;
    }

    let duplicate_rows = counts
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then_some((name, count)))
        .collect::<Vec<_>>();

    assert!(
        duplicate_rows.is_empty(),
        "parser coverage matrix has duplicate rows: {duplicate_rows:?}"
    );
}

fn assert_matrix_covers_grammar_in_order(rows: &[CoverageRow], grammar_nonterminals: &[String]) {
    let grammar_names = grammar_nonterminals
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let matrix_grammar_rows = rows
        .iter()
        .filter(|row| grammar_names.contains(row.name.as_str()))
        .map(|row| row.name.as_str())
        .collect::<Vec<_>>();

    let expected = grammar_nonterminals
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    assert_eq!(
        matrix_grammar_rows, expected,
        "parser coverage matrix must list every grammar nonterminal exactly once and in EBNF order"
    );
}

fn assert_matrix_rows_have_parser_and_test_anchors(rows: &[CoverageRow]) {
    let incomplete_rows = rows
        .iter()
        .filter(|row| !cell_is_complete(&row.parser) || !cell_is_complete(&row.tests))
        .map(|row| row.name.as_str())
        .collect::<Vec<_>>();

    assert!(
        incomplete_rows.is_empty(),
        "parser coverage matrix rows need parser and test anchors: {incomplete_rows:?}"
    );
}

fn cell_is_complete(cell: &str) -> bool {
    let trimmed = cell.trim();

    !trimmed.is_empty()
        && trimmed != "-"
        && !trimmed.eq_ignore_ascii_case("todo")
        && !trimmed.eq_ignore_ascii_case("tbd")
}
