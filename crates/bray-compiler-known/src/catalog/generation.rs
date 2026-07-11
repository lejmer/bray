use std::collections::BTreeMap;
use std::path::Path;

use bray_parser::{
    DeclarationFragmentContext, DeclarationFragmentSyntax, parse_declaration_fragment,
    parse_type_expression_fragment,
};
use bray_source::{SourceIdentity, SourceOrigin, SourceStore, SourceVersion};
use bray_syntax::SyntaxToken;

use super::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogDeclarationSurfaceSyntax,
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics, CatalogFragmentValidator,
    CatalogSource, CatalogSourceAnchor, CatalogSurfaceContext, CatalogSurfaceToken,
    CatalogTypeSurface, CatalogTypeSurfaceSyntax, build_catalog, generator_input_inventory,
};
use crate::catalog_digest::source_digest;

const MANIFEST: &str = include_str!("../../catalog/manifest.txt");

/// Failure to validate or render checked-in compiler-known definitions.
#[derive(Debug)]
pub enum CatalogGenerationError {
    /// Catalog parsing or structural validation failed.
    Catalog(CatalogDiagnostics),
    /// One canonical catalog input could not be read for digesting.
    Io(std::io::Error),
}

impl std::fmt::Display for CatalogGenerationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(diagnostics) => write!(formatter, "{diagnostics:#?}"),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CatalogGenerationError {}

impl From<CatalogDiagnostics> for CatalogGenerationError {
    fn from(diagnostics: CatalogDiagnostics) -> Self {
        Self::Catalog(diagnostics)
    }
}

impl From<std::io::Error> for CatalogGenerationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Deterministic generated files for the compiler-known catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedCatalogOutput {
    rust_source: String,
    source_digest: String,
}

impl GeneratedCatalogOutput {
    /// Returns the complete generated Rust catalog module.
    pub fn rust_source(&self) -> &str {
        &self.rust_source
    }

    /// Returns the digest stamped into generated output.
    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }
}

/// Parses, validates, and deterministically renders the canonical catalog.
pub fn generate_catalog_output() -> Result<GeneratedCatalogOutput, CatalogGenerationError> {
    let mut validator = BrayFragmentValidator::default();
    let mut catalog = build_catalog(generator_input_inventory(), &mut validator)?;
    let (declaration_surfaces, type_surfaces) = validator.into_surfaces();
    let catalog_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog");
    let digest = source_digest(MANIFEST, &catalog_directory)?;

    catalog.declaration_surfaces = declaration_surfaces.into();
    catalog.type_surfaces = type_surfaces.into();

    Ok(GeneratedCatalogOutput {
        rust_source: render_catalog(&catalog, &digest),
        source_digest: digest,
    })
}

#[derive(Default)]
struct BrayFragmentValidator {
    declaration_surfaces: BTreeMap<CatalogSourceAnchor, CatalogDeclarationSurfaceSyntax>,
    type_surfaces: BTreeMap<CatalogSourceAnchor, CatalogTypeSurfaceSyntax>,
}

impl BrayFragmentValidator {
    fn into_surfaces(
        self,
    ) -> (
        Vec<CatalogDeclarationSurfaceSyntax>,
        Vec<CatalogTypeSurfaceSyntax>,
    ) {
        (
            self.declaration_surfaces.into_values().collect(),
            self.type_surfaces.into_values().collect(),
        )
    }
}

impl CatalogFragmentValidator for BrayFragmentValidator {
    fn validate_declaration_surface(
        &mut self,
        source: CatalogSource,
        surface: CatalogDeclarationSurface,
        context: CatalogSurfaceContext,
    ) -> Result<CatalogDeclarationKind, CatalogDiagnostics> {
        let Some(text) = surface.anchor().range().slice_str(source.text()) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        };
        let Some(sources) = fragment_sources(source.relative_path(), text) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        };
        let Some(snapshot) = sources.iter().next() else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        };
        let Some(context) = parser_context(context) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        };
        let result = parse_declaration_fragment(snapshot, context);

        if !result.diagnostics().is_empty() || result.is_recovered() {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        }

        let Some(declaration) = result.declaration() else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        };
        let kind = declaration_kind(declaration);
        let Some(tokens) = declaration_tokens(declaration, text) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            ));
        };

        self.declaration_surfaces.insert(
            surface.anchor(),
            CatalogDeclarationSurfaceSyntax {
                surface,
                kind,
                tokens: tokens.into(),
            },
        );

        Ok(kind)
    }

    fn validate_type_surface(
        &mut self,
        source: CatalogSource,
        surface: CatalogTypeSurface,
    ) -> Result<(), CatalogDiagnostics> {
        let Some(text) = surface.anchor().range().slice_str(source.text()) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidTypeSurface,
            ));
        };
        let Some(sources) = fragment_sources(source.relative_path(), text) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidTypeSurface,
            ));
        };
        let Some(snapshot) = sources.iter().next() else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidTypeSurface,
            ));
        };
        let result = parse_type_expression_fragment(snapshot);

        if !result.diagnostics().is_empty() || result.is_recovered() {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidTypeSurface,
            ));
        }

        let Some(tokens) = collect_surface_tokens(result.type_expression().tokens(), text) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidTypeSurface,
            ));
        };

        self.type_surfaces.insert(
            surface.anchor(),
            CatalogTypeSurfaceSyntax {
                surface,
                tokens: tokens.into(),
            },
        );

        Ok(())
    }
}

