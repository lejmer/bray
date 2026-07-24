use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;

use crate::{SymbolFactKind, SymbolKey, SymbolKind, SymbolOrdinal};

/// Classifies the semantic work that owns one local symbol region.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LocalSymbolRegionRole {
    /// The executable body of a declared or synthesized callable.
    CallableBody,
    /// An anonymous callable signature and body.
    AnonymousCallable,
    /// A constant expression embedded in a declaration type template.
    EmbeddedConstant,
    /// A module contribution target-selection expression.
    TargetGate,
    /// A declaration-owned checked expression fact.
    DeclarationFact(SymbolFactKind),
}

/// A deterministic key for one local semantic region.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalSymbolRegionKey {
    owner: SymbolKey,
    role: LocalSymbolRegionRole,
    anchors: Arc<[SyntaxAnchor]>,
    ordinal: Option<SymbolOrdinal>,
}

impl LocalSymbolRegionKey {
    /// Creates a region key when at least one stable syntax anchor identifies the region.
    pub fn try_new(
        owner: SymbolKey,
        role: LocalSymbolRegionRole,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
    ) -> Option<Self> {
        let anchors = shared_slice(anchors);

        if anchors.is_empty() {
            return None;
        }

        Some(Self {
            owner,
            role,
            anchors,
            ordinal,
        })
    }

    /// Returns the declaration-surface symbol that owns this region.
    pub const fn owner(&self) -> &SymbolKey {
        &self.owner
    }

    /// Returns the semantic role of this region.
    pub const fn role(&self) -> LocalSymbolRegionRole {
        self.role
    }

    /// Returns the canonical syntax path identifying this region.
    pub fn anchors(&self) -> &[SyntaxAnchor] {
        &self.anchors
    }

    /// Returns the stable ordinal used to distinguish repeated roles at one anchor.
    pub const fn ordinal(&self) -> Option<SymbolOrdinal> {
        self.ordinal
    }
}

/// A deterministic key for one symbol inside a local semantic region.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalSymbolKey {
    region: LocalSymbolRegionKey,
    kind: SymbolKind,
    anchors: Arc<[SyntaxAnchor]>,
    ordinal: Option<SymbolOrdinal>,
}

impl LocalSymbolKey {
    pub(super) fn try_new(
        region: LocalSymbolRegionKey,
        kind: SymbolKind,
        anchors: impl IntoIterator<Item = SyntaxAnchor>,
        ordinal: Option<SymbolOrdinal>,
    ) -> Option<Self> {
        if !kind.is_local() {
            return None;
        }

        let anchors = shared_slice(anchors);

        if anchors.is_empty() {
            return None;
        }

        Some(Self {
            region,
            kind,
            anchors,
            ordinal,
        })
    }

    /// Returns the deterministic region key that owns this local symbol.
    pub const fn region(&self) -> &LocalSymbolRegionKey {
        &self.region
    }

    /// Returns this key's exact local symbol kind.
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// Returns every syntax occurrence contributing to this symbol identity.
    pub fn anchors(&self) -> &[SyntaxAnchor] {
        &self.anchors
    }

    /// Returns the stable role or source-order ordinal when one is required.
    pub const fn ordinal(&self) -> Option<SymbolOrdinal> {
        self.ordinal
    }
}
