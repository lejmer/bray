use bray_codegen::CodegenConfiguration;
use bray_diagnostics::DiagnosticBag;
use bray_source::SourceStore;
use bray_symbols::{ImportedInterfaceId, NativeLinkRequirement, PackageIdentity, ProductKind};

use super::Compilation;
use crate::fact::{CompilationInputKey, CompilationInputs};
use crate::request::{CompilationOptions, DependencyInterfaceInput, PackageInterfaceExportRequest};

pub(super) fn compilation_inputs(
    package_identity: &PackageIdentity,
    package_source_authority: crate::PackageSourceAuthority,
    standard_library: Option<&bray_standard_library::StandardLibraryResolver>,
    options: &CompilationOptions,
    sources: &SourceStore,
    source_diagnostics: &DiagnosticBag,
    dependency_interfaces: &[DependencyInterfaceInput],
    platform_services: &[bray_runtime_interface::PlatformServiceBinding],
    package_interface_export: Option<&PackageInterfaceExportRequest>,
    codegen: Option<&CodegenConfiguration>,
) -> CompilationInputs {
    let mut inputs = CompilationInputs::default();

    insert_package_inputs(
        &mut inputs,
        package_identity,
        package_source_authority,
        standard_library,
    );

    insert_source_inputs(&mut inputs, sources, source_diagnostics);
    insert_option_inputs(&mut inputs, options);
    insert_dependency_inputs(&mut inputs, dependency_interfaces);

    insert_product_inputs(
        &mut inputs,
        platform_services,
        package_interface_export,
        codegen,
    );

    inputs
}

fn insert_package_inputs(
    inputs: &mut CompilationInputs,
    package_identity: &PackageIdentity,
    package_source_authority: crate::PackageSourceAuthority,
    standard_library: Option<&bray_standard_library::StandardLibraryResolver>,
) {
    inputs.insert(CompilationInputKey::PackageIdentity, package_identity);

    inputs.insert(
        CompilationInputKey::PackageSourceAuthority,
        &package_source_authority,
    );

    inputs.insert(
        CompilationInputKey::StandardLibrary,
        &standard_library.map(|resolver| resolver.root().path()),
    );
}

fn insert_source_inputs(
    inputs: &mut CompilationInputs,
    sources: &SourceStore,
    source_diagnostics: &DiagnosticBag,
) {
    let source_set = sources
        .iter()
        .map(|source| (source.source_id(), source.identity()))
        .collect::<Vec<_>>();

    inputs.insert(CompilationInputKey::SourceSet, &source_set);

    for source in sources.iter() {
        inputs.insert(CompilationInputKey::Source(source.source_id()), source);
    }

    inputs.insert(CompilationInputKey::SourceDiagnostics, source_diagnostics);
}

fn insert_option_inputs(inputs: &mut CompilationInputs, options: &CompilationOptions) {
    let limits = options.semantic_analysis_limits();

    inputs.insert(CompilationInputKey::ProductKind, &options.product_kind());

    inputs.insert(
        CompilationInputKey::SelectedTarget,
        options.selected_target(),
    );

    inputs.insert(
        CompilationInputKey::NativeLinkInputs,
        options.native_link_inputs(),
    );

    inputs.insert(
        CompilationInputKey::SemanticRecursionLimit,
        &limits.recursion_depth(),
    );

    inputs.insert(
        CompilationInputKey::SemanticPairwiseLimit,
        &limits.pairwise_comparisons(),
    );
}

fn insert_dependency_inputs(
    inputs: &mut CompilationInputs,
    dependencies: &[DependencyInterfaceInput],
) {
    let dependency_set = dependencies
        .iter()
        .map(|input| (input.package(), input.product()))
        .collect::<Vec<_>>();

    inputs.insert(CompilationInputKey::DependencySet, &dependency_set);

    for (index, input) in dependencies.iter().enumerate() {
        let Some(interface) = ImportedInterfaceId::try_from_index(index) else {
            continue;
        };

        insert_dependency_input(inputs, interface, input);
    }
}

