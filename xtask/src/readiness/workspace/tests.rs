use std::collections::BTreeSet;

use syn::visit::Visit;

use super::RustWorkspace;
use super::source::{SourceCollector, asserted_diagnostic_kinds, source_module_path};

#[test]
fn parser_anchors_keep_module_owners_and_ignore_non_executable_mentions() {
    let (functions, tests) = parsed_parser_anchors(
        r#"
        const MENTION: &str = "fn invented() {}";
        const INNER: () = {
            #[test]
            fn not_a_harness_entry() {}
        };

        struct Parser;
        struct Other;

        impl Parser {
            fn source() {}
        }

        impl Other {
            fn source() {}
        }

        fn helper() {}

        #[cfg(test)]
        fn fixture_only() {}

        #[cfg(test)]
        mod tests {
            fn helper_without_test() {}

            #[test]
            fn real_case() {}

            mod nested {
                #[test]
                fn real_case() {}
            }
        }
        "#,
    );

    assert_eq!(
        functions,
        ["parser::state::Parser::source", "parser::state::helper"]
    );

    assert_eq!(
        tests,
        [
            "parser::state::tests::real_case",
            "parser::state::tests::nested::real_case"
        ],
    );
}

pub(in crate::readiness) fn parsed_parser_anchors(source: &str) -> (Vec<String>, Vec<String>) {
    let file = syn::parse_file(source).expect("parser source fixture");
    let path = "crates/bray-parser/src/parser/state.rs";
    let mut tests = Vec::new();
    let mut functions = Vec::new();

    SourceCollector::new(path, source_module_path(path), &mut tests, &mut functions)
        .visit_file(&file);

    (
        functions,
        tests.into_iter().map(|test| test.qualified_name().to_owned()).collect(),
    )
}

pub(in crate::readiness) fn dependency_workspace(
    packages: &[(&str, &str)],
) -> (tempfile::TempDir, RustWorkspace) {
    let directory = tempfile::tempdir().expect("readiness dependency workspace");

    std::fs::write(
        directory.path().join("Cargo.toml"),
        r#"
        [workspace]
        members = ["crates/*"]
        resolver = "3"

        [workspace.dependencies]
        bray-ir = "1"
        mir = { package = "bray-ir", version = "1" }
    "#,
    )
    .expect("workspace manifest");

    for (name, dependencies) in packages {
        let package = directory.path().join("crates").join(name);

        std::fs::create_dir_all(package.join("src")).expect("package source directory");
        std::fs::write(package.join("src/lib.rs"), "").expect("package source");

        std::fs::write(
            package.join("Cargo.toml"),
            format!(
                r#"
            [package]
            name = "{name}"
            version = "0.0.0"
            edition = "2024"

            {dependencies}
        "#
            ),
        )
        .expect("package manifest");
    }

    let workspace = RustWorkspace::load(directory.path().to_owned())
        .expect("readiness workspace must load without dependency resolution");

    (directory, workspace)
}

#[test]
fn diagnostic_assertions_follow_exact_structured_calls() {
    assert_eq!(
        assertions(
            "fn test() { assert_goal_state_diagnostic_kind(&bag, DiagnosticKind::Exact); }"
        ),
        BTreeSet::from(["Exact".to_owned()]),
    );

    assert_eq!(
        assertions(
            r#"
            fn test() {
                assert_goal_state_diagnostic_kind(
                    produced.diagnostics(),
                    DiagnosticKind::Multiline,
                );
            }
            "#,
        ),
        BTreeSet::from(["Multiline".to_owned()]),
    );
}

#[test]
fn diagnostic_assertions_reject_mentions_wrong_kinds_and_fabricated_records() {
    assert!(assertions(
        r#"fn test() { let mention = "assert_goal_state_diagnostic_kind(DiagnosticKind::Mention)"; }"#
    )
    .is_empty());

    assert_eq!(
        assertions(
            "fn test() { assert_goal_state_diagnostic_kind(&bag, DiagnosticKind::Actual); }"
        ),
        BTreeSet::from(["Actual".to_owned()]),
    );

    assert!(
        assertions(
            r#"
        fn test() {
            let bag = DiagnosticBag::single(Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::Fabricated,
                SeverityKind::Error,
            ));
            assert_goal_state_diagnostic_kind(&bag, DiagnosticKind::Fabricated);
        }
        "#,
        )
        .is_empty()
    );
}

