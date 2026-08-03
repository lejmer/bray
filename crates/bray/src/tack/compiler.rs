use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bray_diagnostics::DiagnosticBag;
use bray_project::{ProjectGraph, ProjectPackage, ProjectProduct};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetIdentity, TargetOutputKind, TargetOutputName};
use bray_tooling::OutputFormat;

use crate::tack::error::{operation_diagnostics, selection_diagnostics};
use crate::tack::model::TackInspection;
use crate::tack::project::PlannedProduct;
use crate::tack::tool::{Tool, ToolExecutor, ToolOutput, ToolRequest};
use crate::tack::toolchain::Toolchain;

pub(crate) struct ProjectCompiler<'project> {
    workspace_root: &'project Path,
    graph: &'project ProjectGraph,
    toolchain: &'project Toolchain,
    worker_count: usize,
    output_format: OutputFormat,
    executor: &'project dyn ToolExecutor,
    interfaces: BTreeMap<(ProductIdentity, TargetIdentity), PathBuf>,
    checked: BTreeSet<(ProductIdentity, TargetIdentity)>,
}

impl<'project> ProjectCompiler<'project> {
    pub(crate) fn new(
        workspace_root: &'project Path,
        graph: &'project ProjectGraph,
        toolchain: &'project Toolchain,
        worker_count: usize,
        output_format: OutputFormat,
        executor: &'project dyn ToolExecutor,
    ) -> Self {
        Self {
            workspace_root,
            graph,
            toolchain,
            worker_count,
            output_format,
            executor,
            interfaces: BTreeMap::new(),
            checked: BTreeSet::new(),
        }
    }

    pub(crate) fn check(
        &mut self,
        planned: &PlannedProduct,
    ) -> Result<Vec<ToolOutput>, DiagnosticBag> {
        let product = self.project_product(planned)?.clone();
        let mut outputs = Vec::new();

        if !self.check_dependencies(&product, planned.target(), &mut outputs)? {
            return Ok(outputs);
        }

        let output = self.run_compiler(
            &product,
            planned.target(),
            CompilerAction::Check { interface: None },
        )?;

        outputs.push(output);

        Ok(outputs)
    }

    pub(crate) fn build(
        &mut self,
        planned: &PlannedProduct,
    ) -> Result<(Vec<ToolOutput>, Option<PathBuf>), DiagnosticBag> {
        let product = self.project_product(planned)?.clone();
        let mut outputs = Vec::new();

        if !self.check_dependencies(&product, planned.target(), &mut outputs)? {
            return Ok((outputs, None));
        }

        let output_directory = self.output_directory(product.identity(), planned.target_name());

        std::fs::create_dir_all(&output_directory)
            .map_err(|_| operation_diagnostics("product_output_directory"))?;

        let output = self.run_compiler(
            &product,
            planned.target(),
            CompilerAction::Build {
                output: output_directory.clone(),
            },
        )?;

        outputs.push(output);

        let executable = self.executable_path(&output_directory, &product, planned.target())?;

        Ok((outputs, executable))
    }

    pub(crate) fn inspect(
        &mut self,
        planned: &PlannedProduct,
        inspection: TackInspection,
        source_id: u32,
        offset: Option<u32>,
    ) -> Result<Vec<ToolOutput>, DiagnosticBag> {
        let product = self.project_product(planned)?.clone();
        let mut outputs = Vec::new();

        if !self.check_dependencies(&product, planned.target(), &mut outputs)? {
            return Ok(outputs);
        }

        let output = self.run_compiler(
            &product,
            planned.target(),
            CompilerAction::Inspect {
                inspection,
                source_id,
                offset,
            },
        )?;

        outputs.push(output);

        Ok(outputs)
    }

