use std::borrow::Cow;

use bray_source::{SourceId, SourceIdentity};
use bray_syntax::{
    PreparsedSyntaxEvent, PreparsedSyntaxFragment, PreparsedSyntaxFragmentError, SyntaxKind,
};

use super::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogSourceAnchor, CatalogTypeSurface,
};

/// One pre-parsed token in a generated catalog surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogSurfaceToken {
    kind: SyntaxKind,
    spelling: Cow<'static, str>,
}

impl CatalogSurfaceToken {
    #[cfg(any(test, feature = "generation"))]
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

    /// Returns whether this token is an ordinary identifier.
    pub fn is_identifier(&self) -> bool {
        self.kind == SyntaxKind::IdentifierToken
    }

    /// Returns whether this token declares internal visibility.
    pub fn is_internal_visibility(&self) -> bool {
        self.kind == SyntaxKind::InternalKeyword
    }
}

/// One structural event in a pre-parsed catalog syntax tree.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogSurfaceElement {
    /// Traversal enters a syntax node of the given stable kind.
    EnterNode(SyntaxKind),
    /// Traversal reaches one source token.
    Token(CatalogSurfaceToken),
    /// Traversal exits a syntax node of the given stable kind.
    ExitNode(SyntaxKind),
}

/// Pre-parsed source-independent syntax for one declaration surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogDeclarationSurfaceSyntax {
    pub(super) surface: CatalogDeclarationSurface,
    pub(super) kind: CatalogDeclarationKind,
    pub(super) elements: Cow<'static, [CatalogSurfaceElement]>,
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

    /// Returns balanced node and token events in source order.
    pub fn elements(&self) -> &[CatalogSurfaceElement] {
        &self.elements
    }

    /// Reconstructs the generated declaration as ordinary typed Bray syntax.
    pub fn syntax_fragment(&self) -> Result<PreparsedSyntaxFragment, PreparsedSyntaxFragmentError> {
        preparsed_fragment(self.surface.anchor(), &self.elements)
    }

    /// Returns signature-owned child metadata derived from the generated syntax tree.
    pub fn signature(&self) -> CatalogDeclarationSignature {
        declaration_signature(&self.elements)
    }
}

/// The kind of one written generic parameter in generated declaration syntax.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogGenericParameterKind {
    /// A generic type parameter.
    Type,
    /// A generic constant parameter.
    Const,
}

/// One written generic parameter in generated declaration syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogGenericParameter {
    kind: CatalogGenericParameterKind,
    name: String,
}

impl CatalogGenericParameter {
    /// Returns the parameter category.
    pub const fn kind(&self) -> CatalogGenericParameterKind {
        self.kind
    }

    /// Returns the declared parameter spelling.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The signature-owned child shape of one generated declaration surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogDeclarationSignature {
    generic_parameters: Box<[CatalogGenericParameter]>,
    implementation_parameter_candidates: Box<[CatalogGenericParameter]>,
    callable_parameters: u32,
    predicate_parameters: u32,
    is_static: bool,
}

impl CatalogDeclarationSignature {
    /// Returns written generic parameters in source order.
    pub fn generic_parameters(&self) -> &[CatalogGenericParameter] {
        &self.generic_parameters
    }

    /// Returns names that may become inferred parameters of an implementation.
    pub fn implementation_parameter_candidates(&self) -> &[CatalogGenericParameter] {
        &self.implementation_parameter_candidates
    }

    /// Returns the number of written parameters in the direct callable parameter list.
    pub const fn callable_parameters(&self) -> u32 {
        self.callable_parameters
    }

    /// Returns the number of written parameters in the direct predicate parameter list.
    pub const fn predicate_parameters(&self) -> u32 {
        self.predicate_parameters
    }

    /// Returns whether the declaration carries a static modifier.
    pub const fn is_static(&self) -> bool {
        self.is_static
    }
}

/// Pre-parsed source-independent syntax for one type-expression surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogTypeSurfaceSyntax {
    pub(super) surface: CatalogTypeSurface,
    pub(super) elements: Cow<'static, [CatalogSurfaceElement]>,
}

