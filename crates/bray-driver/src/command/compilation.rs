use std::path::{Path, PathBuf};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
};
use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};
use clap::{Args, ValueEnum};

/// Compilation target selected at the compiler command boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverTarget {
    /// Bray's deterministic x86-64 Linux baseline.
    X86_64UnknownLinuxGnu,
}

impl DriverTarget {
    /// Returns the compiler target represented by this driver selection.
    pub fn selected_target(self) -> bray_compilation::SelectedTarget {
        match self {
            Self::X86_64UnknownLinuxGnu => bray_compilation::SelectedTarget::baseline(),
        }
    }
}

/// One compiled dependency interface selected by a compiler invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverDependencyInterface {
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    path: PathBuf,
}

impl DriverDependencyInterface {
    fn try_from_arguments(identity: &str, path: PathBuf) -> Option<Self> {
        let (package, product) = identity.rsplit_once('/')?;

        Some(Self {
            package: PackageIdentity::try_new(package)?,
            product: InterfaceProductIdentity::try_new(product)?,
            path,
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

    pub(crate) fn load(
        &self,
    ) -> Result<bray_compilation::DependencyInterfaceInput, DiagnosticBag> {
        let bytes = std::fs::read(&self.path).map_err(|error| {
            DiagnosticBag::single(
                Diagnostic::new(
                    DiagnosticId::new(0),
                    DiagnosticKind::SourceFileReadFailed,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::file_path(&self.path))
                .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                    error.kind(),
                )))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::SourceFileMustBeReadable,
                )),
            )
        })?;

        Ok(bray_compilation::DependencyInterfaceInput::new(
            self.package.clone(),
            self.product.clone(),
            self.path.clone(),
            bytes,
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        ))
    }
}

/// Exact package product and dependency context for one compiler invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverCompilationConfiguration {
    product: ProductIdentity,
    product_kind: ProductKind,
    target: DriverTarget,
    dependencies: Vec<DriverDependencyInterface>,
}

impl DriverCompilationConfiguration {
    /// Creates an exact compiler-facing product context.
    pub fn new(
        product: ProductIdentity,
        product_kind: ProductKind,
        target: DriverTarget,
        mut dependencies: Vec<DriverDependencyInterface>,
    ) -> Self {
        dependencies.sort_by(|left, right| {
            left.package()
                .cmp(right.package())
                .then_with(|| left.product().cmp(right.product()))
        });

        Self {
            product,
            product_kind,
            target,
            dependencies,
        }
    }

    /// Returns the exact package-product identity.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the language-level product category.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns the selected compilation target.
    pub const fn target(&self) -> DriverTarget {
        self.target
    }

    /// Returns dependency interface inputs in canonical identity order.
    pub fn dependencies(&self) -> &[DriverDependencyInterface] {
        &self.dependencies
    }
}

#[derive(Args, Debug)]
pub(crate) struct CliCompilationOptions {
    #[arg(long, global = true, value_name = "IDENTITY", default_value = "command.line")]
    package: String,
    #[arg(long, global = true, value_name = "NAME")]
    product: Option<String>,
    #[arg(long = "product-kind", global = true, value_enum, default_value = "library")]
    product_kind: CliProductKind,
    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "x86_64-unknown-linux-gnu",
        value_name = "TRIPLE"
    )]
    target: CliTarget,
    #[arg(
        long = "dependency-product",
        global = true,
        value_name = "PACKAGE/PRODUCT"
    )]
    dependency_products: Vec<String>,
    #[arg(
        long = "dependency-interface",
        global = true,
        value_name = "PATH"
    )]
    dependency_interfaces: Vec<PathBuf>,
}

impl CliCompilationOptions {
    pub(crate) fn into_configuration(
        self,
    ) -> Result<DriverCompilationConfiguration, DiagnosticBag> {
        let package = PackageIdentity::try_new(self.package.clone())
            .ok_or_else(|| invalid_selection(&self.package))?;

        let product_name = self
            .product
            .unwrap_or_else(|| self.product_kind.default_product_name().to_owned());

        let product = ProductIdentity::try_new(package, product_name.clone())
            .ok_or_else(|| invalid_selection(product_name))?;

        if self.dependency_products.len() != self.dependency_interfaces.len() {
            return Err(invalid_selection("dependency-interface"));
        }

        let dependencies = self
            .dependency_products
            .iter()
            .zip(self.dependency_interfaces)
            .map(|(identity, path)| {
                DriverDependencyInterface::try_from_arguments(identity, path)
                    .ok_or_else(|| invalid_selection(identity))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DriverCompilationConfiguration::new(
            product,
            self.product_kind.into(),
            self.target.into(),
            dependencies,
        ))
    }
}

fn invalid_selection(value: impl Into<String>) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name(value)),
    )
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CliProductKind {
    Library,
    Executable,
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
    #[value(name = "x86_64-unknown-linux-gnu")]
    X86_64UnknownLinuxGnu,
}

impl From<CliTarget> for DriverTarget {
    fn from(target: CliTarget) -> Self {
        match target {
            CliTarget::X86_64UnknownLinuxGnu => Self::X86_64UnknownLinuxGnu,
        }
    }
}
