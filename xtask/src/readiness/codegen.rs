use std::collections::BTreeSet;

use serde::Deserialize;

use super::workspace::{RustWorkspace, is_fixture_anchor, require_unique_names};

const REQUIRED_CONTRACTS: &[&str] = &[
    "requested-artifact-only generation",
    "verified artifact serialization",
    "repeat determinism",
    "parallel reversed-demand determinism",
    "typed unsupported artifacts",
    "translation cancellation",
    "serialization cancellation",
    "target capability coverage",
    "exact runtime role mappings",
    "protected frame descriptors",
    "inactive frame and task boundaries",
    "direct await frame dependency",
    "panic transfer outcome",
    "current run cancellation outcome",
    "async frame lowering",
    "main-thread executable host",
    "complete MIR cache identity",
    "cancelled compiler query",
    "single-flight compiler query",
    "composition-root backend selection",
];

const REQUIRED_CLOSED_CATEGORIES: &[&str] = &[
    "ConstructionDefaultProvider",
    "ConstructionInputId",
    "ConstructionTarget",
    "ConversionTarget",
    "MirAggregateKind",
    "MirBinaryOperator",
    "MirCallArgument",
    "MirCallTarget",
    "MirCleanupPhase",
    "MirConstructionInput",
    "MirFrameInitializer",
    "MirGeneratorKind",
    "MirGeneratorOperation",
    "MirImmediateValue",
    "MirOperand",
    "MirPanicCause",
    "MirProjectionKind",
    "MirStoreKind",
    "MirTaskTerminalState",
    "MirUnaryOperator",
    "PatternOperation",
    "PatternPredicate",
    "PatternProjection",
];

#[derive(Deserialize)]
struct CoverageFixture {
    unit_kinds: Vec<CoverageRow>,
    operations: Vec<CoverageRow>,
    async_operations: Vec<CoverageRow>,
    host_operations: Vec<CoverageRow>,
    terminators: Vec<CoverageRow>,
    closed_categories: Vec<ClosedCategory>,
    artifact_kinds: Vec<CoverageRow>,
    contracts: Vec<ContractRow>,
}

#[derive(Deserialize)]
struct CoverageRow {
    name: String,
    disposition: Disposition,
    production: SourceAnchor,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Disposition {
    Translated,
    Serialized,
    CapabilityFailure,
}

#[derive(Deserialize)]
struct ContractRow {
    name: String,
    production: SourceAnchor,
    test: String,
}

#[derive(Deserialize)]
struct ClosedCategory {
    name: String,
    variants: Vec<String>,
    production: SourceAnchor,
}

#[derive(Deserialize)]
struct SourceAnchor {
    path: String,
    symbol: String,
}

pub(super) fn audit(workspace: &RustWorkspace) -> Result<(), String> {
    let fixture: CoverageFixture = workspace.read_fixture("codegen-coverage.json")?;

    require_enum_coverage(&fixture.unit_kinds, workspace, "MirUnitKind")?;
    require_enum_coverage(&fixture.operations, workspace, "MirOperationKind")?;
    require_enum_coverage(&fixture.async_operations, workspace, "MirAsyncOperation")?;
    require_enum_coverage(&fixture.host_operations, workspace, "MirHostOperation")?;
    require_enum_coverage(&fixture.terminators, workspace, "MirTerminatorKind")?;
    require_enum_coverage(&fixture.artifact_kinds, workspace, "BackendArtifactKind")?;
    require_closed_categories(&fixture.closed_categories, workspace)?;
    require_disposition(&fixture.unit_kinds, Disposition::Translated)?;
    require_disposition(&fixture.operations, Disposition::Translated)?;
    require_disposition(&fixture.async_operations, Disposition::Translated)?;
    require_disposition(&fixture.host_operations, Disposition::Translated)?;
    require_disposition(&fixture.terminators, Disposition::Translated)?;
    require_artifact_dispositions(&fixture.artifact_kinds)?;
    require_executable_contracts(&fixture.contracts, workspace)?;

    require_backend_neutral_boundaries(workspace)
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
            "code generation coverage fixture drifted from {enum_name}: expected {expected:?}, found {actual:?}"
        ));
    }

    if rows.len() != actual.len() {
        return Err(format!(
            "code generation coverage fixture repeats a {enum_name} variant"
        ));
    }

    for row in rows {
        require_source_anchor(&row.production, workspace, &row.name)?;
    }

    Ok(())
}

