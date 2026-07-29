use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

#[path = "support/readiness.rs"]
mod support;
#[path = "support/syntax.rs"]
mod syntax_support;

use support::{rust_tests, rust_workspace, workspace_root};
use syntax_support::EnumInventory;

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

const CODEGEN_RUNTIME_ROLES: &[&str] = &[
    "CleanupIncidentReporting",
    "CleanupIncidentTransfer",
    "CurrentRunCancellationObservation",
    "FrameLifecycleResolution",
    "FrameResume",
    "FrameTaskBroadcast",
    "JoinRegistration",
    "RootCancellationRequest",
    "RootExecution",
    "RootTerminalObservation",
    "StructuredShutdown",
    "SuspensionRegistration",
    "TaskAllocation",
    "TaskCancellationRequest",
    "TaskStart",
    "TerminalPublication",
    "Wake",
];

#[derive(Deserialize)]
struct CoverageFixture {
    unit_kinds: Vec<CoverageRow>,
    operations: Vec<CoverageRow>,
    async_operations: Vec<CoverageRow>,
    host_operations: Vec<CoverageRow>,
    terminators: Vec<CoverageRow>,
    closed_categories: Vec<ClosedCategory>,
    runtime_roles: Vec<RuntimeRoleRow>,
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
struct RuntimeRoleRow {
    name: String,
    production: SourceAnchor,
    consumption: SourceAnchor,
}

#[derive(Deserialize)]
struct SourceAnchor {
    path: String,
    symbol: String,
}

#[test]
fn codegen_coverage_fixture_matches_the_complete_backend_contract() {
    let root = workspace_root();
    let fixture = coverage_fixture(&root);
    let rust = rust_workspace(&root);
    let enums = EnumInventory::new(&rust);

    assert_enum_coverage(&fixture.unit_kinds, &rust, &enums, "MirUnitKind");
    assert_enum_coverage(&fixture.operations, &rust, &enums, "MirOperationKind");

    assert_enum_coverage(
        &fixture.async_operations,
        &rust,
        &enums,
        "MirAsyncOperation",
    );

    assert_enum_coverage(
        &fixture.host_operations,
        &rust,
        &enums,
        "MirHostOperation",
    );

    assert_enum_coverage(&fixture.terminators, &rust, &enums, "MirTerminatorKind");

    assert_enum_coverage(
        &fixture.artifact_kinds,
        &rust,
        &enums,
        "BackendArtifactKind",
    );

    assert_closed_categories(&fixture.closed_categories, &rust, &enums);
    assert_runtime_roles(&fixture.runtime_roles, &rust);
    assert_translated(&fixture.unit_kinds);
    assert_translated(&fixture.operations);
    assert_translated(&fixture.async_operations);
    assert_translated(&fixture.host_operations);
    assert_translated(&fixture.terminators);
    assert_artifact_dispositions(&fixture.artifact_kinds);
    assert_contracts_are_executable(&fixture.contracts, &rust);
}

#[test]
fn codegen_and_compilation_keep_backend_implementations_outside_their_boundaries() {
    let root = workspace_root();
    let rust = rust_workspace(&root);

    for manifest in [
        "crates/bray-codegen/Cargo.toml",
        "crates/bray-compilation/Cargo.toml",
    ] {
        let path = root.join(manifest);

        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));

        for forbidden in ["bray-codegen-llvm", "inkwell"] {
            assert!(
                !contents.contains(forbidden),
                "{manifest} must not depend on {forbidden}"
            );
        }
    }

    for (path, contents) in rust
        .iter()
        .filter(|(path, _)| path.starts_with("crates/bray-codegen/src/"))
    {
        for forbidden in ["LLVM", "inkwell"] {
            assert!(
                !contents.contains(forbidden),
                "{path} exposes backend-specific code generation text: {forbidden}"
            );
        }
    }
}

fn coverage_fixture(root: &Path) -> CoverageFixture {
    let path = root.join("crates/bray-compilation/tests/fixtures/codegen-coverage.json");

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
        "code generation coverage fixture drifted from {enum_name}"
    );

    assert_eq!(
        rows.len(),
        actual.len(),
        "code generation coverage fixture repeats a {enum_name} variant"
    );

    for row in rows {
        assert_source_anchor(&row.production, rust, &row.name);
    }
}

