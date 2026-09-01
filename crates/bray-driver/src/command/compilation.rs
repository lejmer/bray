use std::path::{Path, PathBuf};

use bray_base::NonEmptySharedStr;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticProjectSelectionProblem, SeverityKind,
};
use bray_messages::command_help as help;
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
};
use bray_runtime_interface::{PlatformServiceBinding, PlatformServiceRole};
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, PackageIdentity, PackageVersion, ProductIdentity,
    ProductKind,
};
use bray_target::NativeTarget;
use clap::{Args, ValueEnum};

/// One compiled dependency interface selected by a compiler invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverDependencyInterface {
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    path: PathBuf,
    implementation_path: Option<PathBuf>,
}

impl DriverDependencyInterface {
    fn try_from_arguments(
        identity: &str,
        path: PathBuf,
        implementation_path: Option<PathBuf>,
    ) -> Option<Self> {
        let (package, product) = identity.rsplit_once('/')?;

        Some(Self {
            package: PackageIdentity::try_new(package)?,
            product: InterfaceProductIdentity::try_new(product)?,
            path,
            implementation_path,
        })
    }

    /// Returns the dependency package identity.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the package-local dependency product identity.
    pub const fn product(&self) -> &InterfaceProductIdentity {
        &self.product
    }

    /// Returns the compiled interface artifact path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn load(&self) -> Result<bray_compilation::DependencyInterfaceInput, DiagnosticBag> {
        let bytes = std::fs::read(&self.path)
            .map_err(|error| dependency_artifact_read_diagnostics(&self.path, error.kind()))?;

        let mut input = bray_compilation::DependencyInterfaceInput::new(
            self.package.clone(),
            self.product.clone(),
            self.path.clone(),
            bytes,
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        if let Some(path) = &self.implementation_path {
            let bytes = std::fs::read(path)
                .map_err(|error| dependency_artifact_read_diagnostics(path, error.kind()))?;

            let artifact = bray_package_interface::PackageImplementationArtifact::try_from_bytes(
                bytes,
                bray_package_interface::InterfaceValidationLimits::default(),
            )
            .map_err(|error| dependency_implementation_diagnostics(self, path, error))?;

            input = input.with_implementation_artifact(path, std::sync::Arc::new(artifact));
        }

        Ok(input)
    }
}

fn dependency_artifact_read_diagnostics(path: &Path, kind: std::io::ErrorKind) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SourceFileReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path(path))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
            kind,
        )))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::SourceFileMustBeReadable,
        )),
    )
}

fn dependency_implementation_diagnostics(
    dependency: &DriverDependencyInterface,
    path: &Path,
    error: bray_package_interface::InterfaceValidationError,
) -> DiagnosticBag {
    DiagnosticBag::single(
        error.into_diagnostic(DiagnosticId::new(0)).with_note(
            DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
                .with_arg(DiagnosticArg::expected_package_identity(
                    dependency.package().as_str(),
                ))
                .with_arg(DiagnosticArg::expected_product_identity(
                    dependency.product().as_str(),
                ))
                .with_arg(DiagnosticArg::artifact_path(path)),
        ),
    )
}

/// Exact package product and dependency context for one compiler invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverCompilationConfiguration {
    product: ProductIdentity,
    source_package: PackageIdentity,
    package_version: PackageVersion,
    product_kind: ProductKind,
    target: NativeTarget,
    dependencies: Vec<DriverDependencyInterface>,
    platform_services: Vec<PlatformServiceBinding>,
    native_link_inputs: Vec<NativeLinkRequirement>,
}

impl DriverCompilationConfiguration {
    /// Creates an exact compiler-facing product context.
    pub fn new(
        product: ProductIdentity,
        source_package: PackageIdentity,
        package_version: PackageVersion,
        product_kind: ProductKind,
        target: NativeTarget,
        mut dependencies: Vec<DriverDependencyInterface>,
        mut platform_services: Vec<PlatformServiceBinding>,
        mut native_link_inputs: Vec<NativeLinkRequirement>,
    ) -> Self {
        dependencies.sort_by(|left, right| {
            left.package()
                .cmp(right.package())
                .then_with(|| left.product().cmp(right.product()))
        });

        platform_services.sort_unstable();
        native_link_inputs.sort_unstable();
        native_link_inputs.dedup();

        Self {
            product,
            source_package,
            package_version,
            product_kind,
            target,
            dependencies,
            platform_services,
            native_link_inputs,
        }
    }

    /// Returns the exact package-product identity.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the package identity assigned to declarations in the source inputs.
    pub const fn source_package(&self) -> &PackageIdentity {
        &self.source_package
    }

