// rust-style: allow(module-too-large, reason = "project compilation is one stateful dependency traversal and tool-invocation orchestrator with shared artifact publication state")

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use bray_diagnostics::{
    DiagnosticBag, DiagnosticIoErrorKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation, DiagnosticProjectSelectionProblem,
};
use bray_project::{ProjectGraph, ProjectPackage, ProjectProduct};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use bray_target::{NativeTarget, TargetIdentity, TargetOutputKind, TargetOutputName};
use bray_tooling::OutputFormat;

use crate::tack::error::{operation_diagnostics, selection_diagnostics, tool_execution_failure};
use crate::tack::model::{TackBuildConfiguration, TackInspection, TackProfileConfiguration};
use crate::tack::progress::{
    BuildProgressAction, BuildProgressPackage, BuildProgressPlan, BuildProgressSession,
};
use crate::tack::project::PlannedProduct;
use crate::tack::tool::{Tool, ToolExecutor, ToolOutput, ToolRequest};
use crate::tack::toolchain::Toolchain;

pub(crate) struct ProjectCompiler<'project> {
    workspace_root: &'project Path,
    graph: &'project ProjectGraph,
    toolchain: &'project Toolchain,
    worker_count: usize,
    output_format: OutputFormat,
    profile: Option<&'project TackProfileConfiguration>,
    native_link_inputs: Vec<String>,
    executor: &'project dyn ToolExecutor,
    interfaces: BTreeMap<(ProductIdentity, TargetIdentity), CheckedInterface>,
}

struct CheckedInterface {
    path: PathBuf,
    source_input_digest: Option<[u8; 32]>,
    _operation: bray_emitter::ManagedOperation,
}

pub(crate) struct ProductBuild {
    outputs: Vec<ToolOutput>,
    executable: Option<PathBuf>,
}

impl ProductBuild {
    pub(crate) fn into_parts(self) -> (Vec<ToolOutput>, Option<PathBuf>) {
        (self.outputs, self.executable)
    }
}

impl<'project> ProjectCompiler<'project> {
    pub(crate) fn new(
        workspace_root: &'project Path,
        graph: &'project ProjectGraph,
        toolchain: &'project Toolchain,
        worker_count: usize,
        output_format: OutputFormat,
        profile: Option<&'project TackProfileConfiguration>,
        executor: &'project dyn ToolExecutor,
    ) -> Self {
        Self {
            workspace_root,
            graph,
            toolchain,
            worker_count,
            output_format,
            profile,
            native_link_inputs: Vec::new(),
            executor,
            interfaces: BTreeMap::new(),
        }
    }

    pub(crate) fn with_native_link_inputs(mut self, native_link_inputs: Vec<String>) -> Self {
        self.native_link_inputs = native_link_inputs;

        self
    }

    pub(crate) fn check(
        &mut self,
        planned: &PlannedProduct,
    ) -> Result<Vec<ToolOutput>, DiagnosticBag> {
        let product = self.project_product(planned)?.clone();
        let mut outputs = Vec::new();

        if !self.check_dependencies(&product, planned.target(), &mut outputs, None, None)? {
            return Ok(outputs);
        }

        let output = self.run_compiler(
            &product,
            planned.target(),
            CompilerAction::Check {
                interface: None,
                expected_source_digest: None,
            },
            None,
        )?;

        outputs.push(output);

        Ok(outputs)
    }

    pub(crate) fn build(
        &mut self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
        progress: Option<&BuildProgressSession<'_>>,
    ) -> Result<ProductBuild, DiagnosticBag> {
        self.build_product(planned, configuration, progress, None)
    }

    pub(crate) fn build_test(
        &mut self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
        progress: Option<&BuildProgressSession<'_>>,
        evidence: &crate::tack::identity::TestProductBuildEvidence,
    ) -> Result<(ProductBuild, bray_emitter::ProductBuildIdentity), DiagnosticBag> {
        let build = self.build_product(planned, configuration, progress, Some(evidence))?;

        Ok((build, evidence.identity().clone()))
    }

