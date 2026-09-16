use std::collections::BTreeSet;

use serde::Deserialize;

use super::workspace::{
    RustWorkspace, require_executable_source_contracts, require_ordered_names, require_unique_names,
};

const REQUIRED_CONTRACTS: &[&str] = &[
    "mir-validation",
    "lazy-publication",
    "worker-determinism",
    "recovery",
];

const REQUIRED_SEMANTIC_INPUTS: &[&str] = &[
    "CheckerPublication",
    "BodyBehavior",
    "ConstantReferences",
    "ControlFlow",
    "ExpressionTypes",
    "LiteralValues",
    "Patterns",
    "Refinements",
];

#[derive(Deserialize)]
struct CoverageFixture {
    bound_expressions: Vec<CoverageRow>,
    structured_expressions: Vec<CoverageRow>,
    patterns: Vec<CoverageRow>,
    unit_roots: Vec<CoverageRow>,
    semantics: Vec<CoverageRow>,
    contracts: Vec<CoverageRow>,
}

#[derive(Deserialize)]
struct CoverageRow {
    name: String,
    production: String,
    test: String,
}

pub(super) fn audit(workspace: &RustWorkspace) -> Result<(), String> {
    let fixture: CoverageFixture = workspace.read_fixture("lowering-coverage.json")?;

    require_enum_coverage(&fixture.bound_expressions, workspace, "BoundExpression")?;

    require_enum_coverage(
        &fixture.structured_expressions,
        workspace,
        "BoundStructuredExpressionKind",
    )?;

    require_enum_coverage(&fixture.patterns, workspace, "BoundPatternKind")?;
    require_enum_coverage(&fixture.unit_roots, workspace, "BoundUnitRoot")?;
    require_semantic_input_coverage(&fixture.semantics, workspace)?;
    require_contract_names(&fixture.contracts)?;
    require_executable_rows(&fixture.contracts, workspace)?;

    require_codegen_boundary(workspace)
}

fn require_contract_names(rows: &[CoverageRow]) -> Result<(), String> {
    require_ordered_names(
        rows.iter().map(|row| row.name.as_str()),
        REQUIRED_CONTRACTS,
        "lowering readiness contracts",
    )
}

fn require_semantic_input_coverage(
    rows: &[CoverageRow],
    workspace: &RustWorkspace,
) -> Result<(), String> {
    require_unique_names(
        rows.iter().map(|row| row.name.as_str()),
        REQUIRED_SEMANTIC_INPUTS,
        "lowering semantic inputs",
    )?;

    require_executable_rows(rows, workspace)
}

fn require_enum_coverage(
    rows: &[CoverageRow],
    workspace: &RustWorkspace,
    enum_name: &str,
) -> Result<(), String> {
    let expected = workspace
        .enum_variants(enum_name)?
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let actual = rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<BTreeSet<_>>();

    if actual != expected {
        return Err(format!(
            "lowering coverage fixture drifted from {enum_name}: expected {expected:?}, found {actual:?}"
        ));
    }

    require_executable_rows(rows, workspace)
}

fn require_executable_rows(rows: &[CoverageRow], workspace: &RustWorkspace) -> Result<(), String> {
    require_executable_source_contracts(
        rows.iter().map(|row| {
            (
                row.name.as_str(),
                row.production.as_str(),
                row.test.as_str(),
            )
        }),
        workspace,
        "lowering",
    )
}

fn require_codegen_boundary(workspace: &RustWorkspace) -> Result<(), String> {
    let manifest = workspace.read_text("crates/bray-codegen/Cargo.toml")?;

    for forbidden in [
        "bray-binder",
        "bray-bound-tree",
        "bray-checker",
        "bray-compilation",
        "bray-lowering",
    ] {
        if manifest.contains(forbidden) {
            return Err(format!(
                "bray-codegen must consume MIR without depending on {forbidden}"
            ));
        }
    }

    if manifest.contains("bray-ir") {
        Ok(())
    } else {
        Err("bray-codegen must depend on bray-ir".to_owned())
    }
}