impl CatalogTypeSurfaceSyntax {
    /// Returns the catalog surface identified by this syntax record.
    pub const fn surface(&self) -> CatalogTypeSurface {
        self.surface
    }

    /// Returns balanced node and token events in source order.
    pub fn elements(&self) -> &[CatalogSurfaceElement] {
        &self.elements
    }

    /// Reconstructs the generated type expression as ordinary typed Bray syntax.
    pub fn syntax_fragment(&self) -> Result<PreparsedSyntaxFragment, PreparsedSyntaxFragmentError> {
        preparsed_fragment(self.surface.anchor(), &self.elements)
    }
}

fn preparsed_fragment(
    anchor: CatalogSourceAnchor,
    elements: &[CatalogSurfaceElement],
) -> Result<PreparsedSyntaxFragment, PreparsedSyntaxFragmentError> {
    let source = anchor.source().raw();

    let source_id = SourceId::generated(source)
        .ok_or(PreparsedSyntaxFragmentError::GeneratedSourceIdOutOfRange)?;

    let identity = SourceIdentity::generated(source)
        .ok_or(PreparsedSyntaxFragmentError::GeneratedSourceIdOutOfRange)?;

    let events = elements.iter().map(|element| match element {
        CatalogSurfaceElement::EnterNode(kind) => PreparsedSyntaxEvent::EnterNode(*kind),
        CatalogSurfaceElement::Token(token) => PreparsedSyntaxEvent::Token {
            kind: token.kind(),
            text: token.spelling(),
        },
        CatalogSurfaceElement::ExitNode(kind) => PreparsedSyntaxEvent::ExitNode(*kind),
    });

    PreparsedSyntaxFragment::try_new(source_id, identity, "generated-catalog-surface", events)
}

