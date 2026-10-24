use std::collections::BTreeSet;

use super::workspace::RustWorkspace;

const GRAMMAR: &str = "docs/language/syntax-grammar.ebnf";
const COVERAGE: &str = "crates/bray-parser/tests/fixtures/parser-coverage.md";
const LEXICAL_TERMINALS: &[&str] = &[
    "identifier",
    "tuple-element-index",
    "integer-literal",
    "real-literal",
    "imaginary-literal",
    "character-literal",
    "string-literal",
    "byte-string-literal",
];

struct CoverageRow<'source> {
    name: &'source str,
    parser: &'source str,
    tests: &'source str,
}

pub(super) fn audit(workspace: &RustWorkspace) -> Result<(), String> {
    let grammar = workspace.read_text(GRAMMAR)?;
    let coverage = workspace.read_text(COVERAGE)?;

    let tests = workspace
        .parser_tests()
        .iter()
        .map(|test| test.qualified_name())
        .collect::<Vec<_>>();

    validate(&grammar, &coverage, workspace.parser_functions(), &tests)
}

fn validate(
    grammar: &str,
    coverage: &str,
    functions: &[String],
    tests: &[&str],
) -> Result<(), String> {
    let nonterminals = grammar
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.trim()))
        .filter(|name| is_nonterminal_name(name))
        .collect::<Vec<_>>();

    if nonterminals.is_empty() {
        return Err("parser grammar contains no nonterminals".to_owned());
    }

    let rows = coverage_rows(coverage)?;
    let expected = nonterminals.iter().copied().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut actual = Vec::new();

    for row in rows {
        if !seen.insert(row.name) {
            return Err(format!("parser coverage repeats {}", row.name));
        }

        if expected.contains(row.name) {
            actual.push(row.name);
        } else if !LEXICAL_TERMINALS.contains(&row.name) {
            return Err(format!("unknown parser coverage row: {}", row.name));
        }

        require_anchors(
            row.name,
            "production",
            row.parser,
            functions.iter().map(String::as_str),
        )?;

        require_anchors(
            row.name,
            "executable test",
            row.tests,
            tests.iter().copied(),
        )?;
    }

    if actual != nonterminals {
        return Err(format!(
            "parser coverage must contain every grammar nonterminal once in EBNF order: expected {nonterminals:?}, found {actual:?}"
        ));
    }

    Ok(())
}

fn coverage_rows(coverage: &str) -> Result<Vec<CoverageRow<'_>>, String> {
    let mut rows = Vec::new();

    for line in coverage.lines() {
        let trimmed = line.trim();

        if !trimmed.starts_with('|') {
            continue;
        }

        let cells = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();

        if !cells.first().is_some_and(|cell| cell.starts_with('`')) {
            continue;
        }

        let [name, parser, tests] = cells.as_slice() else {
            return Err(format!("malformed parser coverage row: {line}"));
        };

        let name = name
            .strip_prefix('`')
            .and_then(|name| name.strip_suffix('`'))
            .filter(|name| is_nonterminal_name(name))
            .ok_or_else(|| format!("invalid parser coverage name: {name}"))?;

        rows.push(CoverageRow {
            name,
            parser,
            tests,
        });
    }

    Ok(rows)
}

