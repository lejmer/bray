use bray_compilation::{Compilation, SelectedTarget};
use bray_project::load_standard_library_project_graph;
use bray_target::NativeTarget;

use super::core::{standard_library_product, standard_library_version};
use super::error::BuildError;
use crate::workspace;

pub(super) fn verify(mut arguments: impl Iterator<Item = String>) -> Result<(), BuildError> {
    if let Some(argument) = arguments.next() {
        return Err(BuildError::UnexpectedArgument(argument));
    }

    crate::progress::run("Checking standard library OS bindings", || {
        crate::standard_library::os_bindings::verify()
    })
    .map_err(BuildError::OsBindings)?;

    crate::progress::run(
        "Checking standard library source for every native target",
        || verify_source_targets(),
    )?;

    let directory = tempfile::Builder::new()
        .prefix("bray-standard-library-verification-")
        .tempdir()
        .map_err(BuildError::TemporaryDirectory)?;

    crate::progress::run("Verifying the standard library bundle", || {
        crate::standard_library::conformance::verify(directory.path())
    })
}

fn verify_source_targets() -> Result<(), BuildError> {
    let root = workspace::root().map_err(BuildError::Workspace)?;
    let source = root.join("standard-library");

    let graph = load_standard_library_project_graph(&source)
        .map_err(|error| BuildError::Project(format!("{error:?}")))?;

    let product = standard_library_product(&graph)?;
    let version = standard_library_version(&graph)?;

    let source_paths = product
        .sources()
        .iter()
        .map(|path| path.beneath(&source))
        .collect::<Vec<_>>();

    crate::standard_library::target::try_for_each_native_target_compilation(
        product,
        version,
        &source_paths,
        "standard library source",
        verify_source_target,
    )
}

fn verify_source_target(
    target: NativeTarget,
    selected: &SelectedTarget,
    compilation: &Compilation,
) -> Result<(), BuildError> {
    let export = compilation
        .package_interface_export_bundle()
        .ok_or_else(|| {
            BuildError::conformance(
                "cross-target-source",
                format!("{} did not configure interface export", target.as_str()),
            )
        })?;

    if let Err(error) = export {
        return Err(BuildError::compilation_failed(
            selected.profile().identity().clone(),
            format!("{error:?}"),
            compilation.check_diagnostics(),
            compilation.sources(),
        ));
    }

    Ok(())
}