fn declaration_signature(elements: &[CatalogSurfaceElement]) -> CatalogDeclarationSignature {
    let is_implementation = matches!(
        elements.first(),
        Some(CatalogSurfaceElement::EnterNode(
            SyntaxKind::InherentImplementationDeclaration
                | SyntaxKind::NamedTraitImplementationDeclaration
                | SyntaxKind::UnnamedTraitImplementationDeclaration
        ))
    );

    let mut generic_parameters = Vec::new();
    let mut implementation_parameter_candidates = Vec::new();
    let mut callable_parameters = 0_u32;
    let mut predicate_parameters = 0_u32;
    let mut depth = 0_u32;
    let mut generic_list_depth = None;
    let mut generic_parameter = None;
    let mut generic_argument_depth = None;
    let mut implementation_parameter_kind = None;
    let mut parameter_list_depth = None;
    let mut predicate_parameter_list_depth = None;
    let mut is_static = false;

    for element in elements {
        match element {
            CatalogSurfaceElement::EnterNode(kind) => {
                depth = depth.saturating_add(1);

                match (*kind, depth) {
                    (SyntaxKind::GenericParameterList, 2) => generic_list_depth = Some(depth),
                    (SyntaxKind::GenericArgument, _) if is_implementation => {
                        generic_argument_depth = Some(depth);
                    }
                    (SyntaxKind::TypeExpression, _)
                        if generic_argument_depth.is_some()
                            && implementation_parameter_kind.is_none() =>
                    {
                        implementation_parameter_kind =
                            Some((CatalogGenericParameterKind::Type, depth));
                    }
                    (SyntaxKind::Expression, _)
                        if generic_argument_depth.is_some()
                            && implementation_parameter_kind.is_none() =>
                    {
                        implementation_parameter_kind =
                            Some((CatalogGenericParameterKind::Const, depth));
                    }
                    (SyntaxKind::ParameterList, 2) => parameter_list_depth = Some(depth),
                    (SyntaxKind::PredicateParameterList, 2) => {
                        predicate_parameter_list_depth = Some(depth);
                    }
                    (SyntaxKind::GenericTypeParameter, 3) if generic_list_depth == Some(2) => {
                        generic_parameter = Some((CatalogGenericParameterKind::Type, depth));
                    }
                    (SyntaxKind::GenericConstParameter, 3) if generic_list_depth == Some(2) => {
                        generic_parameter = Some((CatalogGenericParameterKind::Const, depth));
                    }
                    (SyntaxKind::Parameter, 3) if parameter_list_depth == Some(2) => {
                        callable_parameters = callable_parameters.saturating_add(1);
                    }
                    (SyntaxKind::PredicateParameter, 3)
                        if predicate_parameter_list_depth == Some(2) =>
                    {
                        predicate_parameters = predicate_parameters.saturating_add(1);
                    }
                    _ => {}
                }
            }
            CatalogSurfaceElement::Token(token) => {
                is_static |= depth <= 2 && token.kind() == SyntaxKind::StaticKeyword;

                if token.is_identifier()
                    && let Some((kind, _)) = generic_parameter.take()
                {
                    generic_parameters.push(CatalogGenericParameter {
                        kind,
                        name: token.spelling().to_owned(),
                    });
                }

                if token.is_identifier()
                    && let Some((kind, _)) = implementation_parameter_kind
                    && !implementation_parameter_candidates.iter().any(
                        |candidate: &CatalogGenericParameter| {
                            candidate.kind == kind && candidate.name == token.spelling()
                        },
                    )
                {
                    implementation_parameter_candidates.push(CatalogGenericParameter {
                        kind,
                        name: token.spelling().to_owned(),
                    });
                }
            }
            CatalogSurfaceElement::ExitNode(kind) => {
                if *kind == SyntaxKind::GenericParameterList && generic_list_depth == Some(depth) {
                    generic_list_depth = None;
                }

                if *kind == SyntaxKind::ParameterList && parameter_list_depth == Some(depth) {
                    parameter_list_depth = None;
                }

                if *kind == SyntaxKind::PredicateParameterList
                    && predicate_parameter_list_depth == Some(depth)
                {
                    predicate_parameter_list_depth = None;
                }

                if *kind == SyntaxKind::GenericArgument && generic_argument_depth == Some(depth) {
                    generic_argument_depth = None;
                    implementation_parameter_kind = None;
                }

                if implementation_parameter_kind
                    .is_some_and(|(_, parameter_depth)| parameter_depth == depth)
                {
                    implementation_parameter_kind = None;
                }

                if generic_parameter.is_some_and(|(_, parameter_depth)| parameter_depth == depth) {
                    generic_parameter = None;
                }

                depth = depth.saturating_sub(1);
            }
        }
    }

    CatalogDeclarationSignature {
        generic_parameters: generic_parameters.into_boxed_slice(),
        implementation_parameter_candidates: implementation_parameter_candidates.into_boxed_slice(),
        callable_parameters,
        predicate_parameters,
        is_static,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogGenericParameterKind, CatalogSurfaceElement, CatalogSurfaceToken,
        declaration_signature,
    };
    use bray_syntax::SyntaxKind;

    use crate::COMPILER_KNOWN_CATALOG;

    #[test]
    fn reconstructed_surfaces_use_the_generated_source_domain() {
        let Some(surface) = COMPILER_KNOWN_CATALOG.declaration_surfaces().first() else {
            panic!("generated catalog must contain declaration surfaces");
        };

        let fragment = match surface.syntax_fragment() {
            Ok(fragment) => fragment,
            Err(error) => panic!("generated catalog surface must reconstruct: {error:?}"),
        };

        assert!(fragment.source().source_id().is_generated());
        assert_eq!(fragment.source().source_id().to_index(), None);
        assert!(fragment.source().identity().is_generated());
    }

    #[test]
    fn signature_scanning_ignores_nested_callable_modifiers() {
        let direct = declaration_signature(&[
            CatalogSurfaceElement::EnterNode(SyntaxKind::FunctionDeclaration),
            CatalogSurfaceElement::EnterNode(SyntaxKind::FunctionModifiers),
            token(SyntaxKind::StaticKeyword, "static"),
            CatalogSurfaceElement::ExitNode(SyntaxKind::FunctionModifiers),
            CatalogSurfaceElement::ExitNode(SyntaxKind::FunctionDeclaration),
        ]);

        let nested = declaration_signature(&[
            CatalogSurfaceElement::EnterNode(SyntaxKind::CallableContractDeclaration),
            CatalogSurfaceElement::EnterNode(SyntaxKind::TypeExpression),
            CatalogSurfaceElement::EnterNode(SyntaxKind::CallableModifiers),
            token(SyntaxKind::StaticKeyword, "static"),
            CatalogSurfaceElement::ExitNode(SyntaxKind::CallableModifiers),
            CatalogSurfaceElement::ExitNode(SyntaxKind::TypeExpression),
            CatalogSurfaceElement::ExitNode(SyntaxKind::CallableContractDeclaration),
        ]);

        assert!(direct.is_static());
        assert!(!nested.is_static());
    }

    #[test]
    fn signature_scanning_retains_generic_names_and_combined_order() {
        let signature = declaration_signature(&[
            CatalogSurfaceElement::EnterNode(SyntaxKind::StructDeclaration),
            CatalogSurfaceElement::EnterNode(SyntaxKind::GenericParameterList),
            CatalogSurfaceElement::EnterNode(SyntaxKind::GenericTypeParameter),
            token(SyntaxKind::IdentifierToken, "T"),
            CatalogSurfaceElement::ExitNode(SyntaxKind::GenericTypeParameter),
            CatalogSurfaceElement::EnterNode(SyntaxKind::GenericConstParameter),
            token(SyntaxKind::IdentifierToken, "N"),
            CatalogSurfaceElement::ExitNode(SyntaxKind::GenericConstParameter),
            CatalogSurfaceElement::ExitNode(SyntaxKind::GenericParameterList),
            CatalogSurfaceElement::ExitNode(SyntaxKind::StructDeclaration),
        ]);

        assert_eq!(signature.generic_parameters().len(), 2);
        assert_eq!(signature.generic_parameters()[0].name(), "T");

        assert_eq!(
            signature.generic_parameters()[0].kind(),
            CatalogGenericParameterKind::Type
        );

        assert_eq!(signature.generic_parameters()[1].name(), "N");

        assert_eq!(
            signature.generic_parameters()[1].kind(),
            CatalogGenericParameterKind::Const
        );
    }

    #[test]
    fn signature_scanning_retains_only_implementation_head_candidates() {
        let implementation = declaration_signature(&[
            CatalogSurfaceElement::EnterNode(SyntaxKind::NamedTraitImplementationDeclaration),
            CatalogSurfaceElement::EnterNode(SyntaxKind::GenericArgument),
            CatalogSurfaceElement::EnterNode(SyntaxKind::TypeExpression),
            token(SyntaxKind::IdentifierToken, "T"),
            CatalogSurfaceElement::ExitNode(SyntaxKind::TypeExpression),
            CatalogSurfaceElement::ExitNode(SyntaxKind::GenericArgument),
            CatalogSurfaceElement::ExitNode(SyntaxKind::NamedTraitImplementationDeclaration),
        ]);

        let callable = declaration_signature(&[
            CatalogSurfaceElement::EnterNode(SyntaxKind::FunctionDeclaration),
            CatalogSurfaceElement::EnterNode(SyntaxKind::GenericArgument),
            CatalogSurfaceElement::EnterNode(SyntaxKind::TypeExpression),
            token(SyntaxKind::IdentifierToken, "T"),
            CatalogSurfaceElement::ExitNode(SyntaxKind::TypeExpression),
            CatalogSurfaceElement::ExitNode(SyntaxKind::GenericArgument),
            CatalogSurfaceElement::ExitNode(SyntaxKind::FunctionDeclaration),
        ]);

        let [candidate] = implementation.implementation_parameter_candidates() else {
            panic!("implementation head should retain one inferred parameter candidate");
        };

        assert_eq!(candidate.name(), "T");
        assert_eq!(candidate.kind(), CatalogGenericParameterKind::Type);
        assert!(callable.implementation_parameter_candidates().is_empty());
    }

    fn token(kind: SyntaxKind, spelling: &str) -> CatalogSurfaceElement {
        CatalogSurfaceElement::Token(CatalogSurfaceToken::new(kind, spelling))
    }
}