fn is_nonterminal_name(name: &str) -> bool {
    name.starts_with(|character: char| character.is_ascii_lowercase())
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn require_anchors<'source>(
    row: &str,
    category: &str,
    cell: &str,
    candidates: impl Iterator<Item = &'source str> + Clone,
) -> Result<(), String> {
    let fragments = cell.split('`').collect::<Vec<_>>();

    if fragments.len() < 3 || fragments.len() % 2 == 0 {
        return Err(format!(
            "parser coverage {row} needs backticked {category} anchors: {cell}"
        ));
    }

    for anchor in fragments.iter().skip(1).step_by(2) {
        let matches = candidates
            .clone()
            .filter(|candidate| {
                *candidate == *anchor
                    || candidate
                        .strip_suffix(*anchor)
                        .is_some_and(|prefix| prefix.ends_with("::"))
            })
            .collect::<Vec<_>>();

        match matches.as_slice() {
            [_] => {}
            [] => {
                return Err(format!(
                    "parser coverage {row} has no {category} anchor: {anchor}"
                ));
            }
            _ => {
                return Err(format!(
                    "parser coverage {row} has ambiguous {category} anchor {anchor}: {matches:?}"
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::workspace::parsed_parser_anchors;
    use super::validate;

    const GRAMMAR: &str = "source-unit = module ;\nmodule = identifier ;\n";
    const COVERAGE: &str = "\
| `source-unit` | `Parser::source` dispatch | `parser::tests::source`, `module_case` |\n\
| `module` | `Parser::module` token set | `module_case` |\n\
| `identifier` | `identifier` | `source` |\n";

    fn check(coverage: &str) -> Result<(), String> {
        validate(
            GRAMMAR,
            coverage,
            &[
                "parser::Parser::source".to_owned(),
                "parser::Parser::module".to_owned(),
                "cursor::identifier".to_owned(),
            ],
            &["parser::tests::source", "parser::tests::module_case"],
        )
    }

    #[test]
    fn grammar_order_lexical_extras_and_grouped_owned_anchors_are_preserved() {
        check(COVERAGE).expect("complete parser coverage");
    }

    #[test]
    fn renamed_deleted_and_non_test_source_declarations_break_their_anchors() {
        let source = r#"
            struct Parser;

            impl Parser {
                fn parse_source() {}
            }

            #[cfg(test)]
            mod tests {
                #[test]
                fn source_case() {}
            }
        "#;

        let coverage =
            "| `source` | `Parser::parse_source` dispatch | `parser::state::tests::source_case` |";

        for (changed, valid) in [
            (source.to_owned(), true),
            (source.replace("fn parse_source", "fn renamed"), false),
            (source.replace("fn parse_source() {}", ""), false),
            (source.replace("fn source_case", "fn renamed_case"), false),
            (source.replace("#[test]", ""), false),
            (source.replace("mod tests", "mod different_owner"), false),
        ] {
            let (functions, tests) = parsed_parser_anchors(&changed);

            let tests = tests.iter().map(String::as_str).collect::<Vec<_>>();
            let result = validate("source = identifier ;", coverage, &functions, &tests);

            assert_eq!(result.is_ok(), valid, "{result:?}: {changed}");
        }
    }

    #[test]
    fn missing_duplicate_unknown_and_reordered_grammar_rows_fail() {
        for coverage in [
            COVERAGE.lines().skip(1).collect::<Vec<_>>().join("\n"),
            format!(
                "{COVERAGE}{}",
                COVERAGE.lines().next().expect("first coverage row")
            ),
            COVERAGE.replace("`identifier` |", "`unknown-terminal` |"),
            COVERAGE.lines().rev().collect::<Vec<_>>().join("\n"),
        ] {
            check(&coverage).expect_err("invalid grammar coverage");
        }
    }

    #[test]
    fn stale_production_and_executable_test_anchors_identify_the_row() {
        for (old, new, category) in [
            ("Parser::source", "Parser::deleted", "production"),
            (
                "parser::tests::source",
                "other::tests::source",
                "executable test",
            ),
            (
                "module_case",
                "helper_without_test_attribute",
                "executable test",
            ),
        ] {
            let error = check(&COVERAGE.replace(old, new)).expect_err("stale anchor");

            assert!(error.contains("source-unit"), "{error}");
            assert!(error.contains(category), "{error}");
            assert!(error.contains(new), "{error}");
        }
    }

    #[test]
    fn unqualified_ambiguous_tests_need_their_module_owner() {
        let tests = [
            "parser::tests::source",
            "other::tests::source",
            "module_case",
        ];

        let functions = [
            "Parser::source".to_owned(),
            "Parser::module".to_owned(),
            "identifier".to_owned(),
        ];

        let error = validate(GRAMMAR, COVERAGE, &functions, &tests)
            .expect_err("ambiguous lexical source test");

        assert!(error.contains("identifier"), "{error}");

        assert!(
            error.contains("ambiguous executable test anchor source"),
            "{error}"
        );

        validate(
            GRAMMAR,
            &COVERAGE.replace("| `source` |", "| `parser::tests::source` |"),
            &functions,
            &tests,
        )
        .expect("qualified module owner disambiguates the case");
    }

    #[test]
    fn ambiguous_production_functions_need_their_module_owner() {
        let functions = [
            "parser::Parser::source".to_owned(),
            "other::Parser::source".to_owned(),
            "Parser::module".to_owned(),
            "identifier".to_owned(),
        ];

        let tests = ["parser::tests::source", "module_case"];

        let error = validate(GRAMMAR, COVERAGE, &functions, &tests)
            .expect_err("ambiguous production owner");

        assert!(error.contains("source-unit"), "{error}");

        assert!(
            error.contains("ambiguous production anchor Parser::source"),
            "{error}"
        );

        validate(
            GRAMMAR,
            &COVERAGE.replace("`Parser::source`", "`parser::Parser::source`"),
            &functions,
            &tests,
        )
        .expect("qualified production owner");
    }

    #[test]
    fn prose_placeholders_malformed_rows_and_unclosed_anchors_fail() {
        for coverage in [
            COVERAGE.replace("`Parser::source` dispatch", "dispatch"),
            COVERAGE.replace("`module_case`", "TODO"),
            COVERAGE.replace("`Parser::source`", "`Parser::source"),
            COVERAGE.replace("| `source-unit` |", "| `source-unit` | extra |"),
        ] {
            check(&coverage).expect_err("invalid coverage cell");
        }
    }
}
