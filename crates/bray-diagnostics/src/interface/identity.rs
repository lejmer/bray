/// Product category retained by a package-interface identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceProductKind {
    /// Reusable library product.
    Library,
    /// Executable product.
    Executable,
    /// Test product.
    Test,
}

impl DiagnosticInterfaceProductKind {
    /// Returns the stable machine key for this product category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Executable => "executable",
            Self::Test => "test",
        }
    }
}

/// Exact package-interface identity retained by implementation validation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticPackageInterfaceIdentity {
    package: String,
    version: String,
    product: String,
    product_kind: DiagnosticInterfaceProductKind,
    public_surface: String,
}

impl DiagnosticPackageInterfaceIdentity {
    /// Creates an exact package-interface identity.
    pub fn new(
        package: impl Into<String>,
        version: impl Into<String>,
        product: impl Into<String>,
        product_kind: DiagnosticInterfaceProductKind,
        public_surface: impl Into<String>,
    ) -> Self {
        Self {
            package: package.into(),
            version: version.into(),
            product: product.into(),
            product_kind,
            public_surface: public_surface.into(),
        }
    }

    /// Returns the package identity.
    pub fn package(&self) -> &str {
        &self.package
    }

    /// Returns the package version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the package-local product identity.
    pub fn product(&self) -> &str {
        &self.product
    }

    /// Returns the product category.
    pub const fn product_kind(&self) -> DiagnosticInterfaceProductKind {
        self.product_kind
    }

    /// Returns the public-surface identity.
    pub fn public_surface(&self) -> &str {
        &self.public_surface
    }
}

/// Exact dependency identity retained by implementation validation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticInterfaceDependency {
    package: String,
    product: String,
    content: [u8; 32],
}

impl DiagnosticInterfaceDependency {
    /// Creates an exact dependency identity.
    pub fn new(package: impl Into<String>, product: impl Into<String>, content: [u8; 32]) -> Self {
        Self {
            package: package.into(),
            product: product.into(),
            content,
        }
    }

    /// Returns the dependency package identity.
    pub fn package(&self) -> &str {
        &self.package
    }

    /// Returns the package-local dependency product identity.
    pub fn product(&self) -> &str {
        &self.product
    }

    /// Returns the dependency semantic-content digest.
    pub const fn content(&self) -> &[u8; 32] {
        &self.content
    }
}
