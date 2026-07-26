use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_checker::SemanticAnalysisLimits;
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    PackageInterfaceIdentity,
};
use bray_source::{SourceInput, SourceSpan};
use bray_symbols::{NativeLinkRequirement, PackageIdentity, ProductKind};

use crate::SelectedTarget;
use crate::worker::WorkerBudget;

/// Options for one compiler operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CompilationOptions {
    worker_budget: WorkerBudget,
    product_kind: ProductKind,
    selected_target: SelectedTarget,
    native_link_inputs: Arc<[NativeLinkRequirement]>,
    semantic_analysis_limits: SemanticAnalysisLimits,
}

impl CompilationOptions {
    /// Creates compilation options.
    pub fn new(
        worker_budget: WorkerBudget,
        product_kind: ProductKind,
        selected_target: SelectedTarget,
    ) -> Self {
        Self {
            worker_budget,
            product_kind,
            selected_target,
            native_link_inputs: Arc::from([]),
            semantic_analysis_limits: SemanticAnalysisLimits::default(),
        }
    }

    /// Returns these options with the native link inputs supplied by the host.
    pub fn with_native_link_inputs(
        mut self,
        inputs: impl IntoIterator<Item = NativeLinkRequirement>,
    ) -> Self {
        self.native_link_inputs = shared_slice(inputs);
        self
    }

    /// Returns these options with explicit semantic-analysis resource limits.
    pub const fn with_semantic_analysis_limits(
        mut self,
        limits: SemanticAnalysisLimits,
    ) -> Self {
        self.semantic_analysis_limits = limits;

        self
    }

    /// Returns the compiler-owned CPU worker budget.
    pub const fn worker_budget(&self) -> WorkerBudget {
        self.worker_budget
    }

    /// Returns the selected package-product category.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns the selected target for this compiler operation.
    pub const fn selected_target(&self) -> &SelectedTarget {
        &self.selected_target
    }

    /// Returns native link inputs available to the selected package and target.
    pub fn native_link_inputs(&self) -> &[NativeLinkRequirement] {
        &self.native_link_inputs
    }

    /// Returns the deterministic semantic-analysis resource limits.
    pub const fn semantic_analysis_limits(&self) -> SemanticAnalysisLimits {
        self.semantic_analysis_limits
    }
}

impl Default for CompilationOptions {
    fn default() -> Self {
        Self::new(
            WorkerBudget::default(),
            ProductKind::Library,
            SelectedTarget::default(),
        )
    }
}

/// Package identity, source inputs, and options used to load a [`Compilation`](crate::Compilation).
#[derive(Debug, Eq, PartialEq)]
pub struct CompilationRequest {
    package_identity: PackageIdentity,
    options: CompilationOptions,
    sources: Vec<SourceInput>,
    dependency_interfaces: Vec<DependencyInterfaceInput>,
    package_interface_export: Option<PackageInterfaceExportRequest>,
}

/// Package-layer identity inputs for the current library product's interface export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInterfaceExportRequest {
    identity: PackageInterfaceIdentity,
    language_revision: InterfaceLanguageRevision,
}

impl PackageInterfaceExportRequest {
    /// Creates an export request from one validated package-layer product identity.
    pub const fn new(
        identity: PackageInterfaceIdentity,
        language_revision: InterfaceLanguageRevision,
    ) -> Self {
        Self {
            identity,
            language_revision,
        }
    }

    /// Returns the selected package, product, and public-surface identity.
    pub const fn identity(&self) -> &PackageInterfaceIdentity {
        &self.identity
    }

    /// Returns the language semantic revision used by the product.
    pub const fn language_revision(&self) -> InterfaceLanguageRevision {
        self.language_revision
    }
}

/// One package-selected compiled dependency interface supplied to a compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyInterfaceInput {
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    artifact_path: Arc<Path>,
    dependency_span: Option<SourceSpan>,
    bytes: Arc<[u8]>,
    validation_policy: InterfaceValidationPolicy,
}

impl DependencyInterfaceInput {
    /// Creates one immutable untrusted dependency-interface input.
    pub fn new(
        package: PackageIdentity,
        product: InterfaceProductIdentity,
        artifact_path: impl Into<PathBuf>,
        bytes: impl Into<Arc<[u8]>>,
        validation_policy: InterfaceValidationPolicy,
    ) -> Self {
        Self {
            package,
            product,
            artifact_path: Arc::from(artifact_path.into()),
            dependency_span: None,
            bytes: bytes.into(),
            validation_policy,
        }
    }

    /// Returns a copy correlated with the source dependency that selected this artifact.
    pub const fn with_dependency_span(mut self, dependency_span: SourceSpan) -> Self {
        self.dependency_span = Some(dependency_span);

        self
    }

    /// Returns the package identity selected by package resolution.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the product identity selected by package resolution.
    pub const fn product(&self) -> &InterfaceProductIdentity {
        &self.product
    }

    /// Returns the stable artifact path supplied by package resolution.
    pub fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    /// Returns the source dependency that selected this artifact, when available.
    pub const fn dependency_span(&self) -> Option<SourceSpan> {
        self.dependency_span
    }

