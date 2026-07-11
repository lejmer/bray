use std::borrow::Cow;

use bray_source::TextRange;

use super::CatalogSourceId;

/// Selects the semantic family defined by one catalog source.
#[cfg(any(test, feature = "generation"))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogKind {
    /// Ambient and module-scoped compiler-known declarations and values.
    CompilerKnown,
    /// Recognition contracts for ordinary imported standard-library declarations.
    RecognizedStandardLibrary,
}

/// One checked-in generator input in the canonical catalog inventory.
#[cfg(any(test, feature = "generation"))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogSource {
    id: CatalogSourceId,
    kind: CatalogKind,
    relative_path: &'static str,
    text: &'static str,
}

#[cfg(any(test, feature = "generation"))]
impl CatalogSource {
    pub(crate) const fn new(
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

    /// Returns source text available only to catalog generation and tests.
    pub const fn text(self) -> &'static str {
        self.text
    }
}

/// The canonical sequence of checked-in catalog generator inputs.
#[cfg(any(test, feature = "generation"))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CatalogSourceInventory {
    pub(super) sources: &'static [CatalogSource],
}

#[cfg(any(test, feature = "generation"))]
impl CatalogSourceInventory {
    /// Returns all generator inputs in canonical manifest order.
    pub const fn sources(self) -> &'static [CatalogSource] {
        self.sources
    }

    /// Returns a source by its checked inventory-local identity.
    pub fn source(self, id: CatalogSourceId) -> Option<&'static CatalogSource> {
        id.to_index().and_then(|index| self.sources.get(index))
    }
}

/// Provenance range inside one catalog generator input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogSourceAnchor {
    pub(super) source: CatalogSourceId,
    pub(super) range: TextRange,
}

impl CatalogSourceAnchor {
    /// Returns the generator-input identity containing this range.
    pub const fn source(self) -> CatalogSourceId {
        self.source
    }

    /// Returns the half-open UTF-8 byte range in the catalog source.
    pub const fn range(self) -> TextRange {
        self.range
    }
}

/// Stable handle to one generated declaration-surface syntax record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogDeclarationSurface(pub(super) CatalogSourceAnchor);

impl CatalogDeclarationSurface {
    /// Returns generator-input provenance for this surface.
    pub const fn anchor(self) -> CatalogSourceAnchor {
        self.0
    }
}

/// Stable handle to one generated type-expression syntax record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogTypeSurface(pub(super) CatalogSourceAnchor);

impl CatalogTypeSurface {
    /// Returns generator-input provenance for this surface.
    pub const fn anchor(self) -> CatalogSourceAnchor {
        self.0
    }
}

/// The exact source spelling of one language-known value token.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogTokenSpelling(pub(super) Cow<'static, str>);

impl CatalogTokenSpelling {
    #[cfg(any(test, feature = "generation"))]
    pub(super) fn new(value: impl AsRef<str>) -> Self {
        Self(Cow::Owned(value.as_ref().to_owned()))
    }

    #[cfg(any(test, feature = "generation"))]
    pub(super) fn to_owned_storage(&self) -> Self {
        Self::new(self.as_str())
    }

    /// Returns the exact token spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) const fn from_static(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }
}

#[cfg(any(test, feature = "generation"))]
const SOURCES: [CatalogSource; 6] = [
    CatalogSource::new(
        CatalogSourceId::new(0),
        CatalogKind::CompilerKnown,
        "catalog/ambient/fundamentals.braydef",
        include_str!("../../catalog/ambient/fundamentals.braydef"),
    ),
    CatalogSource::new(
        CatalogSourceId::new(1),
        CatalogKind::CompilerKnown,
        "catalog/ambient/implementations.braydef",
        include_str!("../../catalog/ambient/implementations.braydef"),
    ),
    CatalogSource::new(
        CatalogSourceId::new(2),
        CatalogKind::CompilerKnown,
        "catalog/ambient/traits.braydef",
        include_str!("../../catalog/ambient/traits.braydef"),
    ),
    CatalogSource::new(
        CatalogSourceId::new(3),
        CatalogKind::CompilerKnown,
        "catalog/core/memory.braydef",
        include_str!("../../catalog/core/memory.braydef"),
    ),
    CatalogSource::new(
        CatalogSourceId::new(4),
        CatalogKind::RecognizedStandardLibrary,
        "catalog/recognized/standard-library.braydef",
        include_str!("../../catalog/recognized/standard-library.braydef"),
    ),
    CatalogSource::new(
        CatalogSourceId::new(5),
        CatalogKind::CompilerKnown,
        "catalog/std/target-facts.braydef",
        include_str!("../../catalog/std/target-facts.braydef"),
    ),
];

#[cfg(any(test, feature = "generation"))]
static INVENTORY: CatalogSourceInventory = CatalogSourceInventory { sources: &SOURCES };

/// Returns the complete generator-input inventory for catalog tests.
#[cfg(any(test, feature = "generation"))]
pub const fn generator_input_inventory() -> &'static CatalogSourceInventory {
    &INVENTORY
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{
        CatalogDeclarationSurface, CatalogKind, CatalogSourceAnchor, CatalogTokenSpelling,
        CatalogTypeSurface, generator_input_inventory,
    };

    #[test]
    fn generator_input_inventory_is_complete_and_canonical() {
        let inventory = generator_input_inventory();
        let sources = inventory.sources();
        let manifest_paths = include_str!("../../catalog/manifest.txt")
            .lines()
            .map(|path| format!("catalog/{path}"))
            .collect::<Vec<_>>();

        assert_eq!(sources.len(), 6);
        assert_eq!(
            sources
                .iter()
                .map(|source| source.relative_path())
                .collect::<Vec<_>>(),
            manifest_paths
        );

        assert_eq!(sources[0].kind(), CatalogKind::CompilerKnown);
        assert_eq!(sources[4].kind(), CatalogKind::RecognizedStandardLibrary);

        assert!(sources[0].relative_path() < sources[1].relative_path());

        assert!(sources[0].text().starts_with("catalog compiler_known;"));
        assert!(
            sources[4]
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
        let source = generator_input_inventory().sources()[0].id();

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
        let spelling = CatalogTokenSpelling::new("true");

        assert_eq!(spelling.as_str(), "true");
    }
}
