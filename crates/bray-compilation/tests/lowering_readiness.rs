use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

#[path = "support/readiness.rs"]
mod support;
#[path = "support/syntax.rs"]
mod syntax_support;

use support::{rust_tests, rust_workspace, workspace_root};
use syntax_support::EnumInventory;

#[derive(Deserialize)]
struct CoverageFixture {
    bound_expressions: Vec<CoverageRow>,
    structured_expressions: Vec<CoverageRow>,
    patterns: Vec<CoverageRow>,
    unit_roots: Vec<CoverageRow>,
    semantic_facts: Vec<CoverageRow>,
    contracts: Vec<CoverageRow>,
}

#[derive(Deserialize)]
struct CoverageRow {
    name: String,
    production: String,
    test: String,
}

#[test]
fn lowering_coverage_fixture_matches_the_complete_executable_contract() {
    let root = workspace_root();
    let fixture = coverage_fixture(&root);
    let rust = rust_workspace(&root);
    let enums = EnumInventory::new(&rust);

    assert_enum_coverage(
        &fixture.bound_expressions,
        &rust,
        &enums,
        "BoundExpression",
    );

    assert_enum_coverage(
        &fixture.structured_expressions,
        &rust,
        &enums,
        "BoundStructuredExpressionKind",
    );

    assert_enum_coverage(&fixture.patterns, &rust, &enums, "BoundPatternKind");
    assert_enum_coverage(&fixture.unit_roots, &rust, &enums, "BoundUnitRoot");
    assert_enum_coverage(&fixture.semantic_facts, &rust, &enums, "LoweringFactKind");
    assert_rows_are_executable(&fixture.contracts, &rust);
}

#[test]
fn codegen_consumes_mir_without_semantic_phase_dependencies() {
    let root = workspace_root();
    let manifest_path = root.join("crates/bray-codegen/Cargo.toml");

    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", manifest_path.display()));

    for forbidden in [
        "bray-binder",
        "bray-bound-tree",
        "bray-checker",
        "bray-compilation",
        "bray-lowering",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "bray-codegen must consume MIR without depending on {forbidden}"
        );
    }

    assert!(manifest.contains("bray-ir"));
}

fn coverage_fixture(root: &Path) -> CoverageFixture {
    let path = root.join("crates/bray-compilation/tests/fixtures/lowering-coverage.json");

    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));

    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("could not decode {}: {error}", path.display()))
}

fn assert_enum_coverage(
    rows: &[CoverageRow],
    rust: &BTreeMap<String, String>,
    enums: &EnumInventory,
    enum_name: &str,
) {
    let expected = enums
        .variants(enum_name)
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let actual = rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        actual, expected,
        "lowering coverage fixture drifted from {enum_name}"
    );

    assert_rows_are_executable(rows, rust);
}

fn assert_rows_are_executable(rows: &[CoverageRow], rust: &BTreeMap<String, String>) {
    let corpus = rust
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");

    let tests = rust_tests(rust)
        .into_iter()
        .map(|test| {
            assert!(
                !test.path().is_empty() && !test.body().is_empty(),
                "executable lowering test {} has no source body",
                test.name()
            );

            test.name().to_owned()
        })
        .collect::<BTreeSet<_>>();

    let mut names = BTreeSet::new();

    for row in rows {
        assert!(
            names.insert(row.name.as_str()),
            "lowering coverage fixture repeats {}",
            row.name
        );

        assert!(
            is_fixture_anchor(&row.production) && corpus.contains(&row.production),
            "missing lowering production anchor for {}: {}",
            row.name,
            row.production
        );

        assert!(
            is_fixture_anchor(&row.test) && tests.contains(&row.test),
            "missing executable lowering test for {}: {}",
            row.name,
            row.test
        );
    }
}

fn is_fixture_anchor(value: &str) -> bool {
    let value = value.trim();

    !value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "none" | "pending" | "tbd" | "todo"
        )
}
