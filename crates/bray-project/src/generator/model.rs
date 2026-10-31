use std::collections::BTreeMap;
use std::hash::Hasher;
use std::sync::Arc;

use bray_base::StableDigestHasher;

use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::{TargetIdentity, TargetProfile};

use crate::ProjectPath;

/// Package-qualified identity of a declared source generator.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratorIdentity {
    pub(crate) package: PackageIdentity,
    pub(crate) name: Arc<str>,
}

impl GeneratorIdentity {
    /// Returns the owning package.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }
    /// Returns the package-local generator name.
    pub fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn qualified_name(&self) -> String {
        format!("{}/{}", self.package.as_str(), self.name)
    }
}

/// Exact workspace executable product and its execution target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratorTool {
    pub(crate) product: ProductIdentity,
    pub(crate) target: TargetIdentity,
}

impl GeneratorTool {
    /// Identifies one exact host artifact supplied by the caller.
    pub const fn new(product: ProductIdentity, target: TargetIdentity) -> Self {
        Self { product, target }
    }
    /// Returns the selected executable product.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }
    /// Returns the target for which the tool must be executable.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }
    pub(crate) fn qualified_name(&self) -> String {
        format!(
            "{}/{}/{}",
            self.product.package().as_str(),
            self.product.name(),
            self.target.as_str()
        )
    }
}

/// Closed permissions for declared file access and an explicit network grant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratorPermissions {
    pub(crate) network: bool,
}

impl GeneratorPermissions {
    /// Returns whether network access is explicitly granted. Ambient filesystem access is always denied.
    pub const fn network(&self) -> bool {
        self.network
    }
}

/// An immutable normalized generator declaration owned by a package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectGenerator {
    pub(crate) identity: GeneratorIdentity,
    pub(crate) tool: GeneratorTool,
    pub(crate) inputs: Arc<[ProjectPath]>,
    pub(crate) generated_inputs: Arc<[(GeneratorIdentity, ProjectPath)]>,
    pub(crate) outputs: Arc<[ProjectPath]>,
    pub(crate) environment: BTreeMap<Arc<str>, Arc<str>>,
    pub(crate) permissions: GeneratorPermissions,
}

impl ProjectGenerator {
    /// Returns the package-qualified generator identity.
    pub const fn identity(&self) -> &GeneratorIdentity {
        &self.identity
    }
    /// Returns the exact tool product and execution target.
    pub const fn tool(&self) -> &GeneratorTool {
        &self.tool
    }
    /// Returns ordered package-relative file inputs.
    pub fn inputs(&self) -> &[ProjectPath] {
        &self.inputs
    }
    /// Returns ordered references to generated outputs in this package.
    pub fn generated_inputs(&self) -> &[(GeneratorIdentity, ProjectPath)] {
        &self.generated_inputs
    }
    /// Returns declared outputs in portable path order.
    pub fn outputs(&self) -> &[ProjectPath] {
        &self.outputs
    }
    /// Returns exact environment values, without inherited process variables.
    pub const fn environment(&self) -> &BTreeMap<Arc<str>, Arc<str>> {
        &self.environment
    }
    /// Returns the complete file and network permission contract.
    pub const fn permissions(&self) -> GeneratorPermissions {
        self.permissions
    }
}

/// Content-identified tool bytes and the identity of the toolchain that produced them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratorExecutable {
    bytes: Arc<[u8]>,
    content_identity: [u8; 32],
    toolchain_identity: [u8; 32],
}

impl GeneratorExecutable {
    /// Supplies exact executable bytes and a content identity for the selected toolchain.
    /// The host must ensure the artifact implements the declared workspace product.
    pub fn new(bytes: Arc<[u8]>, toolchain_identity: [u8; 32]) -> Self {
        let mut hasher = StableDigestHasher::new();

        hasher.write(&bytes);

        Self {
            bytes,
            content_identity: hasher.finalize(),
            toolchain_identity,
        }
    }

