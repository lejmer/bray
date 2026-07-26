use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const REQUIRED_RULE_FAMILIES: &[&str] = &[
    "name-resolution",
    "type-and-generic-binding",
    "callable-surfaces",
    "constant-semantics",
    "generic-constraints",
    "implementation-selection",
    "implementation-coherence",
    "calls-and-overloads",
    "member-index-and-slice-access",
    "conversions",
    "construction",
    "iteration-and-generators",
    "literal-and-expression-typing",
    "patterns-and-match-coverage",
    "control-flow-and-refinement",
    "storage-planning",
    "ownership-borrowing-and-moves",
    "liveness-and-lifecycle",
    "asynchronous-semantics",
    "dependencies-effects-and-trust",
    "callable-contracts",
    "interprocedural-behavior",
    "foreign-and-target-abi",
    "products-entrypoints-and-exports",
    "checked-publication-boundary",
];

const REQUIRED_CROSS_CUTTING_CONTRACTS: &[&str] = &[
    "selected-targets",
    "product-kinds",
    "narrow-demand",
    "demand-order",
    "worker-counts",
    "concurrent-requests",
    "recovery",
    "cancellation",
    "bounded-growth",
    "diagnostic-order",
];

#[derive(Deserialize)]
struct CoverageFixture {
    rule_families: Vec<RuleFamily>,
    cross_cutting: Vec<CrossCuttingContract>,
}

#[derive(Deserialize)]
struct RuleFamily {
    name: String,
    production: Vec<String>,
    diagnostic_source_tests: Vec<String>,
    valid_source_tests: Vec<String>,
    boundary_tests: Vec<String>,
    published: Vec<String>,
}

#[derive(Deserialize)]
struct CrossCuttingContract {
    name: String,
    production: Vec<String>,
    tests: Vec<String>,
}

#[test]
fn semantic_coverage_fixture_names_complete_executable_contracts() {
    let root = workspace_root();
    let fixture = coverage_fixture(&root);
    let rust = rust_workspace(&root);

    assert_required_names(
        fixture
            .rule_families
            .iter()
            .map(|family| family.name.as_str()),
        REQUIRED_RULE_FAMILIES,
    );

    assert_required_names(
        fixture
            .cross_cutting
            .iter()
            .map(|contract| contract.name.as_str()),
        REQUIRED_CROSS_CUTTING_CONTRACTS,
    );

    assert_rule_families_are_complete(&fixture.rule_families);
    assert_code_anchors_exist(&fixture, &rust);
    assert_test_anchors_are_executable(&fixture, &rust);
    assert_source_tests_use_source_integration_crates(&fixture.rule_families, &rust);
}

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let Some(root) = manifest_dir.parent().and_then(Path::parent) else {
        panic!("could not resolve workspace root from {manifest_dir:?}");
    };

    root.to_path_buf()
}

fn coverage_fixture(root: &Path) -> CoverageFixture {
    let path = root.join("crates/bray-compilation/tests/fixtures/semantic-coverage.json");

    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => panic!("could not read {}: {error}", path.display()),
    };

    match serde_json::from_str(&contents) {
        Ok(fixture) => fixture,
        Err(error) => panic!("could not decode {}: {error}", path.display()),
    }
}

fn rust_workspace(root: &Path) -> BTreeMap<String, String> {
    let mut files = Vec::new();

    collect_rust_files(&root.join("crates"), &mut files);

    files.sort();

    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .unwrap_or_else(|_| panic!("{} is outside the workspace", path.display()))
                .to_string_lossy()
                .replace('\\', "/");

            let contents = match std::fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(error) => panic!("could not read {}: {error}", path.display()),
            };

            (relative, contents)
        })
        .collect()
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => panic!("could not read {}: {error}", directory.display()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => panic!("could not read directory entry: {error}"),
        };

        let path = entry.path();

        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn assert_required_names<'name>(actual: impl IntoIterator<Item = &'name str>, required: &[&str]) {
    assert_eq!(
        actual.into_iter().collect::<Vec<_>>(),
        required,
        "semantic coverage fixture rows drifted"
    );
}

