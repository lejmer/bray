use std::collections::BTreeSet;

use bray_compiler_known::ImplementationHook;
use serde::Deserialize;

use super::workspace::{RustWorkspace, is_fixture_anchor, require_unique_names};

const REQUIRED_CONTRACTS: &[&str] = &[
    "closed hook inventory",
    "checker classification",
    "lowering coverage",
    "code generation coverage",
    "unavailable target rejection",
    "invalid obligation rejection",
    "same-name isolation",
];

#[derive(Deserialize)]
struct CoverageFixture {
    operations: Vec<OperationRow>,
    contracts: Vec<ContractRow>,
}

#[derive(Deserialize)]
struct OperationRow {
    name: String,
    checker: SourceAnchor,
    lowering: SourceAnchor,
    codegen: SourceAnchor,
}

#[derive(Deserialize)]
struct ContractRow {
    name: String,
    test: String,
}

#[derive(Deserialize)]
struct SourceAnchor {
    path: String,
    symbol: String,
}

pub(super) fn audit(workspace: &RustWorkspace) -> Result<(), String> {
    let fixture: CoverageFixture = workspace.read_fixture("memory-operations.json")?;

    let expected = ImplementationHook::MEMORY_OPERATIONS
        .iter()
        .map(|hook| format!("{hook:?}"))
        .collect::<BTreeSet<_>>();

    let actual = fixture
        .operations
        .iter()
        .map(|row| row.name.clone())
        .collect::<BTreeSet<_>>();

    if actual != expected {
        return Err(format!(
            "memory operation coverage drifted: expected {expected:?}, found {actual:?}"
        ));
    }

    if fixture.operations.len() != actual.len() {
        return Err("memory operation coverage repeats an implementation hook".to_owned());
    }

    for operation in &fixture.operations {
        require_source_anchor(&operation.checker, workspace, &operation.name, "checker")?;
        require_source_anchor(&operation.lowering, workspace, &operation.name, "lowering")?;
        require_source_anchor(&operation.codegen, workspace, &operation.name, "code generation")?;
    }

    require_unique_names(
        fixture.contracts.iter().map(|row| row.name.as_str()),
        REQUIRED_CONTRACTS,
        "memory operation contract inventory",
    )?;

    let tests = workspace.executable_test_names("memory operation")?;

    for contract in &fixture.contracts {
        if !is_fixture_anchor(&contract.test) || !tests.contains(contract.test.as_str()) {
            return Err(format!(
                "missing executable memory operation test for {}: {}",
                contract.name, contract.test
            ));
        }
    }

    Ok(())
}

fn require_source_anchor(
    anchor: &SourceAnchor,
    workspace: &RustWorkspace,
    operation: &str,
    phase: &str,
) -> Result<(), String> {
    if !is_fixture_anchor(&anchor.path) || !is_fixture_anchor(&anchor.symbol) {
        return Err(format!("invalid {phase} anchor for {operation}"));
    }

    let Some(contents) = workspace.files().get(&anchor.path) else {
        return Err(format!(
            "missing {phase} source for {operation}: {}",
            anchor.path
        ));
    };

    if contents.contains(&anchor.symbol) {
        Ok(())
    } else {
        Err(format!(
            "missing {phase} implementation for {operation}: {} in {}",
            anchor.symbol, anchor.path
        ))
    }
}