fn require_disposition(rows: &[CoverageRow], expected: Disposition) -> Result<(), String> {
    for row in rows {
        if row.disposition != expected {
            return Err(format!(
                "{} has disposition {:?}, expected {expected:?}",
                row.name, row.disposition
            ));
        }
    }

    Ok(())
}

fn require_artifact_dispositions(rows: &[CoverageRow]) -> Result<(), String> {
    for row in rows {
        let expected = match row.name.as_str() {
            "RelocatableObject" | "Assembly" | "BackendIr" | "BackendBitcode" => {
                Disposition::Serialized
            }
            "ExecutableModule" | "DebugCompanion" => Disposition::CapabilityFailure,
            name => {
                return Err(format!(
                    "unknown backend artifact kind in coverage fixture: {name}"
                ));
            }
        };

        if row.disposition != expected {
            return Err(format!(
                "incorrect artifact disposition for {}: expected {expected:?}, found {:?}",
                row.name, row.disposition
            ));
        }
    }

    Ok(())
}

fn require_executable_contracts(
    rows: &[ContractRow],
    workspace: &RustWorkspace,
) -> Result<(), String> {
    require_unique_names(
        rows.iter().map(|row| row.name.as_str()),
        REQUIRED_CONTRACTS,
        "code generation contract inventory",
    )?;

    let tests = workspace.executable_test_names("code generation")?;
    let mut names = BTreeSet::new();

    for row in rows {
        if !names.insert(row.name.as_str()) {
            return Err(format!(
                "code generation coverage fixture repeats {}",
                row.name
            ));
        }

        require_source_anchor(&row.production, workspace, &row.name)?;

        if !is_fixture_anchor(&row.test) || !tests.contains(row.test.as_str()) {
            return Err(format!(
                "missing executable code generation test for {}: {}",
                row.name, row.test
            ));
        }
    }

    Ok(())
}

fn require_closed_categories(
    categories: &[ClosedCategory],
    workspace: &RustWorkspace,
) -> Result<(), String> {
    require_unique_names(
        categories.iter().map(|category| category.name.as_str()),
        REQUIRED_CLOSED_CATEGORIES,
        "code generation closed-category inventory",
    )?;

    for category in categories {
        let expected = workspace
            .enum_variants(&category.name)?
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

        let actual = category.variants.iter().cloned().collect::<BTreeSet<_>>();

        if actual != expected {
            return Err(format!(
                "code generation coverage fixture drifted from {}: expected {expected:?}, found {actual:?}",
                category.name
            ));
        }

        if category.variants.len() != actual.len() {
            return Err(format!(
                "code generation coverage fixture repeats a {} variant",
                category.name
            ));
        }

        require_source_anchor(&category.production, workspace, &category.name)?;
    }

    Ok(())
}

fn require_source_anchor(
    anchor: &SourceAnchor,
    workspace: &RustWorkspace,
    row_name: &str,
) -> Result<(), String> {
    if !is_fixture_anchor(&anchor.path) || !is_fixture_anchor(&anchor.symbol) {
        return Err(format!(
            "invalid code generation production anchor for {row_name}"
        ));
    }

    let Some(contents) = workspace.files().get(&anchor.path) else {
        return Err(format!(
            "missing code generation production file for {row_name}: {}",
            anchor.path
        ));
    };

    if contents.contains(&anchor.symbol) {
        Ok(())
    } else {
        Err(format!(
            "missing code generation production symbol for {row_name}: {} in {}",
            anchor.symbol, anchor.path
        ))
    }
}

fn require_backend_neutral_boundaries(workspace: &RustWorkspace) -> Result<(), String> {
    for manifest in [
        "crates/bray-codegen/Cargo.toml",
        "crates/bray-compilation/Cargo.toml",
    ] {
        let contents = workspace.read_text(manifest)?;

        let production_dependencies = contents
            .split_once("[dev-dependencies]")
            .map_or(contents.as_str(), |(production, _)| production);

        for forbidden in ["bray-codegen-llvm", "inkwell"] {
            if production_dependencies.contains(forbidden) {
                return Err(format!("{manifest} must not depend on {forbidden}"));
            }
        }
    }

    for (path, contents) in workspace
        .files()
        .iter()
        .filter(|(path, _)| path.starts_with("crates/bray-codegen/src/"))
    {
        for forbidden in ["LLVM", "inkwell"] {
            if contents.contains(forbidden) {
                return Err(format!(
                    "{path} exposes backend-specific code generation text: {forbidden}"
                ));
            }
        }
    }

    Ok(())
}