fn assert_translated(rows: &[CoverageRow]) {
    for row in rows {
        assert_eq!(
            row.disposition,
            Disposition::Translated,
            "{} must be translated",
            row.name
        );
    }
}

fn assert_artifact_dispositions(rows: &[CoverageRow]) {
    for row in rows {
        let expected = match row.name.as_str() {
            "RelocatableObject" | "Assembly" | "BackendIr" | "BackendBitcode" => {
                Disposition::Serialized
            }
            "ExecutableModule" | "DebugCompanion" => Disposition::CapabilityFailure,
            name => panic!("unknown backend artifact kind in coverage fixture: {name}"),
        };

        assert_eq!(
            row.disposition, expected,
            "incorrect artifact disposition for {}",
            row.name
        );
    }
}

fn assert_contracts_are_executable(
    rows: &[ContractRow],
    rust: &BTreeMap<String, String>,
) {
    assert_names(rows.iter().map(|row| row.name.as_str()), REQUIRED_CONTRACTS);

    let tests = rust_tests(rust)
        .into_iter()
        .map(|test| {
            assert!(
                !test.path().is_empty() && !test.body().is_empty(),
                "executable code generation test {} has no source body",
                test.name()
            );

            test.name().to_owned()
        })
        .collect::<BTreeSet<_>>();

    let mut names = BTreeSet::new();

    for row in rows {
        assert!(
            names.insert(row.name.as_str()),
            "code generation coverage fixture repeats {}",
            row.name
        );

        assert_source_anchor(&row.production, rust, &row.name);

        assert!(
            is_fixture_anchor(&row.test) && tests.contains(&row.test),
            "missing executable code generation test for {}: {}",
            row.name,
            row.test
        );
    }
}

fn assert_closed_categories(
    categories: &[ClosedCategory],
    rust: &BTreeMap<String, String>,
    enums: &EnumInventory,
) {
    assert_names(
        categories.iter().map(|category| category.name.as_str()),
        REQUIRED_CLOSED_CATEGORIES,
    );

    for category in categories {
        let expected = enums
            .variants(&category.name)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

        let actual = category
            .variants
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

        assert_eq!(
            actual, expected,
            "code generation coverage fixture drifted from {}",
            category.name
        );

        assert_eq!(
            category.variants.len(),
            actual.len(),
            "code generation coverage fixture repeats a {} variant",
            category.name
        );

        assert_source_anchor(&category.production, rust, &category.name);
    }
}

fn assert_runtime_roles(
    rows: &[RuntimeRoleRow],
    rust: &BTreeMap<String, String>,
) {
    assert_names(
        rows.iter().map(|row| row.name.as_str()),
        CODEGEN_RUNTIME_ROLES,
    );

    for row in rows {
        assert_source_anchor(&row.production, rust, &row.name);
        assert_source_anchor(&row.consumption, rust, &row.name);
    }
}

fn assert_names<'name>(
    actual: impl IntoIterator<Item = &'name str>,
    expected: &[&str],
) {
    let actual = actual.into_iter().collect::<Vec<_>>();
    let unique = actual.iter().copied().collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();

    assert_eq!(
        actual.len(),
        unique.len(),
        "code generation coverage inventory contains duplicate names"
    );

    assert_eq!(unique, expected, "code generation coverage inventory drifted");
}

fn assert_source_anchor(
    anchor: &SourceAnchor,
    rust: &BTreeMap<String, String>,
    row_name: &str,
) {
    assert!(
        is_fixture_anchor(&anchor.path) && is_fixture_anchor(&anchor.symbol),
        "invalid code generation production anchor for {row_name}"
    );

    let Some(contents) = rust.get(&anchor.path) else {
        panic!(
            "missing code generation production file for {row_name}: {}",
            anchor.path
        );
    };

    assert!(
        contents.contains(&anchor.symbol),
        "missing code generation production symbol for {row_name}: {} in {}",
        anchor.symbol,
        anchor.path
    );
}

fn is_fixture_anchor(value: &str) -> bool {
    let value = value.trim();

    !value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "none" | "pending" | "tbd" | "todo"
        )
}