fn invalid_surface(anchor: CatalogSourceAnchor, kind: CatalogDiagnosticKind) -> CatalogDiagnostics {
    CatalogDiagnostic::new(anchor, kind).into()
}

fn fragment_sources(name: &str, text: &str) -> Option<SourceStore> {
    let mut sources = SourceStore::new();

    sources
        .insert(
            SourceIdentity::new(0),
            SourceOrigin::test_fixture(name),
            SourceVersion::new(0),
            text.to_owned(),
        )
        .ok()?;

    Some(sources)
}

fn parser_context(context: CatalogSurfaceContext) -> Option<DeclarationFragmentContext> {
    match context {
        CatalogSurfaceContext::Scope => Some(DeclarationFragmentContext::Module),
        CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Struct) => {
            Some(DeclarationFragmentContext::Struct)
        }
        CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Union) => {
            Some(DeclarationFragmentContext::Union)
        }
        CatalogSurfaceContext::Declaration(CatalogDeclarationKind::UnionVariant) => {
            Some(DeclarationFragmentContext::UnionVariant)
        }
        CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Trait) => {
            Some(DeclarationFragmentContext::Trait)
        }
        CatalogSurfaceContext::Declaration(
            CatalogDeclarationKind::InherentImplementation
            | CatalogDeclarationKind::UnnamedTraitImplementation
            | CatalogDeclarationKind::NamedTraitImplementation,
        ) => Some(DeclarationFragmentContext::Implementation),
        CatalogSurfaceContext::Declaration(_) => None,
    }
}

fn declaration_kind(declaration: &DeclarationFragmentSyntax) -> CatalogDeclarationKind {
    match declaration {
        DeclarationFragmentSyntax::Constant(_) => CatalogDeclarationKind::Constant,
        DeclarationFragmentSyntax::Function(_) => CatalogDeclarationKind::Function,
        DeclarationFragmentSyntax::Predicate(_) => CatalogDeclarationKind::Predicate,
        DeclarationFragmentSyntax::CallableContract(_) => CatalogDeclarationKind::CallableContract,
        DeclarationFragmentSyntax::CallableOverload(_) => CatalogDeclarationKind::CallableOverload,
        DeclarationFragmentSyntax::ImplementationOverload(_) => {
            CatalogDeclarationKind::ImplementationOverload
        }
        DeclarationFragmentSyntax::Struct(_) => CatalogDeclarationKind::Struct,
        DeclarationFragmentSyntax::Union(_) => CatalogDeclarationKind::Union,
        DeclarationFragmentSyntax::Trait(_) => CatalogDeclarationKind::Trait,
        DeclarationFragmentSyntax::InherentImplementation(_) => {
            CatalogDeclarationKind::InherentImplementation
        }
        DeclarationFragmentSyntax::UnnamedTraitImplementation(_) => {
            CatalogDeclarationKind::UnnamedTraitImplementation
        }
        DeclarationFragmentSyntax::NamedTraitImplementation(_) => {
            CatalogDeclarationKind::NamedTraitImplementation
        }
        DeclarationFragmentSyntax::StructField(_) => CatalogDeclarationKind::StructField,
        DeclarationFragmentSyntax::UnionVariant(_) => CatalogDeclarationKind::UnionVariant,
        DeclarationFragmentSyntax::UnionPayloadField(_) => {
            CatalogDeclarationKind::UnionPayloadField
        }
        DeclarationFragmentSyntax::TypeConstructorMember(_) => {
            CatalogDeclarationKind::TypeConstructorMember
        }
        DeclarationFragmentSyntax::TypeCallableMember(_) => {
            CatalogDeclarationKind::TypeCallableMember
        }
        DeclarationFragmentSyntax::FinalizerMember(_) => CatalogDeclarationKind::FinalizerMember,
        DeclarationFragmentSyntax::DestructorMember(_) => CatalogDeclarationKind::DestructorMember,
        DeclarationFragmentSyntax::ScopeEnterMember(_) => CatalogDeclarationKind::ScopeEnterMember,
        DeclarationFragmentSyntax::ScopeExitMember(_) => CatalogDeclarationKind::ScopeExitMember,
        DeclarationFragmentSyntax::TraitConstantMember(_) => {
            CatalogDeclarationKind::TraitConstantMember
        }
        DeclarationFragmentSyntax::TraitTypeMember(_) => CatalogDeclarationKind::TraitTypeMember,
        DeclarationFragmentSyntax::TraitPredicateMember(_) => {
            CatalogDeclarationKind::TraitPredicateMember
        }
        DeclarationFragmentSyntax::TraitCallableMember(_) => {
            CatalogDeclarationKind::TraitCallableMember
        }
        DeclarationFragmentSyntax::TraitFinalizerRequirement(_) => {
            CatalogDeclarationKind::TraitFinalizerRequirement
        }
        DeclarationFragmentSyntax::TraitDestructorRequirement(_) => {
            CatalogDeclarationKind::TraitDestructorRequirement
        }
        DeclarationFragmentSyntax::TraitScopeEnterRequirement(_) => {
            CatalogDeclarationKind::TraitScopeEnterRequirement
        }
        DeclarationFragmentSyntax::TraitScopeExitRequirement(_) => {
            CatalogDeclarationKind::TraitScopeExitRequirement
        }
        DeclarationFragmentSyntax::ImplementationTypeMemberBinding(_) => {
            CatalogDeclarationKind::ImplementationTypeMemberBinding
        }
    }
}