    fn build_product(
        &mut self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
        progress: Option<&BuildProgressSession<'_>>,
        evidence: Option<&crate::tack::identity::TestProductBuildEvidence>,
    ) -> Result<ProductBuild, DiagnosticBag> {
        let product = self.project_product(planned)?.clone();
        let mut outputs = Vec::new();

        if !self.check_dependencies(&product, planned.target(), &mut outputs, progress, evidence)? {
            return Ok(ProductBuild {
                outputs,
                executable: None,
            });
        }

        let output_root = self.graph.output_root().beneath(self.workspace_root);

        let relative_output_directory = self.relative_output_directory(
            product.identity(),
            planned.target_name(),
            configuration,
        );

        let output_directory = output_root.join(&relative_output_directory);

        std::fs::create_dir_all(&output_directory).map_err(|error| {
            operation_diagnostics(DiagnosticProjectCommandFailure::Io {
                operation: DiagnosticProjectOperation::ProductOutputDirectory,
                path: output_directory.clone(),
                error: DiagnosticIoErrorKind::from(error.kind()),
            })
        })?;

        let test_catalog = product.kind() == ProductKind::Test;

        let output = self.run_compiler(
            &product,
            planned.target(),
            CompilerAction::Build {
                output_root,
                output_directory: relative_output_directory,
                configuration,
                test_catalog,
                build_identity: evidence.map(|evidence| evidence.identity().clone()),
                expected_source_digest: evidence
                    .and_then(|evidence| evidence.source_inputs(product.identity())),
            },
            progress,
        )?;

        let success = output.success();

        outputs.push(output);

        let executable = if success {
            self.executable_path(&output_directory, &product, planned.target())?
        } else {
            None
        };

        Ok(ProductBuild {
            outputs,
            executable,
        })
    }

    pub(crate) fn build_progress_plan(
        &self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
    ) -> Result<BuildProgressPlan, DiagnosticBag> {
        self.build_progress_plan_with_evidence(planned, configuration, None)
    }

    pub(crate) fn test_build_progress_plan(
        &self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
    ) -> Result<
        (
            BuildProgressPlan,
            crate::tack::identity::TestProductBuildEvidence,
        ),
        DiagnosticBag,
    > {
        let product = self.project_product(planned)?;
        let products = self.reusable_test_products(product, planned.target())?;

        let evidence = crate::tack::identity::test_product_build_evidence(
            self.workspace_root,
            self.graph,
            &products,
            planned.target(),
            configuration,
            &self.native_link_inputs,
            self.toolchain,
            self.executor,
        )?;

        let plan = self.build_progress_plan_with_evidence(
            planned,
            configuration,
            Some(&evidence),
        )?;

        Ok((plan, evidence))
    }

    fn build_progress_plan_with_evidence(
        &self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
        evidence: Option<&crate::tack::identity::TestProductBuildEvidence>,
    ) -> Result<BuildProgressPlan, DiagnosticBag> {
        let product = self.project_product(planned)?;
        let dependencies = self.transitive_dependencies(product, planned.target())?;

        let output_directory =
            self.output_directory(product.identity(), planned.target_name(), configuration);

        let executable = self.executable_path(&output_directory, product, planned.target())?;

        let artifact = executable
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| product.identity().name().to_owned());

        let packages =
            self.progress_packages(product, planned.target(), &dependencies, evidence)?;

        let product_identity = format!(
            "{}/{}",
            product.identity().package().as_str(),
            product.identity().name()
        );

