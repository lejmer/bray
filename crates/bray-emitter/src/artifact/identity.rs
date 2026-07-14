use crate::{ArtifactKind, ProductIdentity};

/// Stable logical identity of one artifact in an immutable emission plan.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactId {
    product: ProductIdentity,
    kind: ArtifactKind,
    ordinal: u32,
}

impl ArtifactId {
    /// Creates an identity from its selected product, category, and deterministic ordinal.
    pub const fn new(product: ProductIdentity, kind: ArtifactKind, ordinal: u32) -> Self {
        Self {
            product,
            kind,
            ordinal,
        }
    }

    /// Returns the product that owns the artifact.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the artifact category.
    pub const fn kind(&self) -> ArtifactKind {
        self.kind
    }

    /// Returns the stable order among same-category artifacts in the product.
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
}

/// Stable identity of one compiler-owned dependency-metadata contribution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyMetadataProducerId(u32);

impl DependencyMetadataProducerId {
    /// Creates an identity from its deterministic order in the emission plan.
    pub const fn new(ordinal: u32) -> Self {
        Self(ordinal)
    }

    /// Returns the deterministic producer order in the emission plan.
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

/// Stable identity of one native linker contribution in an emission plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkerProducerId(u32);

impl LinkerProducerId {
    /// Creates an identity from its deterministic order in the emission plan.
    pub const fn new(ordinal: u32) -> Self {
        Self(ordinal)
    }

    /// Returns the deterministic producer order in the emission plan.
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}
