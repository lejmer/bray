use super::super::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogDiagnostics, CatalogSource,
    CatalogSurfaceContext, CatalogTypeSurface,
};

/// Validates embedded catalog fragments according to the Bray grammar.
pub trait CatalogFragmentValidator {
    /// Validates one declaration fragment in its owner-derived context.
    fn validate_declaration_surface(
        &mut self,
        source: CatalogSource,
        surface: CatalogDeclarationSurface,
        context: CatalogSurfaceContext,
    ) -> Result<CatalogDeclarationKind, CatalogDiagnostics>;

    /// Validates one complete type-expression fragment.
    fn validate_type_surface(
        &mut self,
        source: CatalogSource,
        surface: CatalogTypeSurface,
    ) -> Result<(), CatalogDiagnostics>;
}