fn insert_dependency_input(
    inputs: &mut CompilationInputs,
    interface: ImportedInterfaceId,
    input: &DependencyInterfaceInput,
) {
    let interface_value = (
        input.package(),
        input.product(),
        input.artifact_path(),
        input.dependency_span(),
        input.validation_policy(),
        input.bytes(),
    );

    inputs.insert(
        CompilationInputKey::DependencyInterface(interface),
        &interface_value,
    );

    let implementation_value = (
        input.implementation_artifact_path(),
        input
            .implementation_artifact()
            .map(bray_package_interface::PackageImplementationArtifact::bytes),
    );

    inputs.insert(
        CompilationInputKey::DependencyImplementation(interface),
        &implementation_value,
    );
}

fn insert_product_inputs(
    inputs: &mut CompilationInputs,
    platform_services: &[bray_runtime_interface::PlatformServiceBinding],
    package_interface_export: Option<&PackageInterfaceExportRequest>,
    codegen: Option<&CodegenConfiguration>,
) {
    inputs.insert(CompilationInputKey::PlatformServices, platform_services);

    inputs.insert(
        CompilationInputKey::PackageInterfaceExport,
        &package_interface_export,
    );

    inputs.insert(
        CompilationInputKey::CodegenConfiguration,
        &codegen.map(CodegenConfiguration::selected),
    );
}

impl Compilation {
    pub(super) fn product_kind(&self) -> ProductKind {
        self.record_input(CompilationInputKey::ProductKind);

        self.state.options.product_kind()
    }

    pub(super) fn requested_target(&self) -> &crate::SelectedTarget {
        self.record_input(CompilationInputKey::SelectedTarget);

        self.state.options.selected_target()
    }

    pub(super) fn native_link_inputs(&self) -> &[NativeLinkRequirement] {
        self.record_input(CompilationInputKey::NativeLinkInputs);

        self.state.options.native_link_inputs()
    }

    pub(super) fn semantic_recursion_limit(&self) -> usize {
        self.record_input(CompilationInputKey::SemanticRecursionLimit);

        self.state
            .options
            .semantic_analysis_limits()
            .recursion_depth()
    }

    pub(super) fn semantic_pairwise_limit(&self) -> u64 {
        self.record_input(CompilationInputKey::SemanticPairwiseLimit);

        self.state
            .options
            .semantic_analysis_limits()
            .pairwise_comparisons()
    }

    pub(super) fn dependency_interface_count(&self) -> usize {
        self.record_input(CompilationInputKey::DependencySet);

        self.state.dependency_interfaces.len()
    }

    pub(super) fn dependency_interfaces(&self) -> &[DependencyInterfaceInput] {
        self.record_input(CompilationInputKey::DependencySet);

        &self.state.dependency_interfaces
    }

    pub(super) fn dependency_interface(
        &self,
        interface: ImportedInterfaceId,
    ) -> Option<&DependencyInterfaceInput> {
        self.record_input(CompilationInputKey::DependencyInterface(interface));

        self.state.dependency_interfaces.get(interface.to_index()?)
    }

    pub(super) fn record_dependency_implementation(&self, interface: ImportedInterfaceId) {
        self.record_input(CompilationInputKey::DependencyImplementation(interface));
    }

    pub(super) fn platform_services(&self) -> &[bray_runtime_interface::PlatformServiceBinding] {
        self.record_input(CompilationInputKey::PlatformServices);

        &self.state.platform_services
    }

    pub(super) fn package_interface_export_request(
        &self,
    ) -> Option<&PackageInterfaceExportRequest> {
        self.record_input(CompilationInputKey::PackageInterfaceExport);

        self.state.package_interface_export.as_ref()
    }

    pub(super) fn standard_library(
        &self,
    ) -> Option<&bray_standard_library::StandardLibraryResolver> {
        self.record_input(CompilationInputKey::StandardLibrary);

        self.state.standard_library.as_ref()
    }

    pub(super) fn record_codegen_configuration(&self) {
        self.record_input(CompilationInputKey::CodegenConfiguration);
    }

    pub(super) fn record_input(&self, key: CompilationInputKey) {
        self.state
            .fact_runtime
            .record_input(key)
            .unwrap_or_else(|error| panic!("compilation input tracking must succeed: {error:?}"));
    }
}
