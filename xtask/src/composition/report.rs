use std::path::Path;
use std::process::Output;

use bray_emitter::{
    ArtifactKind, ManagedFilesystemDestination, ManagedOutputDirectory, RetainedProductGeneration,
    retain_published_generation,
};
use bray_symbols::{PackageIdentity, ProductIdentity};

use crate::native_test_report::{NativeOutcome, NativeTestReport};

pub(super) fn retained(workspace: &Path, case: &str) -> Result<RetainedProductGeneration, String> {
    let directory = ManagedOutputDirectory::try_new("native/release/composition")
        .ok_or("invalid composition output directory")?;

    let product = ProductIdentity::try_new(
        PackageIdentity::try_new("composition").ok_or("invalid composition package")?,
        case,
    )
    .ok_or("invalid composition product")?;

    retain_published_generation(
        ManagedFilesystemDestination::new(workspace.join("build"), directory),
        &product,
        &|| false,
    )
    .map_err(|error| format!("composition {case}: retained product: {error:?}"))
}

pub(super) fn validate(
    output: &Output,
    workspace: &Path,
    case: &str,
    no_build: bool,
) -> Result<NativeTestReport, String> {
    if !output.status.success() {
        let diagnostics: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|error| {
                format!(
                    "composition {case}: command output: {error}\n{}",
                    crate::command::failure(case, output)
                )
            })?;

        if diagnostics
            .get("has_errors")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            return Err(format!(
                "composition {case}: production command failed\n{}",
                crate::command::failure(case, output)
            ));
        }
    }

    let report: NativeTestReport = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "composition {case}: report decode: {error}\n{}",
            crate::command::failure(case, output)
        )
    })?;

    validate_structure(&report, case, no_build, output.status.success())
        .and_then(|()| validate_catalog(&report, workspace, case))
        .map_err(|error| format!("{error}\n{}", crate::command::failure(case, output)))?;

    Ok(report)
}

fn validate_structure(
    report: &NativeTestReport,
    case: &str,
    no_build: bool,
    succeeded: bool,
) -> Result<(), String> {
    let failure =
        || format!("composition {case}: report structure does not match the selected case");

    let expects_error = case == "typed-error";
    let discovered = if case == "selected-entry" { 2 } else { 1 };

    if report
        .duration_nanoseconds
        .is_some_and(|duration| duration > 60_000_000_000)
        || report.format != 1
        || succeeded == expects_error
        || report.selection.discovered != discovered
        || report.selection.selected != 1
        || report.selection.filtered_out != discovered - 1
        || report.products.len() != 1
        || report.summary.failed != usize::from(expects_error)
        || report.summary.passed != usize::from(!expects_error)
        || report.build.reused != no_build
        || report.build.compilation == no_build
        || report.build.emission == no_build
        || report.build.linking == no_build
    {
        return Err(failure());
    }

    let product = &report.products[0];

    let [test] = product.tests.as_slice() else {
        return Err(failure());
    };

    let module = case.replace('-', "_");

    if test
        .duration_nanoseconds
        .is_some_and(|duration| duration > 60_000_000_000)
        || product.package != "composition"
        || product.product != case
        || test.identity != format!("composition/{case}::composition.{module}.run")
        || ![&test.stdout, &test.stderr]
            .iter()
            .all(|stream| stream.captured_exactly(&[]))
    {
        return Err(failure());
    }

    Ok(())
}

fn validate_catalog(report: &NativeTestReport, workspace: &Path, case: &str) -> Result<(), String> {
    let failure = || format!("composition {case}: catalog and report identities disagree");
    let product = &report.products[0];
    let test = &product.tests[0];
    let discovered = report.selection.discovered;
    let expects_error = case == "typed-error";
    let retained = retained(workspace, case)?;

    let catalog_path = retained
        .artifact_path(ArtifactKind::TestCatalog, 0)
        .ok_or("composition catalog is absent")?;

    let bytes = std::fs::read(catalog_path)
        .map_err(|error| crate::workspace::io_error("read", catalog_path, error))?;

    let (catalog, digest) = bray_test_protocol::decode_test_catalog(&bytes)
        .map_err(|error| format!("composition {case}: catalog decode: {error:?}"))?;

    match (&test.outcome, expects_error) {
        (NativeOutcome::Passed, false) => {}
        (
            NativeOutcome::ReturnedError {
                error_type,
                formatted_value,
            },
            true,
        ) if catalog.entries().iter().any(|entry| {
            entry
                .error_type()
                .is_some_and(|expected| expected.as_str() == error_type)
        }) && formatted_value.is_none() => {}
        _ => return Err(failure()),
    }

    if catalog.product().package().as_str() != "composition"
        || catalog.product().name() != case
        || catalog.entries().iter().any(|entry| {
            entry.execution()
                != if case == "runtime-role" {
                    bray_symbols::CallableExecution::Asynchronous
                } else {
                    bray_symbols::CallableExecution::Synchronous
                }
        })
        || catalog.entries().len() != discovered
        || bray_base::lowercase_hex(&digest.bytes()) != product.catalog_digest
        || !report.build.products.iter().any(|product| {
            product.product == format!("composition/{case}")
                && product.generation == retained.identity().to_hex()
        })
    {
        return Err(failure());
    }

    Ok(())
}

