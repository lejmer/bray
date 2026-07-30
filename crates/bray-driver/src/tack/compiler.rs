use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_compilation::{
    Compilation, CompilationOptions, DependencyInterfaceInput,
    ProductEmissionInputs, SelectedTarget, WorkerBudget,
};
use bray_diagnostics::DiagnosticBag;
use bray_emitter::{
    ArtifactKind, ArtifactRequirement, EmissionRequest, EmissionStatus, ReplacementPolicy,
    RequestedArtifact, RequestedArtifactDestination,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity,
    InterfaceValidationPolicy, encode_package_interface,
};
use bray_project::{ProjectGraph, ProjectPackage, ProjectProduct};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::{TargetIdentity, TargetOutputKind};

use crate::command::{
    DriverBackend, compilation_request_from_file_arguments,
};
use crate::run::{
    baseline_target_outputs, load_compilation, native_linker,
    package_interface_export_request,
};
use crate::tack::error::{
    operation_diagnostics, selection_diagnostics, unavailable_diagnostics,
};
use crate::tack::project::PlannedProduct;

#[derive(Debug)]
pub(crate) struct ProductBuildOutcome {
    diagnostics: DiagnosticBag,
    executable: Option<PathBuf>,
}

impl ProductBuildOutcome {
    pub(crate) const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(crate) fn executable(&self) -> Option<&Path> {
        self.executable.as_deref()
    }
}

pub(crate) struct ProjectCompiler<'project> {
    workspace_root: &'project Path,
    graph: &'project ProjectGraph,
    worker_budget: WorkerBudget,
    interfaces: BTreeMap<(ProductIdentity, TargetIdentity), Arc<[u8]>>,
    built: BTreeSet<(ProductIdentity, TargetIdentity)>,
}

impl<'project> ProjectCompiler<'project> {
    pub(crate) fn new(
        workspace_root: &'project Path,
        graph: &'project ProjectGraph,
        worker_budget: WorkerBudget,
    ) -> Self {
        Self {
            workspace_root,
            graph,
            worker_budget,
            interfaces: BTreeMap::new(),
            built: BTreeSet::new(),
        }
    }

    pub(crate) fn check(
        &mut self,
        planned: &PlannedProduct,
    ) -> Result<Compilation, DiagnosticBag> {
        let product = self.project_product(planned)?;
        let selected_target = self.selected_target(planned.target())?;
        let compilation = self.compile_product(&product, selected_target, false)?;
        let diagnostics = compilation.check_diagnostics().clone();

        if diagnostics.has_errors() {
            return Err(diagnostics);
        }

        Ok(compilation)
    }

    pub(crate) fn build(
        &mut self,
        planned: &PlannedProduct,
    ) -> Result<ProductBuildOutcome, DiagnosticBag> {
        let product = self.project_product(planned)?;

        self.build_product(&product, planned.target(), planned.target_name())
    }

    pub(crate) fn compilation_for_inspection(
        &mut self,
        planned: &PlannedProduct,
    ) -> Result<Compilation, DiagnosticBag> {
        let product = self.project_product(planned)?;
        let selected_target = self.selected_target(planned.target())?;

        self.compile_product(&product, selected_target, false)
    }

    fn build_product(
        &mut self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        target_name: &str,
    ) -> Result<ProductBuildOutcome, DiagnosticBag> {
        let key = (product.identity().clone(), target.clone());

        if self.built.contains(&key) {
            return Ok(ProductBuildOutcome {
                diagnostics: DiagnosticBag::new(),
                executable: executable_path(
                    self.output_directory(product, target_name),
                    product,
                ),
            });
        }

        let package = self.project_package(product.identity().package())?;

        let dependencies: Vec<_> = package
            .dependencies()
            .iter()
            .map(|dependency| dependency.product().clone())
            .collect();

        let mut diagnostics = DiagnosticBag::new();

        for dependency in dependencies {
            let dependency_product = self.project_product_by_identity(&dependency)?;

            if !dependency_product.targets().contains(target) {
                return Err(selection_diagnostics(format!(
                    "{}/{}",
                    dependency_product.identity().name(),
                    target.as_str()
                )));
            }

            match self.build_product(&dependency_product, target, target_name) {
                Ok(outcome) => {
                    diagnostics = diagnostics.merged(outcome.diagnostics());
                }
                Err(error) => return Err(diagnostics.merged(&error)),
            }
        }

        let selected_target = self.selected_target(target)?;
        let compilation = self.compile_product(product, selected_target.clone(), true)?;
        diagnostics = diagnostics.merged(compilation.check_diagnostics());

        if diagnostics.has_errors() {
            return Err(diagnostics);
        }

        self.retain_interface(product, target, &compilation)?;

        let output_directory = self.output_directory(product, target_name);

        std::fs::create_dir_all(&output_directory)
            .map_err(|_| operation_diagnostics("create_output_directory"))?;

        let emission_diagnostics = self.emit_product(
            product,
            selected_target,
            &compilation,
            output_directory.clone(),
        )?;

        diagnostics = diagnostics.merged(&emission_diagnostics);

        self.built.insert(key);

        Ok(ProductBuildOutcome {
            diagnostics,
            executable: executable_path(output_directory, product),
        })
    }

