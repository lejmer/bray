use serde::Deserialize;

use super::workspace::{RustWorkspace, require_executable_source_contracts, require_ordered_names};

const REQUIRED_CONTRACTS: &[&str] = &[
    "equivalent immutable plans",
    "reversed parallel contributions",
    "sink collisions",
    "hostile output paths",
    "transactional stream failures",
    "filesystem failures",
    "atomic staging cleanup",
    "digest mismatches",
    "reused pure inputs",
    "planning cancellation",
    "codegen cancellation",
    "staging cancellation",
    "link cancellation",
    "publication cancellation",
    "external process cancellation",
    "bounded compiler workers",
    "bounded external processes",
    "deterministic merged diagnostics",
    "narrow artifact requests",
    "missing runtime capabilities",
    "incompatible runtime ABI",
    "root runtime artifact matching",
    "structured shutdown ordering",
    "source run cancellation in MIR",
];

const FORBIDDEN_DEPENDENCIES: &[&str] = &[
    "bray-source",
    "bray-syntax",
    "bray-parser",
    "bray-declarations",
    "bray-binder",
    "bray-bound-tree",
    "bray-checker",
    "bray-lowering",
    "bray-ir",
    "bray-compilation",
    "bray-driver",
];

#[derive(Deserialize)]
struct CoverageFixture {
    contracts: Vec<CoverageRow>,
}

#[derive(Deserialize)]
struct CoverageRow {
    name: String,
    production: String,
    test: String,
}

pub(super) fn audit(workspace: &RustWorkspace) -> Result<(), String> {
    let fixture: CoverageFixture = workspace.read_fixture("emission-coverage.json")?;

    require_ordered_names(
        fixture.contracts.iter().map(|row| row.name.as_str()),
        REQUIRED_CONTRACTS,
        "emission readiness contracts",
    )?;

    require_executable_source_contracts(
        fixture.contracts.iter().map(|row| {
            (
                row.name.as_str(),
                row.production.as_str(),
                row.test.as_str(),
            )
        }),
        workspace,
        "emission",
    )?;

    require_emitter_boundary(workspace)
}

fn require_emitter_boundary(workspace: &RustWorkspace) -> Result<(), String> {
    let manifest = workspace.read_text("crates/bray-emitter/Cargo.toml")?;

    for forbidden in FORBIDDEN_DEPENDENCIES {
        if manifest.contains(forbidden) {
            return Err(format!("bray-emitter must not depend on {forbidden}"));
        }
    }

    Ok(())
}
