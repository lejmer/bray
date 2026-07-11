use std::path::Path;

use bray_parser::{
    DeclarationFragmentContext, DeclarationFragmentSyntax, parse_declaration_fragment,
    parse_type_expression_fragment,
};
use bray_source::{SourceIdentity, SourceOrigin, SourceStore, SourceVersion};

use super::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogDiagnostic, CatalogDiagnosticKind,
    CatalogDiagnostics, CatalogFragmentValidator, CatalogScopeLocation, CatalogSource,
    CatalogSourceAnchor, CatalogSurfaceContext, CatalogTypeSurface, CompilerKnownCatalog,
    CompilerKnownDeclarationOwner, RecognizedStandardLibraryDeclarationOwner, build_catalog,
    generator_input_inventory,
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
    let mut validator = BrayFragmentValidator;
    let catalog = build_catalog(generator_input_inventory(), &mut validator)?;
    let catalog_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog");
    let digest = source_digest(MANIFEST, &catalog_directory)?;

    Ok(GeneratedCatalogOutput {
        rust_source: render_catalog(&catalog, &digest),
        source_digest: digest,
    })
}

struct BrayFragmentValidator;

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

        match result.declaration() {
            Some(declaration) => Ok(declaration_kind(declaration)),
            None => Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidDeclarationSurface,
            )),
        }
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

fn render_catalog(catalog: &CompilerKnownCatalog, digest: &str) -> String {
    let mut output = String::from(
        "// @generated by `cargo xtask compiler-known generate`. Do not edit.\n\n\
         use std::borrow::Cow;\n\n\
         use bray_source::{TextRange, TextSize};\n\n\
         use crate::{AvailabilityRule, ImplementationHook, RepresentationRole};\n\n\
         use super::super::*;\n\n",
    );

    output.push_str(&format!(
        "pub const CATALOG_SOURCE_DIGEST: &str = {digest:?};\n\n"
    ));
    render_scopes(&mut output, catalog);
    render_declarations(&mut output, catalog);
    render_values(&mut output, catalog);
    render_recognized_scopes(&mut output, catalog);
    render_recognized_declarations(&mut output, catalog);
    output.push_str(
        "pub static COMPILER_KNOWN_CATALOG: CompilerKnownCatalog = CompilerKnownCatalog {\n\
             compiler_known_scopes: Cow::Borrowed(COMPILER_KNOWN_SCOPES),\n\
             compiler_known_declarations: Cow::Borrowed(COMPILER_KNOWN_DECLARATIONS),\n\
             compiler_known_values: Cow::Borrowed(COMPILER_KNOWN_VALUES),\n\
             recognized_scopes: Cow::Borrowed(RECOGNIZED_SCOPES),\n\
             recognized_declarations: Cow::Borrowed(RECOGNIZED_DECLARATIONS),\n\
         };\n",
    );

    output
}