    /// Returns the complete content identity of the supplied executable bytes.
    pub const fn content_identity(&self) -> &[u8; 32] {
        &self.content_identity
    }
    /// Returns executable bytes to materialize inside the sandbox.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the host-supplied toolchain content identity.
    pub const fn toolchain_identity(&self) -> &[u8; 32] {
        &self.toolchain_identity
    }
}

/// Logical input identity within the sandbox, separate from writable output identities.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GeneratorInputIdentity {
    /// Package-relative file input.
    Source(ProjectPath),
    /// Output from a generator in the same package and for the same selected target.
    Generated {
        /// Producing generator.
        generator: GeneratorIdentity,
        /// Generator-relative output identity.
        output: ProjectPath,
    },
}

/// Exact immutable bytes supplied to a generator under one logical identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratorInput {
    pub(crate) identity: GeneratorInputIdentity,
    pub(crate) bytes: Arc<[u8]>,
}

impl GeneratorInput {
    /// Returns the logical input identity, without ambient host paths.
    pub const fn identity(&self) -> &GeneratorInputIdentity {
        &self.identity
    }
    /// Returns the captured file contents.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Complete request for a sandboxed generator. All process inputs are explicit.
#[derive(Debug)]
pub struct GeneratorRequest<'a> {
    pub(crate) generator: &'a ProjectGenerator,
    pub(crate) executable: &'a GeneratorExecutable,
    pub(crate) target: &'a TargetProfile,
    pub(crate) inputs: &'a [GeneratorInput],
    pub(crate) policy_identity: &'a [u8; 32],
}

impl<'a> GeneratorRequest<'a> {
    /// Returns the immutable declaration, including output, environment, and permission inventories.
    pub const fn generator(&self) -> &'a ProjectGenerator {
        self.generator
    }
    /// Returns exact host-tool bytes and their toolchain identity.
    pub const fn executable(&self) -> &'a GeneratorExecutable {
        self.executable
    }
    /// Returns all language-defined properties of the selected generation target.
    pub const fn target(&self) -> &'a TargetProfile {
        self.target
    }
    /// Returns ordered immutable file snapshots. Inputs and outputs occupy separate namespaces.
    pub const fn inputs(&self) -> &'a [GeneratorInput] {
        self.inputs
    }
    /// Returns the host's content identity for the enforced sandbox implementation and policy.
    pub const fn policy_identity(&self) -> &'a [u8; 32] {
        self.policy_identity
    }
}

/// Deterministic identity of every declared input and execution policy for one invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratorCacheKey(pub(crate) [u8; 32]);

impl GeneratorCacheKey {
    /// Returns the complete 256-bit content identity.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Immutable generated source snapshot with deterministic provenance.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GeneratedSource {
    pub(crate) generator: GeneratorIdentity,
    pub(crate) target: TargetIdentity,
    pub(crate) output: ProjectPath,
    pub(crate) text: Arc<str>,
    pub(crate) cache_key: GeneratorCacheKey,
}

impl GeneratedSource {
    /// Returns the generator that owns this source.
    pub const fn generator(&self) -> &GeneratorIdentity {
        &self.generator
    }
    /// Returns the selected generation target.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }
    /// Returns the generator-relative output identity, never a source-tree path.
    pub const fn output(&self) -> &ProjectPath {
        &self.output
    }
    /// Returns the captured immutable source text.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Returns the identity of the exact executable, inputs, environment, target and sandbox policy.
    pub const fn cache_key(&self) -> GeneratorCacheKey {
        self.cache_key
    }
}

/// Dependency-first work across target products, host tools, and generated sources.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProjectBuildNode {
    /// Build one exact product for its declared target.
    Product {
        /// Selected product.
        product: ProductIdentity,
        /// Product target.
        target: TargetIdentity,
    },
    /// Execute one generator after its host tool and generated inputs.
    Generator {
        /// Selected generator.
        generator: GeneratorIdentity,
        /// Generated-source target.
        target: TargetIdentity,
    },
}
