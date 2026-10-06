use std::sync::Arc;

use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    PackageInterfaceExportBundle, encode_package_interface,
};
use bray_runtime_interface::PlatformServiceBinding;
use bray_symbols::{PackageIdentity, ProductKind};
use bray_testing::test_source_inputs;

use crate::test_support::package_version;
use crate::{
    Compilation, CompilationOptions, CompilationProfileConfiguration, CompilationRequest,
    DependencyInterfaceInput, PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
};

pub(super) fn execution_consumer(provider: &Compilation, source: &str) -> Compilation {
    crate::test_support::compilation_with_dependencies(source, [execution_dependency(provider)])
}

pub(super) fn execution_dependency(provider: &Compilation) -> DependencyInterfaceInput {
    execution_bundle_dependency(export(provider))
}

pub(super) fn execution_bundle_dependency(
    bundle: &PackageInterfaceExportBundle,
) -> DependencyInterfaceInput {
    let artifact = encode_package_interface(bundle).unwrap();

    DependencyInterfaceInput::new(
        bundle.surface().identity().package().clone(),
        bundle.surface().identity().product().clone(),
        "provider.brayi",
        artifact.shared_bytes(),
        InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
    )
}

pub(super) fn export(compilation: &Compilation) -> &Arc<PackageInterfaceExportBundle> {
    match compilation.package_interface_export_bundle() {
        Some(Ok(bundle)) => bundle,
        Some(Err(error)) => panic!("test library interface must build: {error:?}"),
        None => panic!("test compilation must configure a library interface"),
    }
}

pub(super) fn compilation(source: &str) -> Compilation {
    compilation_from_sources([source])
}

pub(super) fn compilation_from_sources<const N: usize>(sources: [&str; N]) -> Compilation {
    compilation_from_sources_for_product(sources, ProductKind::Library)
}

pub(super) fn compilation_from_sources_for_product<const N: usize>(
    sources: [&str; N],
    product_kind: ProductKind,
) -> Compilation {
    compilation_from_sources_for_product_with_platform_services(
        sources,
        product_kind,
        std::iter::empty(),
    )
}

pub(super) fn compilation_from_sources_for_product_with_platform_services<const N: usize>(
    sources: [&str; N],
    product_kind: ProductKind,
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
) -> Compilation {
    compilation_from_sources_for_product_with_platform_services_and_worker_budget(
        sources,
        product_kind,
        platform_services,
        WorkerBudget::default(),
        None,
        [],
    )
}

pub(super) fn compilation_from_sources_for_product_with_platform_services_and_worker_budget<
    const N: usize,
>(
    sources: [&str; N],
    product_kind: ProductKind,
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    worker_budget: WorkerBudget,
    profile: Option<CompilationProfileConfiguration>,
    native_links: impl IntoIterator<Item = bray_symbols::NativeLinkRequirement>,
) -> Compilation {
    let package = PackageIdentity::try_new("example.package")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    let product = InterfaceProductIdentity::try_new("library")
        .unwrap_or_else(|| panic!("test product identity must be valid"));

    let identity = bray_package_interface::PackageInterfaceIdentity::try_new(
        package.clone(),
        package_version(),
        product,
        bray_package_interface::InterfaceProductKind::Library,
        "public",
    )
    .unwrap_or_else(|| panic!("test export identity must be valid"));

    let export = PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

    let sources = test_source_inputs("test", sources);

    let options = CompilationOptions::new(worker_budget, product_kind, SelectedTarget::default())
        .with_native_link_inputs(native_links);

    let request = CompilationRequest::with_options(package, sources, options)
        .with_platform_services(platform_services)
        .with_package_interface_export(export);

    let request = match profile {
        Some(profile) => request.with_profile(profile),
        None => request,
    };

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
}