    fn check_dependencies(
        &mut self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        outputs: &mut Vec<ToolOutput>,
    ) -> Result<bool, DiagnosticBag> {
        let dependencies = self
            .project_package(product.identity().package())?
            .dependencies()
            .iter()
            .map(|dependency| dependency.product().clone())
            .collect::<Vec<_>>();

        for dependency in dependencies {
            if !self.ensure_dependency(&dependency, target, outputs)? {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn ensure_dependency(
        &mut self,
        identity: &ProductIdentity,
        target: &TargetIdentity,
        outputs: &mut Vec<ToolOutput>,
    ) -> Result<bool, DiagnosticBag> {
        let key = (identity.clone(), target.clone());

        if self.checked.contains(&key) {
            return Ok(true);
        }

        let product = self.project_product_by_identity(identity)?.clone();

        if !product.targets().contains(target) {
            return Err(selection_diagnostics(format!(
                "{}/{}",
                identity.name(),
                target.as_str()
            )));
        }

        if !self.check_dependencies(&product, target, outputs)? {
            return Ok(false);
        }

        let interface = self.cache_interface_path(identity, target);

        let Some(parent) = interface.parent() else {
            return Err(operation_diagnostics("interface_cache_path"));
        };

        std::fs::create_dir_all(parent)
            .map_err(|_| operation_diagnostics("interface_cache_directory"))?;

        let output = self.run_compiler(
            &product,
            target,
            CompilerAction::Check {
                interface: Some(interface.clone()),
            },
        )?;

        let success = output.success();

        outputs.push(output);

        if success {
            self.interfaces.insert(key.clone(), interface);
            self.checked.insert(key);
        }

        Ok(success)
    }

    fn run_compiler(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        action: CompilerAction,
    ) -> Result<ToolOutput, DiagnosticBag> {
        let mut request = ToolRequest::new(Tool::Compiler, self.workspace_root);

        let runtime = action
            .requires_runtime(product.kind())
            .then(|| self.toolchain.runtime_metadata(target));

        request
            .arg("--cpu-count")
            .arg(self.worker_count.to_string())
            .arg("--format")
            .arg(self.output_format.as_str())
            .arg("--package")
            .arg(product.identity().package().as_str())
            .arg("--package-version")
            .arg(
                self.project_package(product.identity().package())?
                    .version()
                    .to_string(),
            )
            .arg("--product")
            .arg(product.identity().name())
            .arg("--product-kind")
            .arg(product_kind_text(product.kind()))
            .arg("--target")
            .arg(target.as_str())
            .arg("--standard-library-root")
            .arg(self.toolchain.standard_library_root().into_os_string());

        for dependency in self.dependencies(product, target)? {
            request
                .arg("--dependency-product")
                .arg(format!(
                    "{}/{}",
                    dependency.identity.package().as_str(),
                    dependency.identity.name()
                ))
                .arg("--dependency-interface")
                .arg(dependency.path.into_os_string());
        }

        action.add_arguments(&mut request, product, runtime);

        request.args(
            product
                .sources()
                .iter()
                .map(|source| source.beneath(self.workspace_root).into_os_string()),
        );

        self.executor
            .capture(request)
            .map_err(|_| operation_diagnostics("compiler_process"))
    }

    fn dependencies(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
    ) -> Result<Vec<DependencyArtifact>, DiagnosticBag> {
        let package = self.project_package(product.identity().package())?;

        package
            .dependencies()
            .iter()
            .map(|dependency| {
                let identity = dependency.product();
                let key = (identity.clone(), target.clone());

                let path = self
                    .interfaces
                    .get(&key)
                    .ok_or_else(|| operation_diagnostics("dependency_interface"))?;

                Ok(DependencyArtifact {
                    identity: identity.clone(),
                    path: path.clone(),
                })
            })
            .collect()
    }

    fn project_product(&self, planned: &PlannedProduct) -> Result<&ProjectProduct, DiagnosticBag> {
        let package = self.project_package(planned.package())?;

        package
            .products()
            .iter()
            .find(|product| product.identity().name() == planned.product_name())
            .ok_or_else(|| selection_diagnostics(planned.product_name()))
    }

    fn project_product_by_identity(
        &self,
        identity: &ProductIdentity,
    ) -> Result<&ProjectProduct, DiagnosticBag> {
        let package = self.project_package(identity.package())?;

        package
            .products()
            .iter()
            .find(|product| product.identity() == identity)
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

    fn output_directory(&self, product: &ProductIdentity, target_name: &str) -> PathBuf {
        self.graph
            .output_root()
            .beneath(self.workspace_root)
            .join(target_name)
            .join(product.package().as_str())
            .join(product.name())
    }

    fn cache_interface_path(&self, product: &ProductIdentity, target: &TargetIdentity) -> PathBuf {
        self.graph
            .output_root()
            .beneath(self.workspace_root)
            .join("cache")
            .join("interfaces")
            .join(target.as_str())
            .join(product.package().as_str())
            .join(format!("{}.brayi", product.name()))
    }

    fn executable_path(
        &self,
        output_directory: &Path,
        product: &ProjectProduct,
        target: &TargetIdentity,
    ) -> Result<Option<PathBuf>, DiagnosticBag> {
        if !product.outputs().contains(&TargetOutputKind::Executable) {
            return Ok(None);
        }

        let native = NativeTarget::for_identity(target)
            .ok_or_else(|| selection_diagnostics(target.as_str()))?;

        let name =
            TargetOutputName::for_native(native.object_format(), TargetOutputKind::Executable)
                .file_name(product.identity().name())
                .ok_or_else(|| operation_diagnostics("executable_output_name"))?;

        Ok(Some(output_directory.join(name)))
    }
}

struct DependencyArtifact {
    identity: ProductIdentity,
    path: PathBuf,
}

enum CompilerAction {
    Check {
        interface: Option<PathBuf>,
    },
    Build {
        output: PathBuf,
    },
    Inspect {
        inspection: TackInspection,
        source_id: u32,
        offset: Option<u32>,
    },
}

impl CompilerAction {
    fn requires_runtime(&self, product_kind: ProductKind) -> bool {
        matches!(self, Self::Build { .. })
            && matches!(product_kind, ProductKind::Executable | ProductKind::Test)
    }

    fn add_arguments(
        self,
        request: &mut ToolRequest,
        product: &ProjectProduct,
        runtime: Option<PathBuf>,
    ) {
        match self {
            Self::Check { interface } => {
                request.arg("check");

                if let Some(interface) = interface {
                    request
                        .arg("--emit-interface")
                        .arg(interface.into_os_string());
                }
            }
            Self::Build { output } => {
                request
                    .arg("build")
                    .arg("--output")
                    .arg(output.into_os_string());

                if let Some(runtime) = runtime {
                    request
                        .arg("--runtime-artifact")
                        .arg(runtime.into_os_string());
                }

                for kind in product.outputs() {
                    request.arg("--artifact").arg(artifact_text(*kind));
                }
            }
            Self::Inspect {
                inspection,
                source_id,
                offset,
            } => {
                request
                    .arg("inspect")
                    .arg(inspection.command_text())
                    .arg("--source-id")
                    .arg(source_id.to_string());

                if let Some(offset) = offset {
                    request.arg("--offset").arg(offset.to_string());
                }
            }
        }
    }
}

fn product_kind_text(kind: ProductKind) -> &'static str {
    match kind {
        ProductKind::Executable => "executable",
        ProductKind::Library => "library",
        ProductKind::Test => "test",
    }
}

fn artifact_text(kind: TargetOutputKind) -> &'static str {
    match kind {
        TargetOutputKind::Assembly => "assembly",
        TargetOutputKind::BackendIr => "backend-ir",
        TargetOutputKind::BackendBitcode => "backend-bitcode",
        TargetOutputKind::RelocatableObject => "relocatable-object",
        TargetOutputKind::ExecutableModule => "executable-module",
        TargetOutputKind::DebugCompanion => "debug-companion",
        TargetOutputKind::PackageInterface => "package-interface",
        TargetOutputKind::DependencyMetadata => "dependency-metadata",
        TargetOutputKind::Executable => "executable",
        TargetOutputKind::StaticLibrary => "static-library",
        TargetOutputKind::SharedLibrary => "shared-library",
        TargetOutputKind::LinkedCompanion => "linked-companion",
    }
}
