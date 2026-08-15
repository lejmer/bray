use std::collections::BTreeSet;

use bray_compilation::{
    Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::DiagnosticKind;
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::{PackageIdentity, ProductKind};
use bray_target::{TargetFacts, TargetOperationFacts, TargetProfile};
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
const UNAVAILABLE_TARGET_FIXTURE: &str = "xtask/fixtures/readiness/memory-unavailable-target.bray";
const INVALID_TARGET_CONTROL_FIXTURES: &[(&str, bray_target::NativeTarget)] = &[
    (
        "xtask/fixtures/readiness/target-control-invalid.bray",
        bray_target::NativeTarget::X86_64LinuxGnu,
    ),
    (
        "xtask/fixtures/readiness/target-control-invalid-constraint.bray",
        bray_target::NativeTarget::X86_64LinuxGnu,
    ),
    (
        "xtask/fixtures/readiness/target-control-invalid-symbol.bray",
        bray_target::NativeTarget::X86_64LinuxGnu,
    ),
    (
        "xtask/fixtures/readiness/target-control-invalid-dialect.bray",
        bray_target::NativeTarget::Aarch64LinuxGnu,
    ),
    (
        "xtask/fixtures/readiness/target-control-invalid-diverging-pure.bray",
        bray_target::NativeTarget::X86_64LinuxGnu,
    ),
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

        require_source_anchor(
            &operation.codegen,
            workspace,
            &operation.name,
            "code generation",
        )?;
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

    audit_unavailable_target(workspace)?;
    audit_invalid_target_control(workspace)?;

    Ok(())
}

fn audit_unavailable_target(workspace: &RustWorkspace) -> Result<(), String> {
    let baseline = SelectedTarget::baseline();
    let profile = baseline.profile();
    let baseline_facts = profile.facts();

    let facts = TargetFacts::new(
        baseline_facts.identity().clone(),
        baseline_facts.scalars(),
        baseline_facts.atomics(),
        baseline_facts.abis(),
        baseline_facts.c_abi(),
        baseline_facts.address_spaces(),
        baseline_facts.alignments(),
        TargetOperationFacts::new(false, false),
    );

    let profile =
        TargetProfile::try_new(profile.identity().clone(), profile.machine().clone(), facts)
            .map_err(|error| format!("could not build unavailable memory target: {error}"))?;

    if fixture_reports(
        workspace,
        UNAVAILABLE_TARGET_FIXTURE,
        SelectedTarget::new(profile, baseline.runtime_abi()),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    )? {
        Ok(())
    } else {
        Err("unavailable memory operation did not produce the structured rejection diagnostic"
            .to_owned())
    }
}

fn audit_invalid_target_control(workspace: &RustWorkspace) -> Result<(), String> {
    for (fixture, target) in INVALID_TARGET_CONTROL_FIXTURES {
        if !fixture_reports(
            workspace,
            fixture,
            SelectedTarget::for_native(*target),
            DiagnosticKind::CheckingInvalidTargetControlContract,
        )? {
            return Err(format!(
                "invalid target-control contract did not produce the structured rejection diagnostic: {fixture}"
            ));
        }
    }

    Ok(())
}

fn fixture_reports(
    workspace: &RustWorkspace,
    source_path: &str,
    target: SelectedTarget,
    expected: DiagnosticKind,
) -> Result<bool, String> {
    let source = workspace.read_text(source_path)?;

    let package = PackageIdentity::try_new("memory.readiness")
        .ok_or_else(|| "memory readiness package identity is invalid".to_owned())?;

    let input = SourceInput::virtual_text(
        SourceIdentity::new(0),
        source_path,
        SourceVersion::new(0),
        source,
    );

    let options = CompilationOptions::new(
        WorkerBudget::serial(),
        ProductKind::Executable,
        target,
    );

    let compilation = Compilation::load(CompilationRequest::with_options(
        package,
        vec![input],
        options,
    ))
    .map_err(|error| format!("could not load target-control fixture {source_path}: {error:?}"))?;

    Ok(compilation
        .check_diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.kind() == expected))
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