    /// Returns the semantic version of the selected package.
    pub const fn package_version(&self) -> &PackageVersion {
        &self.package_version
    }

    /// Returns the language-level product category.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns the selected compilation target.
    pub const fn target(&self) -> NativeTarget {
        self.target
    }

    /// Returns dependency interface inputs in canonical identity order.
    pub fn dependencies(&self) -> &[DriverDependencyInterface] {
        &self.dependencies
    }

    /// Returns private platform-service declaration bindings in role order.
    pub fn platform_services(&self) -> &[PlatformServiceBinding] {
        &self.platform_services
    }

    /// Returns native libraries made available by the build host.
    pub fn native_link_inputs(&self) -> &[NativeLinkRequirement] {
        &self.native_link_inputs
    }
}

#[derive(Args, Debug)]
pub(crate) struct CliCompilationOptions {
    #[arg(
        long,
        global = true,
        value_name = "IDENTITY",
        default_value = "command.line",
        help = help::PACKAGE
    )]
    package: String,
    #[arg(
        long = "source-package",
        global = true,
        value_name = "IDENTITY",
        help = help::SOURCE_PACKAGE
    )]
    source_package: Option<String>,
    #[arg(
        long = "package-version",
        global = true,
        value_name = "VERSION",
        default_value = "0.0.0",
        help = help::PACKAGE_VERSION
    )]
    package_version: String,
    #[arg(long, global = true, value_name = "NAME", help = help::PRODUCT)]
    product: Option<String>,
    #[arg(
        long = "product-kind",
        global = true,
        value_enum,
        default_value = "library",
        help = help::PRODUCT_KIND
    )]
    product_kind: CliProductKind,
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "x86_64-unknown-linux-gnu",
        value_name = "TRIPLE",
        help = help::TARGET
    )]
    target: CliTarget,
    #[arg(
        long = "dependency-product",
        global = true,
        value_name = "PACKAGE/PRODUCT",
        help = help::DEPENDENCY_PRODUCT
    )]
    dependency_products: Vec<String>,
    #[arg(
        long = "dependency-interface",
        global = true,
        value_name = "PATH",
        help = help::DEPENDENCY_INTERFACE
    )]
    dependency_interfaces: Vec<PathBuf>,
    #[arg(
        long = "dependency-implementation",
        global = true,
        value_name = "PATH",
        help = help::DEPENDENCY_IMPLEMENTATION
    )]
    dependency_implementations: Vec<PathBuf>,
    #[arg(
        long = "platform-service",
        global = true,
        value_name = "ROLE=DECLARATION",
        hide = true
    )]
    platform_services: Vec<String>,
    #[arg(
        long = "native-link-input",
        global = true,
        value_name = "NAME=KIND",
        value_parser = parse_native_link_input,
        hide = true
    )]
    native_link_inputs: Vec<NativeLinkRequirement>,
}