        Ok(BuildProgressPlan::new(
            product_identity,
            configuration,
            artifact,
            display_path(&output_directory, self.workspace_root),
            executable
                .as_deref()
                .map(|path| display_path(path, self.workspace_root))
                .unwrap_or_else(|| display_path(&output_directory, self.workspace_root)),
            packages,
        ))
    }

    pub(crate) fn test_product_identity(
        &self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
    ) -> Result<bray_emitter::ProductBuildIdentity, DiagnosticBag> {
        let product = self.project_product(planned)?;
        let products = self.reusable_test_products(product, planned.target())?;

        crate::tack::identity::test_product_identity(
            self.workspace_root,
            self.graph,
            &products,
            planned.target(),
            configuration,
            &self.native_link_inputs,
            self.toolchain,
            self.executor,
        )
    }

    pub(crate) fn inspect(
        &mut self,
        planned: &PlannedProduct,
        inspection: TackInspection,
        source_id: u32,
        offset: Option<u32>,
        source: bool,
    ) -> Result<Vec<ToolOutput>, DiagnosticBag> {
        let product = self.project_product(planned)?.clone();
        let mut outputs = Vec::new();

        if !self.check_dependencies(&product, planned.target(), &mut outputs, None, None)? {
            return Ok(outputs);
        }

        let output = self.run_compiler(
            &product,
            planned.target(),
            CompilerAction::Inspect {
                inspection,
                source_id,
                offset,
                source,
            },
            None,
        )?;

        outputs.push(output);

        Ok(outputs)
    }

    fn check_dependencies(
        &mut self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        outputs: &mut Vec<ToolOutput>,
        progress: Option<&BuildProgressSession<'_>>,
        evidence: Option<&crate::tack::identity::TestProductBuildEvidence>,
    ) -> Result<bool, DiagnosticBag> {
        let dependencies = self.direct_dependencies(product, target)?;

        for dependency in dependencies {
            if !self.ensure_dependency(&dependency, target, outputs, progress, evidence)? {
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
        progress: Option<&BuildProgressSession<'_>>,
        evidence: Option<&crate::tack::identity::TestProductBuildEvidence>,
    ) -> Result<bool, DiagnosticBag> {
        let key = (identity.clone(), target.clone());

        if self.interface_matches(identity, target, evidence) {
            return Ok(true);
        }

        let source_inputs = evidence.and_then(|evidence| evidence.source_inputs(identity));

        let product = self.project_product_by_identity(identity)?.clone();

        if !product.targets().contains(target) {
            return Err(selection_diagnostics(
                DiagnosticProjectSelectionProblem::ProductTargetUnavailable {
                    product: identity.name().to_owned(),
                    target: target.as_str().to_owned(),
                },
            ));
        }

        if !self.check_dependencies(&product, target, outputs, progress, evidence)? {
            return Ok(false);
        }

        let operation = bray_emitter::ManagedOperation::begin(
            &self.graph.output_root().beneath(self.workspace_root),
            identity,
            target,
            &|| false,
        )
        .map_err(storage_diagnostics)?;

        let interface = operation
            .directory()
            .join(format!("{}.brayi", identity.name()));

        let output = self.run_compiler(
            &product,
            target,
            CompilerAction::Check {
                interface: Some(interface.clone()),
                expected_source_digest: source_inputs,
            },
            progress,
        )?;

        let success = output.success();

        outputs.push(output);

        if success {
            self.interfaces.insert(
                key,
                CheckedInterface {
                    path: interface,
                    source_input_digest: source_inputs,
                    _operation: operation,
                },
            );
        }

        Ok(success)
    }

    fn run_compiler(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        action: CompilerAction,
        progress: Option<&BuildProgressSession<'_>>,
    ) -> Result<ToolOutput, DiagnosticBag> {
        let package = product.identity().package().as_str();

        if let Some(progress) = progress {
            progress.start_package(package);
        }

        let mut request = ToolRequest::new(Tool::Compiler, self.workspace_root);

        let runtime = action
            .requires_runtime(product.kind())
            .then(|| self.toolchain.runtime_metadata(target));

        let uses_standard_library_source = self.graph.source_authority().is_standard_library();

        request
            .arg("--cpu-count")
            .arg(self.worker_count.to_string())
            .arg("--format")
            .arg(self.output_format.as_str())
            .arg("--package")
            .arg(product.identity().package().as_str())
            .arg("--source-package")
            .arg(source_package(product)?.as_str())
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
            .arg(target.as_str());

        if consumes_standard_library(product) {
            let root = self.toolchain.standard_library_root().into_os_string();

            let root_argument = if uses_standard_library_source {
                action
                    .requires_runtime(product.kind())
                    .then_some("--standard-library-provider-root")
            } else {
                Some("--standard-library-root")
            };

            if let Some(root_argument) = root_argument {
                request.arg(root_argument).arg(root);
            }
        }

        if uses_standard_library_source {
            request.arg("--standard-library-source");
        }

        for binding in product.platform_services() {
            request.arg("--platform-service").arg(format!(
                "{}={}",
                binding.role().as_str(),
                binding.dotted_path()
            ));
        }

        for input in &self.native_link_inputs {
            request.arg("--native-link-input").arg(input);
        }

        if let Some(profile) = self.profile {
            request.arg("--profile").arg(profile.mode().as_str());

            if let Some(output_directory) = profile.output_directory() {
                request
                    .arg("--profile-output")
                    .arg(self.profile_report_path(output_directory, product, target, &action)?);
            }
        }

        for dependency in self.dependencies(product, target)? {
            request
                .arg("--dependency-product")
                .arg(format!(
                    "{}/{}",
                    dependency.identity.package().as_str(),
                    dependency.identity.name()
                ))
                .arg("--dependency-interface")
                .arg(dependency.interface.into_os_string())
                .arg("--dependency-implementation")
                .arg(dependency.implementation.into_os_string());
        }

        if let Some(digest) = action.expected_source_digest() {
            let digest = serde_json::to_string(&digest)
                .unwrap_or_else(|error| panic!("source input digest must serialize: {error:?}"));

            request.arg("--expected-source-digest").arg(digest);
        }

        action.add_arguments(&mut request, product, runtime);

        request.args(
            product
                .sources()
                .iter()
                .map(|source| source.beneath(self.workspace_root).into_os_string()),
        );

        let output = self
            .executor
            .capture(request)
            .map(|output| {
                output.with_subject(format!(
                    "{}/{}",
                    product.identity().package().as_str(),
                    product.identity().name()
                ))
            })
            .map_err(|error| {
                operation_diagnostics(tool_execution_failure(
                    DiagnosticProjectOperation::CompilerProcess,
                    error,
                ))
            });

        if let Some(progress) = progress {
            progress.finish_package_work(
                package,
                1,
                output.as_ref().is_ok_and(ToolOutput::success),
            );
        }

        output
    }

    fn profile_report_path(
        &self,
        output_directory: &Path,
        product: &ProjectProduct,
        target: &TargetIdentity,
        action: &CompilerAction,
    ) -> Result<PathBuf, DiagnosticBag> {
        let output_directory = if output_directory.is_absolute() {
            output_directory.to_path_buf()
        } else {
            self.workspace_root.join(output_directory)
        };

        std::fs::create_dir_all(&output_directory).map_err(|error| {
            operation_diagnostics(DiagnosticProjectCommandFailure::Io {
                operation: DiagnosticProjectOperation::CompilerProfileOutputDirectory,
                path: output_directory.clone(),
                error: DiagnosticIoErrorKind::from(error.kind()),
            })
        })?;

        Ok(output_directory.join(format!(
            "{}-{}-{}-{}.json",
            product.identity().package().as_str(),
            product.identity().name(),
            target.as_str(),
            action.profile_name(),
        )))
    }

    fn transitive_dependencies(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
    ) -> Result<BTreeSet<ProductIdentity>, DiagnosticBag> {
        let mut dependencies = BTreeSet::new();

        let mut pending = self.direct_dependencies(product, target)?;

        while let Some(identity) = pending.pop() {
            if !dependencies.insert(identity.clone()) {
                continue;
            }

            let package = self.project_package(identity.package())?;

            let product = package
                .products()
                .iter()
                .find(|product| product.identity() == &identity)
                .ok_or_else(|| {
                    selection_diagnostics(DiagnosticProjectSelectionProblem::NoMatchingProduct(
                        identity.name().to_owned(),
                    ))
                })?;

            pending.extend(self.direct_dependencies(product, target)?);
        }

        Ok(dependencies)
    }

    fn reusable_test_products(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
    ) -> Result<Vec<ProjectProduct>, DiagnosticBag> {
        let mut identities = self.transitive_dependencies(product, target)?;
        identities.insert(product.identity().clone());

        identities
            .iter()
            .map(|identity| self.project_product_by_identity(identity).cloned())
            .collect()
    }

    fn progress_packages(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
        dependencies: &BTreeSet<ProductIdentity>,
        evidence: Option<&crate::tack::identity::TestProductBuildEvidence>,
    ) -> Result<Vec<BuildProgressPackage>, DiagnosticBag> {
        let mut packages = Vec::new();

        for package in self.graph.packages() {
            let mut units = 0_u64;
            let mut action = BuildProgressAction::CheckInterface;

            for candidate in package.products() {
                let is_root = candidate.identity() == product.identity();

                let is_pending_dependency = dependencies.contains(candidate.identity())
                    && !self.interface_matches(candidate.identity(), target, evidence);

                if !is_root && !is_pending_dependency {
                    continue;
                }

                units = units.checked_add(1).ok_or_else(|| {
                    operation_diagnostics(DiagnosticProjectCommandFailure::CapacityExceeded(
                        DiagnosticProjectOperation::WorkflowUnitCount,
                    ))
                })?;

                if is_root {
                    action = BuildProgressAction::ProduceArtifacts;
                }
            }

            if units == 0 {
                continue;
            }

            packages.push(BuildProgressPackage::new(
                package.identity().as_str(),
                package.path().as_str(),
                units,
                action,
            ));
        }

        Ok(packages)
    }

    fn interface_matches(
        &self,
        identity: &ProductIdentity,
        target: &TargetIdentity,
        evidence: Option<&crate::tack::identity::TestProductBuildEvidence>,
    ) -> bool {
        let key = (identity.clone(), target.clone());
        let source_input_digest = evidence.and_then(|evidence| evidence.source_inputs(identity));

        self.interfaces
            .get(&key)
            .is_some_and(|interface| interface.source_input_digest == source_input_digest)
    }

    fn dependencies(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
    ) -> Result<Vec<DependencyArtifact>, DiagnosticBag> {
        self.direct_dependencies(product, target)?
            .into_iter()
            .map(|identity| {
                let key = (identity.clone(), target.clone());

                let path = self.interfaces.get(&key).ok_or_else(|| {
                    operation_diagnostics(DiagnosticProjectCommandFailure::MissingResult(
                        DiagnosticProjectOperation::DependencyInterface,
                    ))
                })?;

                Ok(DependencyArtifact {
                    identity,
                    interface: path.path.clone(),
                    implementation: path.path.with_extension("brayimpl"),
                })
            })
            .collect()
    }

    fn direct_dependencies(
        &self,
        product: &ProjectProduct,
        target: &TargetIdentity,
    ) -> Result<Vec<ProductIdentity>, DiagnosticBag> {
        let uses_standard_library_source = self.graph.source_authority().is_standard_library();

        let mut dependencies = product
            .dependencies()
            .iter()
            .filter(|dependency| dependency.is_active_for(target))
            .map(|dependency| dependency.product().clone())
            .collect::<BTreeSet<_>>();

        dependencies.extend(
            product
                .tested_library()
                .filter(|library| {
                    uses_standard_library_source || !is_public_standard_library(library)
                })
                .cloned(),
        );

        Ok(dependencies.into_iter().collect())
    }

    fn project_product(&self, planned: &PlannedProduct) -> Result<&ProjectProduct, DiagnosticBag> {
        let package = self.project_package(planned.package())?;

        package
            .products()
            .iter()
            .find(|product| product.identity().name() == planned.product_name())
            .ok_or_else(|| {
                selection_diagnostics(DiagnosticProjectSelectionProblem::NoMatchingProduct(
                    planned.product_name().to_owned(),
                ))
            })
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
            .ok_or_else(|| {
                selection_diagnostics(DiagnosticProjectSelectionProblem::NoMatchingProduct(
                    identity.name().to_owned(),
                ))
            })
    }

    fn project_package(
        &self,
        identity: &PackageIdentity,
    ) -> Result<&ProjectPackage, DiagnosticBag> {
        self.graph.package(identity).ok_or_else(|| {
            selection_diagnostics(DiagnosticProjectSelectionProblem::UnknownPackage(
                identity.as_str().to_owned(),
            ))
        })
    }

    fn output_directory(
        &self,
        product: &ProductIdentity,
        target_name: &str,
        configuration: TackBuildConfiguration,
    ) -> PathBuf {
        self.graph
            .output_root()
            .beneath(self.workspace_root)
            .join(self.relative_output_directory(product, target_name, configuration))
    }

    pub(crate) fn lock_published_product(
        &self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
    ) -> Result<bray_emitter::PublishedProductReadGuard, DiagnosticBag> {
        let product = self.project_product(planned)?;

        let (destination, relative) = self.managed_destination(product, planned, configuration)?;

        bray_emitter::lock_published_product(destination, product.identity()).map_err(|error| {
            storage_diagnostics(error.into_storage_error(&PathBuf::from(relative)))
        })
    }

    pub(crate) fn retain_test_product(
        &self,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
    ) -> Result<bray_emitter::RetainedProductGeneration, DiagnosticBag> {
        let product = self.project_product(planned)?;

        let (destination, relative) = self.managed_destination(product, planned, configuration)?;

        bray_emitter::retain_published_generation(destination, product.identity(), &|| false)
            .map_err(|error| {
                storage_diagnostics(error.into_storage_error(&PathBuf::from(relative)))
            })
    }

    fn managed_destination(
        &self,
        product: &ProjectProduct,
        planned: &PlannedProduct,
        configuration: TackBuildConfiguration,
    ) -> Result<(bray_emitter::ManagedFilesystemDestination, String), DiagnosticBag> {
        let output_root = self.graph.output_root().beneath(self.workspace_root);

        let relative = self.relative_output_directory(
            product.identity(),
            planned.target_name(),
            configuration,
        );

        let directory = bray_emitter::ManagedOutputDirectory::try_new(relative.replace('\\', "/"))
            .ok_or_else(|| {
                operation_diagnostics(DiagnosticProjectCommandFailure::Io {
                    operation: DiagnosticProjectOperation::ProductOutputDirectory,
                    path: PathBuf::from(&relative),
                    error: DiagnosticIoErrorKind::InvalidInput,
                })
            })?;

        Ok((
            bray_emitter::ManagedFilesystemDestination::new(output_root, directory),
            relative,
        ))
    }

    fn relative_output_directory(
        &self,
        product: &ProductIdentity,
        target_name: &str,
        configuration: TackBuildConfiguration,
    ) -> String {
        format!(
            "{}/{}/{}",
            target_name,
            configuration.directory_name(),
            product.package().as_str()
        )
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

        let native = NativeTarget::for_identity(target).ok_or_else(|| {
            selection_diagnostics(DiagnosticProjectSelectionProblem::UnknownTarget(
                target.as_str().to_owned(),
            ))
        })?;

        let name =
            TargetOutputName::for_native(native.object_format(), TargetOutputKind::Executable)
                .file_name(product.identity().name())
                .ok_or_else(|| {
                    operation_diagnostics(DiagnosticProjectCommandFailure::MissingResult(
                        DiagnosticProjectOperation::ExecutableOutputName,
                    ))
                })?;

        Ok(Some(output_directory.join(name)))
    }
}

fn source_package(product: &ProjectProduct) -> Result<PackageIdentity, DiagnosticBag> {
    if product.tested_library().is_none() {
        return Ok(product.identity().package().clone());
    }

    PackageIdentity::try_new(format!(
        "{}.tests.{}",
        product.identity().package().as_str(),
        product.identity().name()
    ))
    .ok_or_else(|| {
        operation_diagnostics(DiagnosticProjectCommandFailure::Invariant(
            DiagnosticProjectOperation::TestSourcePackageIdentity,
        ))
    })
}

fn consumes_standard_library(product: &ProjectProduct) -> bool {
    !is_public_standard_library(product.identity())
}

fn is_public_standard_library(product: &ProductIdentity) -> bool {
    product.package().as_str() == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
        && product.name() == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PRODUCT_IDENTITY
}

fn display_path(path: &Path, workspace_root: &Path) -> String {
    path.strip_prefix(workspace_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

struct DependencyArtifact {
    identity: ProductIdentity,
    interface: PathBuf,
    implementation: PathBuf,
}

enum CompilerAction {
    Check {
        interface: Option<PathBuf>,
        expected_source_digest: Option<[u8; 32]>,
    },
    Build {
        output_root: PathBuf,
        output_directory: String,
        configuration: TackBuildConfiguration,
        test_catalog: bool,
        build_identity: Option<bray_emitter::ProductBuildIdentity>,
        expected_source_digest: Option<[u8; 32]>,
    },
    Inspect {
        inspection: TackInspection,
        source_id: u32,
        offset: Option<u32>,
        source: bool,
    },
}

impl CompilerAction {
    const fn profile_name(&self) -> &'static str {
        match self {
            Self::Check { .. } => "check",
            Self::Build { .. } => "build",
            Self::Inspect { .. } => "inspect",
        }
    }

    fn requires_runtime(&self, product_kind: ProductKind) -> bool {
        matches!(self, Self::Build { .. })
            && matches!(product_kind, ProductKind::Executable | ProductKind::Test)
    }

    const fn expected_source_digest(&self) -> Option<[u8; 32]> {
        match self {
            Self::Check {
                expected_source_digest,
                ..
            }
            | Self::Build {
                expected_source_digest,
                ..
            } => *expected_source_digest,
            Self::Inspect { .. } => None,
        }
    }

    fn add_arguments(
        self,
        request: &mut ToolRequest,
        product: &ProjectProduct,
        runtime: Option<PathBuf>,
    ) {
        match self {
            Self::Check { interface, .. } => {
                request.arg("check");

                if let Some(interface) = interface {
                    request
                        .arg("--emit-interface")
                        .arg(interface.into_os_string());
                }
            }
            Self::Build {
                output_root,
                output_directory,
                configuration,
                test_catalog,
                build_identity,
                expected_source_digest: _,
            } => {
                request
                    .arg("build")
                    .arg("--output")
                    .arg(output_root.into_os_string())
                    .arg("--managed-output-directory")
                    .arg(output_directory);

                if configuration == TackBuildConfiguration::Release {
                    request.arg("--release");
                }

                if let Some(runtime) = runtime {
                    request
                        .arg("--runtime-artifact")
                        .arg(runtime.into_os_string());
                }

                if test_catalog {
                    request.arg("--test-catalog");
                }

                if let Some(identity) = build_identity {
                    let identity = serde_json::to_string(&identity).unwrap_or_else(|error| {
                        panic!("reusable build identity must serialize: {error:?}")
                    });

                    request.arg("--build-identity").arg(identity);
                }

                for kind in product.outputs() {
                    request.arg("--artifact").arg(artifact_text(*kind));
                }
            }
            Self::Inspect {
                inspection,
                source_id,
                offset,
                source,
            } => {
                request.arg("inspect").arg(inspection.command_text());

                if matches!(
                    inspection,
                    TackInspection::Bound | TackInspection::Lowered | TackInspection::Mir
                ) {
                    request.arg("--source-id").arg(source_id.to_string());

                    if let Some(offset) = offset {
                        request.arg("--offset").arg(offset.to_string());
                    }
                }

                if source && inspection == TackInspection::Mir {
                    request.arg("--source");
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
        TargetOutputKind::PackageImplementation => "package-implementation",
        TargetOutputKind::DependencyMetadata => "dependency-metadata",
        TargetOutputKind::TestCatalog => "test-catalog",
        TargetOutputKind::Executable => "executable",
        TargetOutputKind::StaticLibrary => "static-library",
        TargetOutputKind::SharedLibrary => "shared-library",
        TargetOutputKind::LinkedCompanion => "linked-companion",
    }
}

fn storage_diagnostics(error: bray_emitter::StorageError) -> DiagnosticBag {
    DiagnosticBag::single(error.into_diagnostic(
        bray_diagnostics::DiagnosticId::new(0),
        bray_diagnostics::SeverityKind::Error,
    ))
}