    /// Returns the immutable untrusted artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn shared_bytes(&self) -> Arc<[u8]> {
        // Validation retains immutable request bytes, so sharing avoids copying dependency files.
        Arc::clone(&self.bytes)
    }

    /// Returns the compatibility and resource policy for this artifact.
    pub const fn validation_policy(&self) -> InterfaceValidationPolicy {
        self.validation_policy
    }
}

impl CompilationRequest {
    /// Creates a compilation request for one source package with default options.
    pub fn new(package_identity: PackageIdentity, sources: Vec<SourceInput>) -> Self {
        Self::with_options(package_identity, sources, CompilationOptions::default())
    }

    /// Creates a compilation request for one source package with explicit options.
    pub fn with_options(
        package_identity: PackageIdentity,
        sources: Vec<SourceInput>,
        options: CompilationOptions,
    ) -> Self {
        Self {
            package_identity,
            options,
            sources,
            dependency_interfaces: Vec::new(),
            package_interface_export: None,
        }
    }

    /// Returns a copy owning the selected compiled dependency interfaces.
    pub fn with_dependency_interfaces(
        mut self,
        dependency_interfaces: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Self {
        self.dependency_interfaces = dependency_interfaces.into_iter().collect();

        self
    }

    /// Returns a copy configured to produce one library interface.
    pub fn with_package_interface_export(
        mut self,
        package_interface_export: PackageInterfaceExportRequest,
    ) -> Self {
        self.package_interface_export = Some(package_interface_export);

        self
    }

    /// Returns the source package identity selected for this compilation.
    pub const fn package_identity(&self) -> &PackageIdentity {
        &self.package_identity
    }

    /// Returns the compilation options.
    pub const fn options(&self) -> &CompilationOptions {
        &self.options
    }

    /// Returns the source inputs in request order.
    pub fn sources(&self) -> &[SourceInput] {
        &self.sources
    }

    /// Returns selected dependency interfaces in package-request order.
    pub fn dependency_interfaces(&self) -> &[DependencyInterfaceInput] {
        &self.dependency_interfaces
    }

    /// Returns the selected current-product interface export, when requested.
    pub const fn package_interface_export(&self) -> Option<&PackageInterfaceExportRequest> {
        self.package_interface_export.as_ref()
    }

    /// Consumes the request into its parts.
    pub fn into_parts(
        self,
    ) -> (
        PackageIdentity,
        CompilationOptions,
        Vec<SourceInput>,
        Vec<DependencyInterfaceInput>,
        Option<PackageInterfaceExportRequest>,
    ) {
        (
            self.package_identity,
            self.options,
            self.sources,
            self.dependency_interfaces,
            self.package_interface_export,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    };
    use bray_checker::SemanticAnalysisLimits;
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, ProductKind};

    use crate::worker::WorkerBudget;

    use super::{
        CompilationOptions, CompilationRequest, DependencyInterfaceInput,
        PackageInterfaceExportRequest,
    };

    #[test]
    fn compilation_requests_hold_sources_and_options() {
        let limits = SemanticAnalysisLimits::new(17, 23);

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            crate::SelectedTarget::baseline(),
        )
        .with_semantic_analysis_limits(limits);

        let source = SourceInput::virtual_text(
            SourceIdentity::new(1),
            "source-1",
            SourceVersion::new(1),
            "one",
        );

        let Some(package_identity) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity must be valid");
        };

        let Some(export_identity) = bray_package_interface::PackageInterfaceIdentity::try_new(
            package_identity.clone(),
            product("library"),
            bray_package_interface::InterfaceProductKind::Library,
            "public-v1",
        ) else {
            panic!("test export identity must be valid");
        };

        let export =
            PackageInterfaceExportRequest::new(export_identity, InterfaceLanguageRevision::new(0));

        let request = CompilationRequest::with_options(
            package_identity.clone(),
            vec![source],
            options.clone(),
        )
        .with_dependency_interfaces([dependency_interface()])
        .with_package_interface_export(export);

        assert_eq!(request.package_identity(), &package_identity);
        assert_eq!(request.options(), &options);
        assert_eq!(request.options().product_kind(), ProductKind::Library);
        assert_eq!(request.options().semantic_analysis_limits(), limits);
        assert_eq!(request.sources().len(), 1);
        assert_eq!(request.dependency_interfaces().len(), 1);

        assert_eq!(
            request
                .package_interface_export()
                .map(PackageInterfaceExportRequest::identity)
                .map(bray_package_interface::PackageInterfaceIdentity::public_surface),
            Some("public-v1")
        );
    }

    fn dependency_interface() -> DependencyInterfaceInput {
        let Some(package) = PackageIdentity::try_new("test.dependency") else {
            panic!("test dependency package identity must be valid");
        };

        DependencyInterfaceInput::new(
            package,
            product("main"),
            "test.dependency.brayi",
            Arc::<[u8]>::from([1, 2, 3]),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
    }

    fn product(value: &str) -> InterfaceProductIdentity {
        InterfaceProductIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }
}
