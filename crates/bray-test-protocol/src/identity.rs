use bray_source::{SourceSpan, SourceVersion};
use bray_symbols::{ModulePathKey, ProductIdentity, SymbolName};

/// The declaration path of one test inside its package product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestDeclarationPath {
    module: ModulePathKey,
    name: SymbolName,
}

impl TestDeclarationPath {
    /// Creates a test declaration path from its module and function name.
    pub const fn new(module: ModulePathKey, name: SymbolName) -> Self {
        Self { module, name }
    }

    /// Returns the module that contains the test declaration.
    pub const fn module(&self) -> &ModulePathKey {
        &self.module
    }

    /// Returns the test function name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }
}

/// Stable identity of one declared test in one package product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestIdentity {
    product: ProductIdentity,
    declaration: TestDeclarationPath,
}

impl TestIdentity {
    /// Creates a test identity from its product and declaration path.
    pub const fn new(product: ProductIdentity, declaration: TestDeclarationPath) -> Self {
        Self {
            product,
            declaration,
        }
    }

    /// Returns the product that owns the test.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the test's fully qualified declaration path.
    pub const fn declaration(&self) -> &TestDeclarationPath {
        &self.declaration
    }
}

/// Exact source revision and range that declares one test entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestSourceAnchor {
    package: [u8; 32],
    span: SourceSpan,
    version: SourceVersion,
}

impl TestSourceAnchor {
    /// Creates a source anchor from an exact span and source revision.
    pub const fn new(package: [u8; 32], span: SourceSpan, version: SourceVersion) -> Self {
        Self { package, span, version }
    }

    /// Returns the product namespace that qualifies the source ID.
    pub const fn package(self) -> [u8; 32] {
        self.package
    }

    /// Returns the declaring source span.
    pub const fn span(self) -> SourceSpan {
        self.span
    }

    /// Returns the declaring source revision.
    pub const fn version(self) -> SourceVersion {
        self.version
    }
}