    fn compile_product(
        &mut self,
        product: &ProjectProduct,
        selected_target: SelectedTarget,
        codegen: bool,
    ) -> Result<Compilation, DiagnosticBag> {
        let package = self.project_package(product.identity().package())?;

        let dependencies: Vec<_> = package
            .dependencies()
            .iter()
            .map(|dependency| dependency.product().clone())
            .collect();

        let mut dependency_inputs = Vec::with_capacity(dependencies.len());

        for dependency in dependencies {
            let bytes = self.interface_for(&dependency, selected_target.profile().identity())?;

            let interface_product = InterfaceProductIdentity::try_new(dependency.name())
                .ok_or_else(|| operation_diagnostics("dependency_product_identity"))?;

            dependency_inputs.push(DependencyInterfaceInput::new(
                dependency.package().clone(),
                interface_product,
                self.interface_path(&dependency, selected_target.profile().identity())?,
                bytes,
                InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
            ));
        }

        let options = CompilationOptions::new(
            self.worker_budget,
            product.kind(),
            selected_target,
        );

        let files: Vec<PathBuf> = product
            .sources()
            .iter()
            .map(|source| source.beneath(self.workspace_root))
            .collect();

        let mut request = compilation_request_from_file_arguments(
            product.identity().package().clone(),
            files,
            options,
        )?
        .with_dependency_interfaces(dependency_inputs);

        if product.kind() == ProductKind::Library {
            request = request.with_package_interface_export(
                package_interface_export_request(product.identity().clone()),
            );
        }

        load_compilation(request, codegen.then_some(DriverBackend::Llvm))
            .ok_or_else(|| operation_diagnostics("load_compilation"))
    }

    fn interface_for(
        &mut self,
        identity: &ProductIdentity,
        target: &TargetIdentity,
    ) -> Result<Arc<[u8]>, DiagnosticBag> {
        let key = (identity.clone(), target.clone());

        if let Some(bytes) = self.interfaces.get(&key) {
            return Ok(Arc::clone(bytes));
        }

        let product = self.project_product_by_identity(identity)?;

        if product.kind() != ProductKind::Library || !product.targets().contains(target) {
            return Err(selection_diagnostics(format!(
                "{}/{}",
                identity.name(),
                target.as_str()
            )));
        }

        let selected_target = self.selected_target(target)?;
        let compilation = self.compile_product(&product, selected_target, false)?;
        let diagnostics = compilation.check_diagnostics().clone();

        if diagnostics.has_errors() {
            return Err(diagnostics);
        }

        self.retain_interface(&product, target, &compilation)?;

        self.interfaces
            .get(&key)
            .map(Arc::clone)
            .ok_or_else(|| operation_diagnostics("retain_dependency_interface"))
    }

    fn retain_interface(
        &mut self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        compilation: &Compilation,
    ) -> Result<(), DiagnosticBag> {
        if product.kind() != ProductKind::Library {
            return Ok(());
        }

        let Some(bundle) = compilation.package_interface_export_bundle() else {
            return Err(operation_diagnostics("package_interface_export"));
        };

        let bundle = bundle
            .as_ref()
            .map_err(|_| operation_diagnostics("package_interface_export"))?;

        let artifact = encode_package_interface(bundle)
            .map_err(|_| operation_diagnostics("package_interface_encoding"))?;

        self.interfaces.insert(
            (product.identity().clone(), target.clone()),
            artifact.shared_bytes(),
        );

        Ok(())
    }

