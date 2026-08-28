use std::collections::BTreeMap;
use std::path::Path;

use bray_parser::{
    DeclarationFragmentContext, DeclarationFragmentSyntax, parse_declaration_fragment,
    parse_type_expression_fragment,
};
use bray_source::{SourceIdentity, SourceOrigin, SourceStore, SourceVersion};
use bray_syntax::{SyntaxWalkControl, SyntaxWalkEvent, SyntaxWalkRoot, walk_syntax_node};

use super::rendering::render_catalog;
use super::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogDeclarationSurfaceSyntax,
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogDiagnostics, CatalogFragmentValidator,
    CatalogSource, CatalogSourceAnchor, CatalogSurfaceContext, CatalogSurfaceElement,
    CatalogSurfaceToken, CatalogTypeSurface, CatalogTypeSurfaceSyntax, build_catalog,
    generator_input_inventory,
};
use crate::catalog_digest::source_digest;

const MANIFEST: &str = include_str!("../../catalog/catalog.braydef-manifest");

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
    grammar_revision: crate::CatalogGrammarRevision,
    source_digest: String,
}

impl GeneratedCatalogOutput {
    /// Returns the complete generated Rust catalog module.
    pub fn rust_source(&self) -> &str {
        &self.rust_source
    }

    /// Returns the exact catalog grammar revision used for generation.
    pub const fn grammar_revision(&self) -> crate::CatalogGrammarRevision {
        self.grammar_revision
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

    let digest = source_digest(
        crate::CatalogGrammarRevision::SUPPORTED,
        MANIFEST,
        &catalog_directory,
    )?;

    catalog.declaration_surfaces = declaration_surfaces.into();
    catalog.type_surfaces = type_surfaces.into();

    Ok(GeneratedCatalogOutput {
        rust_source: render_catalog(&catalog, crate::CatalogGrammarRevision::SUPPORTED, &digest),
        grammar_revision: crate::CatalogGrammarRevision::SUPPORTED,
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

        let Some(elements) = declaration_elements(declaration, text) else {
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
                elements: elements.into(),
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

        let Some(elements) = collect_surface_elements(result.type_expression(), text) else {
            return Err(invalid_surface(
                surface.anchor(),
                CatalogDiagnosticKind::InvalidTypeSurface,
            ));
        };

        self.type_surfaces.insert(
            surface.anchor(),
            CatalogTypeSurfaceSyntax {
                surface,
                elements: elements.into(),
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

pub(super) fn declaration_kind(declaration: &DeclarationFragmentSyntax) -> CatalogDeclarationKind {
    match declaration {
        DeclarationFragmentSyntax::Constant(_) => CatalogDeclarationKind::Constant,
        DeclarationFragmentSyntax::Static(_) => {
            panic!("compiler-known catalog does not support static declarations")
        }
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

fn declaration_elements(
    declaration: &DeclarationFragmentSyntax,
    source_text: &str,
) -> Option<Vec<CatalogSurfaceElement>> {
    macro_rules! elements {
        ($syntax:expr) => {
            collect_surface_elements($syntax, source_text)
        };
    }

    match declaration {
        DeclarationFragmentSyntax::Constant(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::Static(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::Function(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::Predicate(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::CallableContract(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::CallableOverload(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::ImplementationOverload(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::Struct(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::Union(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::Trait(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::InherentImplementation(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::UnnamedTraitImplementation(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::NamedTraitImplementation(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::StructField(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::UnionVariant(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::UnionPayloadField(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TypeConstructorMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TypeCallableMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::FinalizerMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::DestructorMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::ScopeEnterMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::ScopeExitMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitConstantMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitTypeMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitPredicateMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitCallableMember(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitFinalizerRequirement(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitDestructorRequirement(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitScopeEnterRequirement(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::TraitScopeExitRequirement(syntax) => elements!(syntax),
        DeclarationFragmentSyntax::ImplementationTypeMemberBinding(syntax) => elements!(syntax),
    }
}

fn collect_surface_elements(
    node: &impl SyntaxWalkRoot,
    source_text: &str,
) -> Option<Vec<CatalogSurfaceElement>> {
    let mut elements = Vec::new();
    let mut valid = true;

    walk_syntax_node(node, |event| {
        let element = match event {
            SyntaxWalkEvent::EnterNode(node) => CatalogSurfaceElement::EnterNode(node.kind()),
            SyntaxWalkEvent::Token(token) => match token.text(source_text) {
                Some(spelling) => {
                    CatalogSurfaceElement::Token(CatalogSurfaceToken::new(token.kind(), spelling))
                }
                None => {
                    valid = false;

                    return SyntaxWalkControl::Stop;
                }
            },
            SyntaxWalkEvent::ExitNode(node) => CatalogSurfaceElement::ExitNode(node.kind()),
        };

        elements.push(element);

        SyntaxWalkControl::Continue
    });

    valid.then_some(elements)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_syntax::SyntaxKind;

    use super::{generate_catalog_output, render_catalog};
    use crate::{
        CATALOG_GRAMMAR_REVISION, CATALOG_SOURCE_DIGEST, COMPILER_KNOWN_CATALOG,
        CatalogSurfaceElement, generator_input_inventory,
    };

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
        assert_eq!(first.grammar_revision(), CATALOG_GRAMMAR_REVISION);
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
            render_catalog(
                &COMPILER_KNOWN_CATALOG,
                output.grammar_revision(),
                output.source_digest(),
            )
        );

        assert_eq!(generator_input_inventory().sources().len(), 18);

        let Some(declaration) = COMPILER_KNOWN_CATALOG
            .compiler_known_declarations()
            .iter()
            .find(|declaration| declaration.key().as_str() == "RawPointer")
        else {
            panic!("published catalog should contain RawPointer");
        };

        let Some(surface) = COMPILER_KNOWN_CATALOG.declaration_surface(declaration.surface())
        else {
            panic!("published declaration should retain pre-parsed syntax");
        };

        assert_eq!(surface.kind(), declaration.kind());

        assert!(
            surface
                .elements()
                .contains(&CatalogSurfaceElement::EnterNode(
                    SyntaxKind::GenericParameterList
                ))
        );

        assert_balanced(surface.elements());

        let Some(value) = COMPILER_KNOWN_CATALOG
            .compiler_known_values()
            .iter()
            .find(|value| value.key().as_str() == "None")
        else {
            panic!("published catalog should contain None");
        };

        let Some(type_surface) = COMPILER_KNOWN_CATALOG.type_surface(value.type_surface()) else {
            panic!("published value should retain pre-parsed type syntax");
        };

        assert!(type_surface.elements().iter().any(|element| matches!(
            element,
            CatalogSurfaceElement::Token(token) if token.kind() == SyntaxKind::QuestionToken
        )));

        assert!(maximum_depth(type_surface.elements()) >= 3);

        assert_balanced(type_surface.elements());
    }

    #[test]
    fn published_catalog_contains_the_closed_conformance_entries() {
        let compiler_known = COMPILER_KNOWN_CATALOG
            .compiler_known_declarations()
            .iter()
            .map(|descriptor| descriptor.key().as_str())
            .collect::<BTreeSet<_>>();

        for key in [
            "CheckedConvertTo",
            "CheckedConvertToCall",
            "CheckedConvertToError",
            "ConversionError",
            "Copyable",
            "HeapStorageImplementation",
            "Storage",
            "StorageBorrow",
            "StorageBorrowMut",
            "StorageCreate",
            "StorageDestroy",
            "StorageRelease",
            "Uninit",
            "UninitNew",
            "UninitPointer",
            "UninitPointerMut",
            "UninitWrite",
            "UninitAssumeInitialized",
            "UninitMove",
            "UninitInitialized",
            "BorrowFrom",
            "BorrowMutFrom",
            "SharedAliasValid",
            "ExclusiveAliasValid",
            "EpochCurrent",
            "SynchronizedAccess",
            "MovementStable",
            "FinalizationPending",
            "AddressOf",
            "AddressOfMut",
            "Allocate",
            "Deallocate",
            "MemoryCopy",
            "MemoryCopyOverlapping",
            "RawPointerByteOffset",
            "RawPointerIsNull",
            "RawPointerNull",
            "RawPointerOffset",
            "RawPointerRead",
            "RawPointerReinterpret",
            "RawPointerWrite",
            "AlignedFor",
            "InitializedAs",
            "InitializedRangeAs",
            "NonOverlapping",
            "OwnedAllocation",
            "SameAllocation",
            "ValidRead",
            "ValidWrite",
        ] {
            assert!(compiler_known.contains(key), "{key} must be compiler-known");
        }

        let recognized = COMPILER_KNOWN_CATALOG
            .recognized_standard_library_declarations()
            .iter()
            .map(|descriptor| descriptor.key().as_str())
            .collect::<BTreeSet<_>>();

        let expected = [
            "StandardCallbackContext",
            "StandardCallbackContextConstructor",
            "StandardCallbackState",
            "StandardCharacterFromScalarValue",
            "StandardCharacterIsAlphabetic",
            "StandardCharacterIsNumeric",
            "StandardCharacterIsWhitespace",
            "StandardCharacterScalarValue",
            "StandardCharacterUtf8Byte",
            "StandardCharacterUtf8Length",
            "StandardConvert",
            "StandardCurrentNativeThreadIdentity",
            "StandardMainNativeThreadIdentity",
            "StandardMemoryAddressOf",
            "StandardMemoryAddressOfMut",
            "StandardMemoryAlignOf",
            "StandardMemoryAllocate",
            "StandardMemoryAssumeInitialized",
            "StandardMemoryBorrowFrom",
            "StandardMemoryBorrowMutFrom",
            "StandardMemoryRawBufferRelocate",
            "StandardMemoryByteBufferFill",
            "StandardMemoryByteBufferCopy",
            "StandardMemoryByteBufferRead",
            "StandardMemoryByteOffset",
            "StandardMemorySliceLength",
            "StandardMemoryCallableFromPointer",
            "StandardMemoryCapacity",
            "StandardMemoryCopy",
            "StandardMemoryCopyOverlapping",
            "StandardRawBufferConstructor",
            "StandardMemoryDeallocate",
            "StandardMemoryInitializedCount",
            "StandardMemoryInitializedSlice",
            "StandardMemoryInitializedSliceMut",
            "StandardMemoryIsNull",
            "StandardMemoryLayout",
            "StandardMemoryLayoutAlign",
            "StandardMemoryLayoutBytes",
            "StandardMemoryLayoutError",
            "StandardMemoryLayoutErrorVariant0SizeOverflow",
            "StandardMemoryLayoutErrorVariant1UnsupportedAlignment",
            "StandardMemoryLayoutOf",
            "StandardMemoryMoveInitialized",
            "StandardMemoryNull",
            "StandardMemoryOffset",
            "StandardMemoryPointer",
            "StandardMemoryPointerFromCallable",
            "StandardMemoryRead",
            "StandardMemoryReinterpret",
            "StandardMemoryRawBufferRelease",
            "StandardMemoryRawBufferReplace",
            "StandardMemorySetInitializedCount",
            "StandardMemorySizeOf",
            "StandardMemorySparePointer",
            "StandardMemoryStrideOf",
            "StandardMemoryTrailingLayoutOf",
            "StandardMemoryUninitNew",
            "StandardMemoryUninitPointer",
            "StandardMemoryUninitPointerMut",
            "StandardMemoryUninitWrite",
            "StandardMemoryWrite",
            "StandardNativeThreadExecution",
            "StandardNativeThreadStart",
            "StandardNativeThreadPanicReportRecovery",
            "StandardNativeThreadPanicReporting",
            "StandardRawAllocation",
            "StandardRawAllocationAlign",
            "StandardRawAllocationBytes",
            "StandardRawAllocationPointer",
            "StandardRawBuffer",
            "StandardRawBufferCapacity",
            "StandardRawBufferConstructor",
            "StandardRawBufferInitialized",
            "StandardRawBufferPointer",
            "StandardRoundTo",
            "StandardRoundingRule",
            "StandardRoundingRuleVariant0NearestEven",
            "StandardRoundingRuleVariant1TowardZero",
            "StandardRoundingRuleVariant2TowardNegativeInfinity",
            "StandardRoundingRuleVariant3TowardPositiveInfinity",
            "StandardRoundingRuleVariant4AwayFromZero",
            "StandardRunCancellationObservation",
            "StandardRunCancellationPropagation",
            "StandardRuntimeCharacterFromScalarValue",
            "StandardRuntimeCharacterIsAlphabetic",
            "StandardRuntimeCharacterIsNumeric",
            "StandardRuntimeCharacterIsWhitespace",
            "StandardRuntimeCharacterScalarValue",
            "StandardRuntimeCharacterUtf8Byte",
            "StandardRuntimeCharacterUtf8Length",
            "StandardRuntimeMemoryAllocate",
            "StandardRuntimeMemoryDeallocate",
            "StandardRuntimeOwnedText",
            "StandardRuntimeOwnedTextData",
            "StandardRuntimeOwnedTextLength",
            "StandardRuntimeOwnedTextOwner",
            "StandardRuntimeStringEquals",
            "StandardRuntimeStringFromUtf8",
            "StandardRuntimeStringScalarAt",
            "StandardRuntimeStringScalarCount",
            "StandardRuntimeStringScalarSlice",
            "StandardRuntimeValidatedText",
            "StandardRuntimeValidatedTextText",
            "StandardRuntimeValidatedTextValid",
            "StandardSaturateTo",
            "StandardScalarCursor",
            "StandardScalarCursorIterator",
            "StandardScalarCursorIteratorNext",
            "StandardStringEquals",
            "StandardStringFromUtf8",
            "StandardStringIsEmpty",
            "StandardStringScalarAt",
            "StandardStringScalarCount",
            "StandardStringScalarSlice",
            "StandardStringScalars",
            "StandardStringUtf8",
            "StandardTaskEventCreation",
            "StandardTaskEventDestruction",
            "StandardTaskEventSignal",
            "StandardTaskEventWait",
            "StandardTaskYield",
            "StandardTestingFail",
            "StandardThreadTransferredValue",
            "StandardTruncateTo",
            "StandardUtf8Error",
            "StandardUtf8ErrorVariant0InvalidEncoding",
            "StandardWrapTo",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();

        assert_eq!(recognized, expected);
    }

    fn assert_balanced(elements: &[CatalogSurfaceElement]) {
        let mut stack = Vec::new();

        for element in elements {
            match element {
                CatalogSurfaceElement::EnterNode(kind) => stack.push(*kind),
                CatalogSurfaceElement::Token(_) => {}
                CatalogSurfaceElement::ExitNode(kind) => {
                    let Some(entered) = stack.pop() else {
                        panic!("surface exits a node that was not entered");
                    };

                    assert_eq!(entered, *kind);
                }
            }
        }

        assert!(stack.is_empty());
    }

    fn maximum_depth(elements: &[CatalogSurfaceElement]) -> usize {
        let mut depth = 0_usize;
        let mut maximum = 0_usize;

        for element in elements {
            match element {
                CatalogSurfaceElement::EnterNode(_) => {
                    depth += 1;
                    maximum = maximum.max(depth);
                }
                CatalogSurfaceElement::Token(_) => {}
                CatalogSurfaceElement::ExitNode(_) => depth -= 1,
            }
        }

        maximum
    }
}