fn assertions(source: &str) -> BTreeSet<String> {
    let function = syn::parse_str::<syn::ItemFn>(source)
        .unwrap_or_else(|error| panic!("test function must parse: {error}"));

    asserted_diagnostic_kinds(&function.block)
}

#[test]
fn parser_anchors_follow_cargo_targets_module_inclusion_and_logical_names() {
    for (declaration, manifest, production, test, valid) in [
        ("", "", "parse_root", "orphan::source_case", false),
        ("mod orphan;", "", "orphan::parse_source", "orphan::source_case", true),
        ("", "", "orphan::parse_source", "root_case", false),
        ("#[path = \"orphan.rs\"] mod renamed;", "", "parse_root", "orphan::source_case", false),
        ("#[path = \"orphan.rs\"] mod renamed;", "", "renamed::parse_source", "renamed::source_case", true),
        ("mod nested;", "", "parse_root", "nested::child::source_case", true),
        ("mod separate;", "", "parse_root", "separate::child::source_case", true),
        ("mod inline { mod child; }", "", "parse_root", "inline::child::source_case", true),
        ("#[cfg(not(test))] mod orphan;", "", "parse_root", "orphan::source_case", false),
        ("#[cfg(any())] mod orphan;", "", "orphan::parse_source", "root_case", false),
        ("mod orphan;", "[lib]\ntest = false", "parse_root", "orphan::source_case", false),
        ("", "[[test]]\nname = \"literal_cases\"\npath = \"tests/different_filename.rs\"", "parse_root", "literal_cases::source_case", true),
        ("", "[[test]]\nname = \"literal_cases\"\npath = \"tests/different_filename.rs\"", "parse_root", "different_filename::source_case", false),
        ("", "", "parse_root", "different_filename::source_case", false),
    ] {
        let directory = tempfile::tempdir().expect("parser module workspace");
        let root = directory.path();

        let sources = [
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/bray-parser\"]\nresolver = \"3\"".to_owned()),
            ("crates/bray-parser/Cargo.toml", format!("[package]\nname = \"bray-parser\"\nversion = \"0.0.0\"\nedition = \"2024\"\nautotests = false\n{manifest}")),
            ("crates/bray-parser/src/lib.rs", format!("pub fn parse_root() {{}}\n#[test] fn root_case() {{}}\n{declaration}")),
            ("crates/bray-parser/src/orphan.rs", "pub fn parse_source() {}\n#[test] fn source_case() {}".to_owned()),
            ("crates/bray-parser/src/nested/mod.rs", "mod child;".to_owned()),
            ("crates/bray-parser/src/nested/child.rs", "#[test] fn source_case() {}".to_owned()),
            ("crates/bray-parser/src/separate.rs", "mod child;".to_owned()),
            ("crates/bray-parser/src/separate/child.rs", "#[test] fn source_case() {}".to_owned()),
            ("crates/bray-parser/src/inline/child.rs", "#[test] fn source_case() {}".to_owned()),
            ("crates/bray-parser/tests/different_filename.rs", "#[test] fn source_case() {}".to_owned()),
            ("docs/language/syntax-grammar.ebnf", "source = identifier ;".to_owned()),
            ("crates/bray-parser/tests/fixtures/parser-coverage.md", format!("| `source` | `{production}` | `{test}` |")),
        ];

        for (path, source) in sources {
            let path = root.join(path);

            std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
            std::fs::write(path, source).expect("module fixture source");
        }

        let workspace = RustWorkspace::load(root.to_owned()).expect("module fixture corpus");
        let result = super::super::parser::audit(&workspace);

        assert_eq!(result.is_ok(), valid, "{declaration}: {manifest}: {production}, {test}: {result:?}");

        if let Err(error) = result {
            assert!(error.contains("source"), "{error}");
            assert!(error.contains(production) || error.contains(test), "{error}");
        }
    }
}
