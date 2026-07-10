use std::sync::Arc;

use bray_source::TextRange;

use super::CatalogSourceId;

/// Selects the semantic family defined by one catalog source.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogKind {
    /// Ambient and module-scoped compiler-known declarations and values.
    CompilerKnown,
    /// Recognition contracts for ordinary imported standard-library declarations.
    RecognizedStandardLibrary,
}

/// One source file compiled into the canonical catalog inventory.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogSource {
    id: CatalogSourceId,
    kind: CatalogKind,
    relative_path: &'static str,
    text: &'static str,
}

impl CatalogSource {
    pub(super) const fn new(
        id: CatalogSourceId,
        kind: CatalogKind,
        relative_path: &'static str,
        text: &'static str,
    ) -> Self {
        Self {
            id,
            kind,
            relative_path,
            text,
        }
    }

    /// Returns the inventory-local source identity.
    pub const fn id(self) -> CatalogSourceId {
        self.id
    }

    /// Returns the descriptor family defined by this source.
    pub const fn kind(self) -> CatalogKind {
        self.kind
    }

    /// Returns the repository-relative catalog path used for invariant reports.
    pub const fn relative_path(self) -> &'static str {
        self.relative_path
    }

    /// Returns the source text embedded in the compiler binary.
    pub const fn text(self) -> &'static str {
        self.text
    }
}

/// The complete canonical sequence of catalog sources embedded in the compiler.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogSourceInventory {
    pub(super) sources: &'static [CatalogSource],
}

impl CatalogSourceInventory {
    /// Returns all embedded sources in canonical inventory order.
    pub const fn sources(self) -> &'static [CatalogSource] {
        self.sources
    }

    /// Returns a source by its checked inventory-local identity.
    pub fn source(self, id: CatalogSourceId) -> Option<&'static CatalogSource> {
        id.to_index().and_then(|index| self.sources.get(index))
    }
}

/// A source range inside one embedded catalog file.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogSourceAnchor {
    pub(super) source: CatalogSourceId,
    pub(super) range: TextRange,
}

impl CatalogSourceAnchor {
    /// Returns the embedded source containing this range.
    pub const fn source(self) -> CatalogSourceId {
        self.source
    }

    /// Returns the half-open UTF-8 byte range in the catalog source.
    pub const fn range(self) -> TextRange {
        self.range
    }
}

/// An exact embedded Bray declaration fragment retained for lazy semantic work.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogDeclarationSurface(pub(super) CatalogSourceAnchor);

impl CatalogDeclarationSurface {
    /// Returns the exact catalog source anchor for the Bray fragment.
    pub const fn anchor(self) -> CatalogSourceAnchor {
        self.0
    }
}

/// An exact embedded Bray type-expression fragment retained for lazy binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogTypeSurface(pub(super) CatalogSourceAnchor);

impl CatalogTypeSurface {
    /// Returns the exact catalog source anchor for the Bray fragment.
    pub const fn anchor(self) -> CatalogSourceAnchor {
        self.0
    }
}

/// The exact source spelling of one language-known value token.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogTokenSpelling(pub(super) Arc<str>);

impl CatalogTokenSpelling {
    /// Returns the exact token spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

const COMPILER_KNOWN_SOURCE_ID: CatalogSourceId = CatalogSourceId::new(0);
const RECOGNIZED_SOURCE_ID: CatalogSourceId = CatalogSourceId::new(1);

const SOURCES: [CatalogSource; 2] = [
    CatalogSource::new(
        COMPILER_KNOWN_SOURCE_ID,
        CatalogKind::CompilerKnown,
        "catalog/ambient/fundamentals.braydef",
        include_str!("../../catalog/ambient/fundamentals.braydef"),
    ),
    CatalogSource::new(
        RECOGNIZED_SOURCE_ID,
        CatalogKind::RecognizedStandardLibrary,
        "catalog/recognized/standard-library.braydef",
        include_str!("../../catalog/recognized/standard-library.braydef"),
    ),
];

static INVENTORY: CatalogSourceInventory = CatalogSourceInventory { sources: &SOURCES };

/// Returns the complete target-independent embedded source inventory.
pub const fn embedded_source_inventory() -> &'static CatalogSourceInventory {
    &INVENTORY
}

#[cfg(test)]
mod tests {
    use bray_base::shared_str;
    use bray_source::{TextRange, TextSize};

    use super::{
        CatalogDeclarationSurface, CatalogKind, CatalogSourceAnchor, CatalogTokenSpelling,
        CatalogTypeSurface, embedded_source_inventory,
    };

    #[test]
    fn embedded_inventory_is_complete_and_canonical() {
        let inventory = embedded_source_inventory();
        let sources = inventory.sources();

        assert_eq!(sources.len(), 2);

        assert_eq!(sources[0].kind(), CatalogKind::CompilerKnown);
        assert_eq!(sources[1].kind(), CatalogKind::RecognizedStandardLibrary);

        assert!(sources[0].relative_path() < sources[1].relative_path());

        assert!(sources[0].text().starts_with("catalog compiler_known;"));
        assert!(
            sources[1]
                .text()
                .starts_with("catalog recognized_standard_library;")
        );

        for (index, source) in sources.iter().enumerate() {
            assert_eq!(source.id().to_index(), Some(index));
            assert_eq!(inventory.source(source.id()), Some(source));
        }
    }

    #[test]
    fn declaration_and_type_surfaces_are_distinct_exact_anchors() {
        let source = embedded_source_inventory().sources()[0].id();

        let anchor = CatalogSourceAnchor {
            source,
            range: TextRange::new(TextSize::new(1), TextSize::new(4)),
        };

        let declaration = CatalogDeclarationSurface(anchor);
        let type_surface = CatalogTypeSurface(anchor);

        assert_eq!(declaration.anchor(), anchor);
        assert_eq!(type_surface.anchor(), anchor);
    }

    #[test]
    fn token_spelling_retains_exact_catalog_text() {
        let spelling = CatalogTokenSpelling(shared_str("true"));

        assert_eq!(spelling.as_str(), "true");
    }
}
