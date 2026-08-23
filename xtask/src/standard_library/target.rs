use std::path::PathBuf;

use bray_compilation::{Compilation, SelectedTarget, WorkerBudget};
use bray_project::ProjectProduct;
use bray_symbols::PackageVersion;
use bray_target::NativeTarget;

use super::command::{BuildError, standard_library_source_request};

pub(super) fn try_for_each_native_target_compilation(
    product: &ProjectProduct,
    version: &PackageVersion,
    source_paths: &[PathBuf],
    operation: &'static str,
    check: impl Fn(NativeTarget, &SelectedTarget, &Compilation) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    let worker_budget = WorkerBudget::default();

    for target in NativeTarget::ALL {
        let (selected, compilation) = target_compilation(
            product,
            version,
            source_paths,
            target,
            worker_budget,
            operation,
        )?;

        run_target_check(operation, target, &selected, &compilation, &check)?;
    }

    Ok(())
}

fn run_target_check(
    operation: &str,
    target: NativeTarget,
    selected: &SelectedTarget,
    compilation: &Compilation,
    check: &impl Fn(NativeTarget, &SelectedTarget, &Compilation) -> Result<(), BuildError>,
) -> Result<(), BuildError> {
    crate::progress::run(
        &format!("Checking {operation} for {}", target.as_str()),
        || check(target, selected, compilation),
    )
}

fn target_compilation(
    product: &ProjectProduct,
    version: &PackageVersion,
    source_paths: &[PathBuf],
    target: NativeTarget,
    worker_budget: WorkerBudget,
    operation: &'static str,
) -> Result<(SelectedTarget, Compilation), BuildError> {
    let selected = SelectedTarget::for_native(target);

    let request =
        standard_library_source_request(product, version, source_paths, &selected, worker_budget)?;

    let compilation = Compilation::load(request).map_err(|error| {
        BuildError::conformance(
            operation,
            format!("{} could not load: {error:?}", target.as_str()),
        )
    })?;

    Ok((selected, compilation))
}