impl CliCompilationOptions {
    pub(crate) fn into_configuration(
        self,
    ) -> Result<DriverCompilationConfiguration, DiagnosticBag> {
        let package = PackageIdentity::try_new(self.package.clone()).ok_or_else(|| {
            invalid_selection(DiagnosticProjectSelectionProblem::InvalidPackageIdentity(
                self.package.clone(),
            ))
        })?;

        let source_package = self
            .source_package
            .map(|identity| {
                PackageIdentity::try_new(identity.clone()).ok_or_else(|| {
                    invalid_selection(
                        DiagnosticProjectSelectionProblem::InvalidSourcePackageIdentity(identity),
                    )
                })
            })
            .transpose()?
            .unwrap_or_else(|| package.clone());

        let package_version = PackageVersion::try_new(&self.package_version).ok_or_else(|| {
            invalid_selection(DiagnosticProjectSelectionProblem::InvalidPackageVersion(
                self.package_version.clone(),
            ))
        })?;

        let product_name = self
            .product
            .unwrap_or_else(|| self.product_kind.default_product_name().to_owned());

        let product = ProductIdentity::try_new(package, product_name.clone()).ok_or_else(|| {
            invalid_selection(DiagnosticProjectSelectionProblem::InvalidProductIdentity(
                product_name,
            ))
        })?;

        if self.dependency_products.len() != self.dependency_interfaces.len()
            || (!self.dependency_implementations.is_empty()
                && self.dependency_products.len() != self.dependency_implementations.len())
        {
            return Err(invalid_selection(
                DiagnosticProjectSelectionProblem::DependencyArgumentCountMismatch {
                    products: u32::try_from(self.dependency_products.len()).unwrap_or(u32::MAX),
                    interfaces: u32::try_from(self.dependency_interfaces.len()).unwrap_or(u32::MAX),
                    implementations: u32::try_from(self.dependency_implementations.len())
                        .unwrap_or(u32::MAX),
                },
            ));
        }

        let dependencies = self
            .dependency_products
            .iter()
            .zip(self.dependency_interfaces)
            .enumerate()
            .map(|(index, (identity, path))| {
                let implementation_path = self.dependency_implementations.get(index).cloned();

                DriverDependencyInterface::try_from_arguments(identity, path, implementation_path)
                    .ok_or_else(|| {
                        invalid_selection(
                            DiagnosticProjectSelectionProblem::InvalidDependencyProduct(
                                identity.clone(),
                            ),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let platform_services = self
            .platform_services
            .into_iter()
            .map(|binding| {
                parse_platform_service_binding(&binding).ok_or_else(|| {
                    invalid_selection(
                        DiagnosticProjectSelectionProblem::InvalidPlatformServiceBinding(binding),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DriverCompilationConfiguration::new(
            product,
            source_package,
            package_version,
            self.product_kind.into(),
            self.target.into(),
            dependencies,
            platform_services,
            self.native_link_inputs,
        ))
    }
}

fn parse_native_link_input(value: &str) -> Result<NativeLinkRequirement, String> {
    let (name, kind) = value
        .split_once('=')
        .ok_or_else(|| "native link inputs require NAME=KIND".to_owned())?;

    let name = NonEmptySharedStr::try_new(name)
        .ok_or_else(|| "native link input names must be nonempty".to_owned())?;

    let kind = NativeLinkKind::for_name(kind).ok_or_else(|| {
        "native link input kinds must be dynamic, static, system, or framework".to_owned()
    })?;

    Ok(NativeLinkRequirement::new(name, kind))
}

fn parse_platform_service_binding(value: &str) -> Option<PlatformServiceBinding> {
    let (role, declaration) = value.split_once('=')?;

    let role = PlatformServiceRole::from_name(role)?;

    PlatformServiceBinding::try_new(role, declaration)
}

fn invalid_selection(problem: DiagnosticProjectSelectionProblem) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::project_selection_problem(problem)),
    )
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliProductKind {
    #[value(help = help::PRODUCT_LIBRARY)]
    Library,
    #[value(help = help::PRODUCT_EXECUTABLE)]
    Executable,
    #[value(help = help::PRODUCT_TEST)]
    Test,
}

impl CliProductKind {
    const fn default_product_name(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Executable => "application",
            Self::Test => "tests",
        }
    }
}

impl From<CliProductKind> for ProductKind {
    fn from(kind: CliProductKind) -> Self {
        match kind {
            CliProductKind::Library => Self::Library,
            CliProductKind::Executable => Self::Executable,
            CliProductKind::Test => Self::Test,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliTarget {
    #[value(name = "x86_64-unknown-linux-gnu", help = help::TARGET_X86_64_LINUX_GNU)]
    X86_64UnknownLinuxGnu,
    #[value(name = "aarch64-unknown-linux-gnu", help = help::TARGET_AARCH64_LINUX_GNU)]
    Aarch64UnknownLinuxGnu,
    #[value(name = "x86_64-pc-windows-msvc", help = help::TARGET_X86_64_WINDOWS_MSVC)]
    X86_64PcWindowsMsvc,
    #[value(name = "aarch64-pc-windows-msvc", help = help::TARGET_AARCH64_WINDOWS_MSVC)]
    Aarch64PcWindowsMsvc,
    #[value(name = "x86_64-apple-darwin", help = help::TARGET_X86_64_MACOS)]
    X86_64AppleDarwin,
    #[value(name = "aarch64-apple-darwin", help = help::TARGET_AARCH64_MACOS)]
    Aarch64AppleDarwin,
}

impl From<CliTarget> for NativeTarget {
    fn from(target: CliTarget) -> Self {
        match target {
            CliTarget::X86_64UnknownLinuxGnu => Self::X86_64LinuxGnu,
            CliTarget::Aarch64UnknownLinuxGnu => Self::Aarch64LinuxGnu,
            CliTarget::X86_64PcWindowsMsvc => Self::X86_64WindowsMsvc,
            CliTarget::Aarch64PcWindowsMsvc => Self::Aarch64WindowsMsvc,
            CliTarget::X86_64AppleDarwin => Self::X86_64MacOs,
            CliTarget::Aarch64AppleDarwin => Self::Aarch64MacOs,
        }
    }
}