pub(super) fn same_generation(
    first: &NativeTestReport,
    rerun: &NativeTestReport,
) -> Result<(), String> {
    let generations = |report: &NativeTestReport| {
        report
            .build
            .products
            .iter()
            .map(|entry| (entry.product.clone(), entry.generation.clone()))
            .collect::<Vec<_>>()
    };

    if generations(first) != generations(rerun)
        || first.products[0].catalog_digest != rerun.products[0].catalog_digest
    {
        return Err(
            "composition retained rerun changed the product or catalog identity".to_owned(),
        );
    }

    Ok(())
}

pub(super) fn evidence(root: &Path, workspace: &Path, toolchain: &Path) -> String {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");

    let mut evidence = format!(
        "compiler={} digest={:?}\ntoolchain={} digest={:?}\nstandard-library={} digest={:?}\nruntime={} digest={:?}\noutput={}",
        compiler.display(),
        bray_emitter::path_digest(&compiler),
        toolchain.display(),
        bray_emitter::toolchain_path_digest(&toolchain.join("lib/bray")),
        toolchain.join("lib/bray/standard-library").display(),
        bray_emitter::build_input_path_digest(&toolchain.join("lib/bray/standard-library")),
        toolchain.join("lib/bray/runtime").display(),
        bray_emitter::build_input_path_digest(&toolchain.join("lib/bray/runtime")),
        workspace.join("build").display()
    );

    for case in super::command::CASES {
        match retained(workspace, case) {
            Ok(generation) => {
                evidence.push_str(&format!(
                    "\n{case}: generation={} identities={:?}",
                    generation.identity().to_hex(),
                    generation.build_identity()
                ));

                for (kind, ordinal, path) in generation.artifacts() {
                    evidence.push_str(&format!(
                        "\n  {kind:?}/{ordinal}: {} digest={:?}",
                        path.display(),
                        bray_emitter::path_digest(path)
                    ));
                }
            }
            Err(error) => evidence.push_str(&format!("\n{error}")),
        }
    }

    evidence
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn reports_require_matching_protocol_selection_and_build_activity() {
        let valid = json!({
            "format": 1,
            "build": {"reused": true, "compilation": false, "emission": false, "linking": false, "products": []},
            "selection": {"discovered": 1, "selected": 1, "filtered_out": 0},
            "products": [{"package": "composition", "product": "synchronous", "catalog_digest": "",
                "tests": [{"identity": "composition/synchronous::composition.synchronous.run", "outcome": {"kind": "passed"},
                    "stdout": {"policy": "captured", "bytes": [], "truncated": false, "discarded_byte_count": 0, "failure": null},
                    "stderr": {"policy": "captured", "bytes": [], "truncated": false, "discarded_byte_count": 0, "failure": null}}]}],
            "summary": {"passed": 1, "failed": 0}
        });

        let report = serde_json::from_value(valid.clone()).unwrap();
        assert!(super::validate_structure(&report, "synchronous", true, true).is_ok());

        for (pointer, replacement) in [
            ("/format", json!(2)),
            ("/build/compilation", json!(true)),
            ("/selection/selected", json!(0)),
            ("/products/0/product", json!("other")),
            ("/products/0/tests/0/identity", json!("other")),
            ("/products/0/tests/0/stdout/policy", json!("inherited")),
            ("/products/0/tests/0/stderr/bytes", json!([1])),
            ("/products/0/tests/0/stdout/truncated", json!(true)),
            ("/products/0/tests/0/stderr/discarded_byte_count", json!(1)),
            (
                "/products/0/tests/0/stdout/failure",
                json!({"kind": "read"}),
            ),
            ("/duration_nanoseconds", json!(60_000_000_001_u64)),
        ] {
            let mut altered = valid.clone();

            if pointer == "/duration_nanoseconds" {
                altered["duration_nanoseconds"] = replacement;
            } else {
                *altered.pointer_mut(pointer).unwrap() = replacement;
            }

            let report = serde_json::from_value(altered).unwrap();

            assert!(
                super::validate_structure(&report, "synchronous", true, true).is_err(),
                "{pointer}"
            );
        }
    }
}
