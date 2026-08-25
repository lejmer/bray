use std::path::PathBuf;

use bray_compilation::{
    CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    PackageInterfaceIdentity,
};
use bray_project::ProjectProduct;
use bray_standard_library::{
    PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY, PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
};
use bray_symbols::{NativeLinkRequirement, PackageVersion, ProductIdentity, ProductKind};
use bray_tooling::source_inputs_from_file_arguments;

use super::BuildError;

pub(in crate::standard_library) fn request(
    product: &ProjectProduct,
    version: &PackageVersion,
    source_paths: &[PathBuf],
    selected: &SelectedTarget,
    worker_budget: WorkerBudget,
) -> Result<CompilationRequest, BuildError> {
    let native_links =
        crate::standard_library::os_bindings::native_links(selected.profile().identity())
            .map_err(BuildError::OsBindings)?;

    request_with_native_links(
        product,
        version,
        source_paths,
        selected,
        worker_budget,
        native_links,
    )
}

pub(super) fn request_with_native_links(
    product: &ProjectProduct,
    version: &PackageVersion,
    source_paths: &[PathBuf],
    selected: &SelectedTarget,
    worker_budget: WorkerBudget,
    native_links: Vec<NativeLinkRequirement>,
) -> Result<CompilationRequest, BuildError> {
    let sources = source_inputs_from_file_arguments(source_paths.iter().cloned())
        .map_err(|error| BuildError::Source(format!("{error:?}")))?;

    let options = CompilationOptions::new(worker_budget, ProductKind::Library, selected.clone())
        .with_native_link_inputs(native_links);

    Ok(
        CompilationRequest::with_options(product.identity().package().clone(), sources, options)
            .with_standard_library_source_authority()
            .with_platform_services(product.platform_services().iter().cloned())
            .with_package_interface_export(interface_export_request(product.identity(), version)?),
    )
}

fn interface_export_request(
    product: &ProductIdentity,
    version: &PackageVersion,
) -> Result<bray_compilation::PackageInterfaceExportRequest, BuildError> {
    let product_identity =
        InterfaceProductIdentity::try_new(PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY)
            .ok_or(BuildError::InvalidIdentity)?;

    // The interface shares the immutable Arc-backed package version from the project graph.
    let identity = PackageInterfaceIdentity::try_new(
        product.package().clone(),
        version.clone(),
        product_identity,
        InterfaceProductKind::Library,
        PUBLIC_STANDARD_LIBRARY_SURFACE_IDENTITY,
    )
    .ok_or(BuildError::InvalidIdentity)?;

    Ok(bray_compilation::PackageInterfaceExportRequest::new(
        identity,
        InterfaceLanguageRevision::new(0),
    ))
}