    fn emit_product(
        &self,
        product: &ProjectProduct,
        selected_target: SelectedTarget,
        compilation: &Compilation,
        output_directory: PathBuf,
    ) -> Result<DiagnosticBag, DiagnosticBag> {
        let outputs = baseline_target_outputs(
            &selected_target,
            product.outputs().iter().copied(),
        );

        let artifacts = product.outputs().iter().copied().map(|kind| {
            RequestedArtifact::new(
                ArtifactKind::from(kind),
                ArtifactRequirement::Required,
            )
        });

        let linked = product.outputs().iter().any(|output| {
            matches!(
                output,
                TargetOutputKind::Executable
                    | TargetOutputKind::StaticLibrary
                    | TargetOutputKind::SharedLibrary
                    | TargetOutputKind::LinkedCompanion
            )
        });

        let linker = if linked {
            Some(
                native_linker()
                    .ok_or_else(|| unavailable_diagnostics("native_linker"))?,
            )
        } else {
            None
        };

        let product_identity = product.identity().clone();

        let native = match &linker {
            Some(linker) if linked => Some(
                compilation
                    .native_product_facts(
                        product_identity.clone(),
                        None,
                        [],
                        linker,
                    )
                    .map_err(|_| operation_diagnostics("native_product_facts"))?,
            ),
            _ => None,
        };

        let request = EmissionRequest::try_new(
            product_identity,
            product.kind(),
            native
                .as_ref()
                .and_then(|facts| facts.executable_host().cloned()),
            selected_target.profile().identity().clone(),
            RequestedArtifactDestination::FilesystemDirectory(output_directory),
            artifacts,
            ReplacementPolicy::ReplaceExisting,
        )
        .map_err(|_| operation_diagnostics("emission_request"))?;

        let mut inputs = ProductEmissionInputs::new(&outputs);

        if let (Some(native), Some(linker)) = (native.as_ref(), linker.as_ref()) {
            inputs = inputs.with_native_product(native, linker);
        }

        let outcome = compilation
            .emit_product(request, inputs)
            .map_err(|error| error.diagnostics().clone())?;

        match outcome.status() {
            EmissionStatus::Complete => Ok(outcome.diagnostics().clone()),
            EmissionStatus::Failed(_) | EmissionStatus::Cancelled => {
                let diagnostics = outcome.diagnostics().clone();

                if diagnostics.is_empty() {
                    Err(operation_diagnostics("product_emission"))
                } else {
                    Err(diagnostics)
                }
            }
        }
    }

    fn selected_target(
        &self,
        identity: &TargetIdentity,
    ) -> Result<SelectedTarget, DiagnosticBag> {
        let baseline = SelectedTarget::baseline();

        if baseline.profile().identity() != identity {
            return Err(unavailable_diagnostics(identity.as_str()));
        }

        Ok(baseline)
    }

    fn project_product(
        &self,
        planned: &PlannedProduct,
    ) -> Result<ProjectProduct, DiagnosticBag> {
        let package = self.project_package(planned.package())?;

        package
            .products()
            .iter()
            .find(|product| product.identity().name() == planned.product_name())
            .cloned()
            .ok_or_else(|| selection_diagnostics(planned.product_name()))
    }

    fn project_product_by_identity(
        &self,
        identity: &ProductIdentity,
    ) -> Result<ProjectProduct, DiagnosticBag> {
        let package = self.project_package(identity.package())?;

        package
            .products()
            .iter()
            .find(|product| product.identity() == identity)
            .cloned()
            .ok_or_else(|| selection_diagnostics(identity.name()))
    }

    fn project_package(
        &self,
        identity: &PackageIdentity,
    ) -> Result<&ProjectPackage, DiagnosticBag> {
        self.graph
            .package(identity)
            .ok_or_else(|| selection_diagnostics(identity.as_str()))
    }

    fn output_directory(
        &self,
        product: &ProjectProduct,
        target_name: &str,
    ) -> PathBuf {
        self.graph
            .output_root()
            .beneath(self.workspace_root)
            .join(target_name)
            .join(product.identity().package().as_str())
            .join(product.identity().name())
    }

    fn interface_path(
        &self,
        product: &ProductIdentity,
        target: &TargetIdentity,
    ) -> Result<PathBuf, DiagnosticBag> {
        let target_name = self
            .graph
            .targets()
            .iter()
            .find(|candidate| candidate.identity() == target)
            .map(bray_project::ProjectTarget::name)
            .ok_or_else(|| selection_diagnostics(target.as_str()))?;

        Ok(self
            .graph
            .output_root()
            .beneath(self.workspace_root)
            .join(target_name)
            .join(product.package().as_str())
            .join(product.name())
            .join(format!("{}.brayi", product.name())))
    }
}

fn executable_path(
    output_directory: PathBuf,
    product: &ProjectProduct,
) -> Option<PathBuf> {
    product
        .outputs()
        .contains(&TargetOutputKind::Executable)
        .then(|| output_directory.join(product.identity().name()))
}
