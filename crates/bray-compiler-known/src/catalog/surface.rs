use std::borrow::Cow;

use bray_syntax::SyntaxKind;

use super::{CatalogDeclarationKind, CatalogDeclarationSurface, CatalogTypeSurface};

/// One pre-parsed token in a generated catalog surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogSurfaceToken {
    kind: SyntaxKind,
    spelling: Cow<'static, str>,
}

impl CatalogSurfaceToken {
    pub(super) fn new(kind: SyntaxKind, spelling: impl AsRef<str>) -> Self {
        Self {
            kind,
            spelling: Cow::Owned(spelling.as_ref().to_owned()),
        }
    }

    pub(super) const fn from_static(kind: SyntaxKind, spelling: &'static str) -> Self {
        Self {
            kind,
            spelling: Cow::Borrowed(spelling),
        }
    }

    /// Returns the token's stable Bray syntax kind.
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// Returns the token spelling without source trivia.
    pub fn spelling(&self) -> &str {
        &self.spelling
    }
}

/// Pre-parsed source-independent syntax for one declaration surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogDeclarationSurfaceSyntax {
    pub(super) surface: CatalogDeclarationSurface,
    pub(super) kind: CatalogDeclarationKind,
    pub(super) tokens: Cow<'static, [CatalogSurfaceToken]>,
}

impl CatalogDeclarationSurfaceSyntax {
    /// Returns the catalog surface identified by this syntax record.
    pub const fn surface(&self) -> CatalogDeclarationSurface {
        self.surface
    }

    /// Returns the parsed declaration category.
    pub const fn kind(&self) -> CatalogDeclarationKind {
        self.kind
    }

    /// Returns pre-parsed tokens in source order without trivia.
    pub fn tokens(&self) -> &[CatalogSurfaceToken] {
        &self.tokens
    }
}

/// Pre-parsed source-independent syntax for one type-expression surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogTypeSurfaceSyntax {
    pub(super) surface: CatalogTypeSurface,
    pub(super) tokens: Cow<'static, [CatalogSurfaceToken]>,
}

impl CatalogTypeSurfaceSyntax {
    /// Returns the catalog surface identified by this syntax record.
    pub const fn surface(&self) -> CatalogTypeSurface {
        self.surface
    }

    /// Returns pre-parsed tokens in source order without trivia.
    pub fn tokens(&self) -> &[CatalogSurfaceToken] {
        &self.tokens
    }
}