fn declaration_tokens(
    declaration: &DeclarationFragmentSyntax,
    source_text: &str,
) -> Option<Vec<CatalogSurfaceToken>> {
    macro_rules! tokens {
        ($syntax:expr) => {
            collect_surface_tokens($syntax.tokens(), source_text)
        };
    }

    match declaration {
        DeclarationFragmentSyntax::Constant(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::Function(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::Predicate(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::CallableContract(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::CallableOverload(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::ImplementationOverload(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::Struct(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::Union(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::Trait(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::InherentImplementation(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::UnnamedTraitImplementation(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::NamedTraitImplementation(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::StructField(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::UnionVariant(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::UnionPayloadField(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TypeConstructorMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TypeCallableMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::FinalizerMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::DestructorMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::ScopeEnterMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::ScopeExitMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitConstantMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitTypeMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitPredicateMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitCallableMember(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitFinalizerRequirement(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitDestructorRequirement(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitScopeEnterRequirement(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::TraitScopeExitRequirement(syntax) => tokens!(syntax),
        DeclarationFragmentSyntax::ImplementationTypeMemberBinding(syntax) => tokens!(syntax),
    }
}

fn collect_surface_tokens(
    tokens: impl Iterator<Item = SyntaxToken>,
    source_text: &str,
) -> Option<Vec<CatalogSurfaceToken>> {
    tokens
        .map(|token| {
            Some(CatalogSurfaceToken::new(
                token.kind(),
                token.text(source_text)?,
            ))
        })
        .collect()
}

use super::rendering::render_catalog;

#[cfg(test)]
mod tests {
    use super::{generate_catalog_output, render_catalog};
    use crate::{CATALOG_SOURCE_DIGEST, COMPILER_KNOWN_CATALOG, generator_input_inventory};

    #[test]
    fn generation_is_byte_deterministic() {
        let first = match generate_catalog_output() {
            Ok(output) => output,
            Err(diagnostics) => panic!("catalog generation failed: {diagnostics:#?}"),
        };
        let second = match generate_catalog_output() {
            Ok(output) => output,
            Err(diagnostics) => panic!("catalog generation failed: {diagnostics:#?}"),
        };

        assert_eq!(first, second);
        assert_eq!(first.source_digest().len(), 64);
        assert_eq!(first.source_digest(), CATALOG_SOURCE_DIGEST);
    }

    #[test]
    fn published_catalog_matches_fresh_generation() {
        let output = match generate_catalog_output() {
            Ok(output) => output,
            Err(diagnostics) => panic!("catalog generation failed: {diagnostics:#?}"),
        };

        assert_eq!(
            output.rust_source(),
            render_catalog(&COMPILER_KNOWN_CATALOG, output.source_digest())
        );
        assert_eq!(generator_input_inventory().sources().len(), 6);

        let declaration = &COMPILER_KNOWN_CATALOG.compiler_known_declarations()[0];
        let Some(surface) = COMPILER_KNOWN_CATALOG.declaration_surface(declaration.surface())
        else {
            panic!("published declaration should retain pre-parsed syntax");
        };

        assert_eq!(surface.kind(), declaration.kind());
        assert!(!surface.tokens().is_empty());

        let value = &COMPILER_KNOWN_CATALOG.compiler_known_values()[0];
        let Some(type_surface) = COMPILER_KNOWN_CATALOG.type_surface(value.type_surface()) else {
            panic!("published value should retain pre-parsed type syntax");
        };

        assert!(!type_surface.tokens().is_empty());
    }
}
