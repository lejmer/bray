use std::collections::BTreeMap;

use bray_diagnostics::DiagnosticKind;
use serde::Deserialize;

use super::workspace::{RustTest, RustWorkspace, require_ordered_names};

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

const REQUIRED_DIAGNOSTIC_SURFACE_TESTS: &[&str] = &[
    "json_output_preserves_related_locations_and_suggestions",
    "declaration_diagnostic_publishes_the_actual_related_origin",
    "parser_diagnostic_publishes_labels_and_safe_code_actions",
    "text_output_renders_related_locations_labels_and_suggestions",
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

pub(super) fn audit_coverage(workspace: &RustWorkspace) -> Result<(), String> {
    let fixture: CoverageFixture = workspace.read_fixture("semantic-coverage.json")?;

    require_ordered_names(
        fixture
            .rule_families
            .iter()
            .map(|family| family.name.as_str()),
        REQUIRED_RULE_FAMILIES,
        "semantic rule families",
    )?;

    require_ordered_names(
        fixture
            .cross_cutting
            .iter()
            .map(|contract| contract.name.as_str()),
        REQUIRED_CROSS_CUTTING_CONTRACTS,
        "semantic cross-cutting contracts",
    )?;

    require_complete_rule_families(&fixture.rule_families)?;
    require_code_anchors(&fixture, workspace)?;
    require_executable_test_anchors(&fixture, workspace)?;

    require_source_integration_tests(&fixture.rule_families, workspace)
}

pub(super) fn audit_diagnostics(workspace: &RustWorkspace) -> Result<(), String> {
    let mut coverage = BTreeMap::new();
    let mut candidates = BTreeMap::new();

    for &kind in DiagnosticKind::ALL {
        let variant = format!("{kind:?}");
        let reference = format!("DiagnosticKind :: {variant}");

        let kind_candidates = workspace
            .tests()
            .iter()
            .filter(|test| test.body().contains(&reference))
            .collect::<Vec<_>>();

        let producers = kind_candidates
            .iter()
            .copied()
            .filter(|test| test.asserted_diagnostic_kinds().contains(&variant))
            .map(test_location)
            .collect::<Vec<_>>();

        candidates.insert(
            kind,
            kind_candidates
                .into_iter()
                .map(test_location)
                .take(3)
                .collect::<Vec<_>>(),
        );

        coverage.insert(kind, producers);
    }

    let missing = coverage
        .iter()
        .filter(|(_, tests)| tests.is_empty())
        .map(|(kind, _)| format!("{kind:?} ({:?})", candidates[kind]))
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        return Err(format!(
            "{} diagnostic kinds lack quality-asserted executable producer tests: {missing:?}",
            missing.len(),
        ));
    }

    let tests = rust_test_locations(workspace);

    let missing_surfaces = REQUIRED_DIAGNOSTIC_SURFACE_TESTS
        .iter()
        .filter(|test| !tests.contains_key(**test))
        .copied()
        .collect::<Vec<_>>();

    if missing_surfaces.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "diagnostic rendered surfaces lack executable coverage: {missing_surfaces:?}"
        ))
    }
}

fn test_location(test: &RustTest) -> String {
    format!("{}::{}", test.path(), test.name())
}

fn require_complete_rule_families(families: &[RuleFamily]) -> Result<(), String> {
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

    if incomplete.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "semantic coverage rule families contain incomplete cells: {incomplete:?}"
        ))
    }
}

fn cell_is_incomplete(values: &[String]) -> bool {
    values.is_empty()
        || values.iter().any(|value| {
            value.trim().is_empty()
                || value.eq_ignore_ascii_case("todo")
                || value.eq_ignore_ascii_case("tbd")
        })
}

fn require_code_anchors(
    fixture: &CoverageFixture,
    workspace: &RustWorkspace,
) -> Result<(), String> {
    let mut missing = BTreeMap::<&str, Vec<&str>>::new();

    for family in &fixture.rule_families {
        for anchor in family.production.iter().chain(&family.published) {
            if !workspace.contains_source(anchor) {
                missing
                    .entry(family.name.as_str())
                    .or_default()
                    .push(anchor);
            }
        }
    }

    for contract in &fixture.cross_cutting {
        for anchor in &contract.production {
            if !workspace.contains_source(anchor) {
                missing
                    .entry(contract.name.as_str())
                    .or_default()
                    .push(anchor);
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "semantic coverage code anchors do not exist: {missing:?}"
        ))
    }
}

fn require_executable_test_anchors(
    fixture: &CoverageFixture,
    workspace: &RustWorkspace,
) -> Result<(), String> {
    let tests = rust_test_locations(workspace);
    let mut missing = BTreeMap::<&str, Vec<&str>>::new();

    for family in &fixture.rule_families {
        for test in family
            .diagnostic_source_tests
            .iter()
            .chain(&family.valid_source_tests)
            .chain(&family.boundary_tests)
        {
            if !tests.contains_key(test.as_str()) {
                missing.entry(family.name.as_str()).or_default().push(test);
            }
        }
    }

    for contract in &fixture.cross_cutting {
        for test in &contract.tests {
            if !tests.contains_key(test.as_str()) {
                missing
                    .entry(contract.name.as_str())
                    .or_default()
                    .push(test);
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "semantic coverage test anchors are not executable tests: {missing:?}"
        ))
    }
}

fn require_source_integration_tests(
    families: &[RuleFamily],
    workspace: &RustWorkspace,
) -> Result<(), String> {
    let tests = rust_test_locations(workspace);
    let mut misplaced = BTreeMap::<&str, Vec<(&str, &str)>>::new();

    for family in families {
        for test in family
            .diagnostic_source_tests
            .iter()
            .chain(&family.valid_source_tests)
        {
            let Some(path) = tests.get(test.as_str()) else {
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

    if misplaced.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "semantic source coverage anchors use lower-level fabricated fixtures: {misplaced:?}"
        ))
    }
}

fn rust_test_locations(workspace: &RustWorkspace) -> BTreeMap<&str, &str> {
    workspace
        .tests()
        .iter()
        .map(|test| (test.name(), test.path()))
        .collect()
}