fn render_scopes(output: &mut String, catalog: &CompilerKnownCatalog) {
    output.push_str("static COMPILER_KNOWN_SCOPES: &[CompilerKnownScopeDescriptor] = &[\n");

    for scope in catalog.compiler_known_scopes() {
        let location = match scope.location() {
            CatalogScopeLocation::Ambient => "CatalogScopeLocation::Ambient".to_owned(),
            CatalogScopeLocation::Module(path) => format!(
                "CatalogScopeLocation::Module(CatalogPath::from_static(&[{}]))",
                path.segments()
                    .map(|segment| format!("Cow::Borrowed({segment:?})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };

        output.push_str(&format!(
            "    CompilerKnownScopeDescriptor {{\n        id: CompilerKnownScopeId::new({}),\n        key: CompilerKnownScopeKey::from_static({:?}),\n        location: {},\n        declaration_ids: Cow::Borrowed(&[{}]),\n        value_ids: Cow::Borrowed(&[{}]),\n    }},\n",
            scope.id().raw(),
            scope.key().as_str(),
            location,
            render_ids(scope.declaration_ids(), "CompilerKnownDeclarationId"),
            render_ids(scope.value_ids(), "CompilerKnownValueId"),
        ));
    }

    output.push_str("];\n\n");
}

fn render_declarations(output: &mut String, catalog: &CompilerKnownCatalog) {
    output.push_str(
        "static COMPILER_KNOWN_DECLARATIONS: &[CompilerKnownDeclarationDescriptor] = &[\n",
    );

    for declaration in catalog.compiler_known_declarations() {
        let owner = match declaration.owner() {
            CompilerKnownDeclarationOwner::Scope(id) => format!(
                "CompilerKnownDeclarationOwner::Scope(CompilerKnownScopeId::new({}))",
                id.raw()
            ),
            CompilerKnownDeclarationOwner::Declaration(id) => format!(
                "CompilerKnownDeclarationOwner::Declaration(CompilerKnownDeclarationId::new({}))",
                id.raw()
            ),
        };

        output.push_str(&format!(
            "    CompilerKnownDeclarationDescriptor {{\n        id: CompilerKnownDeclarationId::new({}),\n        key: CompilerKnownDeclarationKey::from_static({:?}),\n        owner: {},\n        kind: CatalogDeclarationKind::{:?},\n        surface: {},\n        representation_role: {},\n        implementation_hook: {},\n        availability_rule: AvailabilityRule::{:?},\n    }},\n",
            declaration.id().raw(),
            declaration.key().as_str(),
            owner,
            declaration.kind(),
            render_declaration_surface(declaration.surface()),
            render_option("RepresentationRole", declaration.representation_role()),
            render_option("ImplementationHook", declaration.implementation_hook()),
            declaration.availability_rule(),
        ));
    }

    output.push_str("];\n\n");
}

fn render_values(output: &mut String, catalog: &CompilerKnownCatalog) {
    output.push_str("static COMPILER_KNOWN_VALUES: &[CompilerKnownValueDescriptor] = &[\n");

    for value in catalog.compiler_known_values() {
        output.push_str(&format!(
            "    CompilerKnownValueDescriptor {{\n        id: CompilerKnownValueId::new({}),\n        key: CompilerKnownValueKey::from_static({:?}),\n        owner_scope: CompilerKnownScopeId::new({}),\n        spelling: CatalogTokenSpelling::from_static({:?}),\n        type_surface: {},\n        representation_role: RepresentationRole::{:?},\n        availability_rule: AvailabilityRule::{:?},\n    }},\n",
            value.id().raw(),
            value.key().as_str(),
            value.owner_scope().raw(),
            value.spelling().as_str(),
            render_type_surface(value.type_surface()),
            value.representation_role(),
            value.availability_rule(),
        ));
    }

    output.push_str("];\n\n");
}

fn render_recognized_scopes(output: &mut String, catalog: &CompilerKnownCatalog) {
    output.push_str("static RECOGNIZED_SCOPES: &[RecognizedStandardLibraryScopeDescriptor] = &[\n");

    for scope in catalog.recognized_standard_library_scopes() {
        let segments = scope
            .path()
            .segments()
            .map(|segment| format!("Cow::Borrowed({segment:?})"))
            .collect::<Vec<_>>()
            .join(", ");

        output.push_str(&format!(
            "    RecognizedStandardLibraryScopeDescriptor {{\n        id: RecognizedStandardLibraryScopeId::new({}),\n        key: RecognizedStandardLibraryScopeKey::from_static({:?}),\n        path: CatalogPath::from_static(&[{}]),\n        declaration_ids: Cow::Borrowed(&[{}]),\n    }},\n",
            scope.id().raw(),
            scope.key().as_str(),
            segments,
            render_ids(
                scope.declaration_ids(),
                "RecognizedStandardLibraryDeclarationId"
            ),
        ));
    }

    output.push_str("];\n\n");
}

fn render_recognized_declarations(output: &mut String, catalog: &CompilerKnownCatalog) {
    output.push_str(
        "static RECOGNIZED_DECLARATIONS: &[RecognizedStandardLibraryDeclarationDescriptor] = &[\n",
    );

    for declaration in catalog.recognized_standard_library_declarations() {
        let owner = match declaration.owner() {
            RecognizedStandardLibraryDeclarationOwner::Scope(id) => format!(
                "RecognizedStandardLibraryDeclarationOwner::Scope(RecognizedStandardLibraryScopeId::new({}))",
                id.raw()
            ),
            RecognizedStandardLibraryDeclarationOwner::Declaration(id) => format!(
                "RecognizedStandardLibraryDeclarationOwner::Declaration(RecognizedStandardLibraryDeclarationId::new({}))",
                id.raw()
            ),
        };

        output.push_str(&format!(
            "    RecognizedStandardLibraryDeclarationDescriptor {{\n        id: RecognizedStandardLibraryDeclarationId::new({}),\n        key: RecognizedStandardLibraryDeclarationKey::from_static({:?}),\n        owner: {},\n        kind: CatalogDeclarationKind::{:?},\n        surface: {},\n        implementation_hook: {},\n        availability_rule: AvailabilityRule::{:?},\n    }},\n",
            declaration.id().raw(),
            declaration.key().as_str(),
            owner,
            declaration.kind(),
            render_declaration_surface(declaration.surface()),
            render_option("ImplementationHook", declaration.implementation_hook()),
            declaration.availability_rule(),
        ));
    }

    output.push_str("];\n\n");
}

fn render_ids<T>(ids: &[T], type_name: &str) -> String
where
    T: Copy + Into<u32>,
{
    ids.iter()
        .map(|id| format!("{type_name}::new({})", Into::<u32>::into(*id)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_declaration_surface(surface: CatalogDeclarationSurface) -> String {
    format!(
        "CatalogDeclarationSurface({})",
        render_anchor(surface.anchor())
    )
}

fn render_type_surface(surface: CatalogTypeSurface) -> String {
    format!("CatalogTypeSurface({})", render_anchor(surface.anchor()))
}

fn render_anchor(anchor: CatalogSourceAnchor) -> String {
    format!(
        "CatalogSourceAnchor {{ source: CatalogSourceId::new({}), range: TextRange::new(TextSize::new({}), TextSize::new({})) }}",
        anchor.source().raw(),
        anchor.range().start().bytes(),
        anchor.range().end().bytes(),
    )
}

fn render_option<T>(type_name: &str, value: Option<T>) -> String
where
    T: std::fmt::Debug,
{
    match value {
        Some(value) => format!("Some({type_name}::{value:?})"),
        None => "None".to_owned(),
    }
}

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
    }
}
