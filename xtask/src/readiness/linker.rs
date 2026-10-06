use serde::Deserialize;

use super::workspace::{RustWorkspace, require_executable_source_contracts, require_ordered_names};

const REQUIRED_CONTRACTS: &[&str] = &[
    "typed plan validation",
    "lld driver selection",
    "configured system driver selection",
    "exact link arguments",
    "deterministic response files",
    "static archives",
    "executable products",
    "shared products",
    "companion outputs",
    "tool failures",
    "compiler operation cancellation",
    "missing outputs",
    "repeat determinism",
    "async host entry",
    "runtime ABI and startup inputs",
    "root frame descriptor",
    "main thread lane",
    "structured shutdown inputs",
    "typed input boundary",
];

const FORBIDDEN_DEPENDENCIES: &[&str] = &[
    "bray-source",
    "bray-syntax",
    "bray-parser",
    "bray-binder",
    "bray-bound-tree",
    "bray-checker",
    "bray-lowering",
    "bray-ir",
    "bray-codegen",
    "bray-emitter",
    "bray-compilation",
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
    let fixture: CoverageFixture = workspace.read_fixture("linker-coverage.json")?;

    require_ordered_names(
        fixture.contracts.iter().map(|row| row.name.as_str()),
        REQUIRED_CONTRACTS,
        "linker readiness contracts",
    )?;

    require_executable_rows(&fixture.contracts, workspace)?;

    require_typed_linker_input_boundary(workspace)
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
        "linker",
    )
}

fn require_typed_linker_input_boundary(workspace: &RustWorkspace) -> Result<(), String> {
    let dependencies = workspace.package_dependencies("crates/bray-linker/Cargo.toml")?;

    for forbidden in FORBIDDEN_DEPENDENCIES {
        if dependencies.contains_production(forbidden) {
            return Err(format!("bray-linker must not depend on {forbidden}"));
        }
    }

    for (path, contents) in workspace
        .files()
        .iter()
        .filter(|(path, _)| path.starts_with("crates/bray-linker/src/"))
    {
        let production = contents.split("#[cfg(test)]").next().unwrap_or(contents);

        for forbidden in ["bray_syntax", "bray_bound_tree", "Future<", "Task<"] {
            if production.contains(forbidden) {
                return Err(format!(
                    "{path} interprets source semantics at the linker boundary: {forbidden}"
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::workspace::tests::dependency_workspace;
    use super::{FORBIDDEN_DEPENDENCIES, require_typed_linker_input_boundary};

    #[test]
    fn semantic_dependencies_are_allowed_only_for_linker_development() {
        for forbidden in FORBIDDEN_DEPENDENCIES {
            for section in [
                "dev-dependencies",
                "target.'cfg(windows)'.dev-dependencies",
                "dependencies",
                "build-dependencies",
                "target.'cfg(windows)'.dependencies",
            ] {
                let manifest = format!(
                    r#"
                    [dev-dependencies]
                    fixture = "1"

                    [{section}.semantic]
                    package = "{forbidden}"
                    version = "1"
                "#
                );

                let (_directory, workspace) = dependency_workspace(&[("bray-linker", &manifest)]);

                let expected = if section.ends_with("dev-dependencies") {
                    Ok(())
                } else {
                    Err(format!("bray-linker must not depend on {forbidden}"))
                };

                assert_eq!(require_typed_linker_input_boundary(&workspace), expected);
            }
        }
    }
}