fn assert_rule_families_are_complete(families: &[RuleFamily]) {
    let incomplete = families
        .iter()
        .filter(|family| {
            [
                family.production.as_slice(),
                family.diagnostic_source_tests.as_slice(),
                family.valid_source_tests.as_slice(),
                family.boundary_tests.as_slice(),
                family.published.as_slice(),
            ]
            .into_iter()
            .any(cell_is_incomplete)
        })
        .map(|family| family.name.as_str())
        .collect::<Vec<_>>();

    assert!(
        incomplete.is_empty(),
        "semantic coverage rule families contain incomplete cells: {incomplete:?}"
    );
}

fn cell_is_incomplete(values: &[String]) -> bool {
    values.is_empty()
        || values.iter().any(|value| {
            value.trim().is_empty()
                || value.eq_ignore_ascii_case("todo")
                || value.eq_ignore_ascii_case("tbd")
        })
}

fn assert_code_anchors_exist(fixture: &CoverageFixture, rust: &BTreeMap<String, String>) {
    let corpus = rust
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");

    let mut missing = BTreeMap::<&str, Vec<&str>>::new();

    for family in &fixture.rule_families {
        for anchor in family.production.iter().chain(&family.published) {
            if !corpus.contains(anchor) {
                missing
                    .entry(family.name.as_str())
                    .or_default()
                    .push(anchor);
            }
        }
    }

    for contract in &fixture.cross_cutting {
        for anchor in &contract.production {
            if !corpus.contains(anchor) {
                missing
                    .entry(contract.name.as_str())
                    .or_default()
                    .push(anchor);
            }
        }
    }

    assert!(
        missing.is_empty(),
        "semantic coverage code anchors do not exist: {missing:?}"
    );
}

fn assert_test_anchors_are_executable(fixture: &CoverageFixture, rust: &BTreeMap<String, String>) {
    let tests = rust_test_locations(rust);
    let mut missing = BTreeMap::<&str, Vec<&str>>::new();

    for family in &fixture.rule_families {
        for test in family
            .diagnostic_source_tests
            .iter()
            .chain(&family.valid_source_tests)
            .chain(&family.boundary_tests)
        {
            if !tests.contains_key(test) {
                missing.entry(family.name.as_str()).or_default().push(test);
            }
        }
    }

    for contract in &fixture.cross_cutting {
        for test in &contract.tests {
            if !tests.contains_key(test) {
                missing
                    .entry(contract.name.as_str())
                    .or_default()
                    .push(test);
            }
        }
    }

    assert!(
        missing.is_empty(),
        "semantic coverage test anchors are not executable tests: {missing:?}"
    );
}

fn assert_source_tests_use_source_integration_crates(
    families: &[RuleFamily],
    rust: &BTreeMap<String, String>,
) {
    let tests = rust_test_locations(rust);
    let mut misplaced = BTreeMap::<&str, Vec<(&str, &str)>>::new();

    for family in families {
        for test in family
            .diagnostic_source_tests
            .iter()
            .chain(&family.valid_source_tests)
        {
            let Some(path) = tests.get(test) else {
                continue;
            };

            if !path.starts_with("crates/bray-binder/")
                && !path.starts_with("crates/bray-compilation/")
            {
                misplaced
                    .entry(family.name.as_str())
                    .or_default()
                    .push((test, path));
            }
        }
    }

    assert!(
        misplaced.is_empty(),
        "semantic source coverage anchors use lower-level fabricated fixtures: {misplaced:?}"
    );
}

fn rust_test_locations(rust: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut tests = BTreeMap::new();

    for (path, contents) in rust {
        let mut is_test = false;

        for line in contents.lines() {
            let trimmed = line.trim();

            if trimmed == "#[test]" {
                is_test = true;
                continue;
            }

            if !is_test {
                continue;
            }

            if trimmed.starts_with("#[") || trimmed.is_empty() {
                continue;
            }

            if let Some(after_fn) = trimmed.strip_prefix("fn ")
                && let Some((name, _)) = after_fn.split_once('(')
            {
                tests.insert(name.to_owned(), path.clone());
            }

            is_test = false;
        }
    }

    tests
}
