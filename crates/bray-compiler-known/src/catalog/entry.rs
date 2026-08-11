use std::sync::Arc;

use super::{
    CatalogDeclarationSurface, CatalogKind, CatalogSourceAnchor, CatalogTokenSpelling,
    CatalogTypeSurface,
};
use crate::CatalogGrammarRevision;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedCatalogSource {
    pub(super) source_kind: CatalogKind,
    pub(super) declared_kind: Anchored<CatalogKind>,
    pub(super) declared_revision: Anchored<CatalogGrammarRevision>,
    pub(super) scopes: Vec<ParsedScope>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedScope {
    pub(super) key: Anchored<Arc<str>>,
    pub(super) location: Anchored<ParsedScopeLocation>,
    pub(super) entries: Vec<ParsedEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ParsedScopeLocation {
    Ambient,
    Path(Vec<Arc<str>>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ParsedEntry {
    Declaration(ParsedDeclaration),
    Value(ParsedValue),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedDeclaration {
    pub(super) key: Anchored<Arc<str>>,
    pub(super) fields: Vec<ParsedDeclarationField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ParsedDeclarationField {
    Kind(Anchored<Arc<str>>),
    Owner(Anchored<Arc<str>>),
    Identity(Anchored<ParsedDeclarationIdentity>),
    Availability(Anchored<Arc<str>>),
    Representation(Anchored<Arc<str>>),
    Implementation(Anchored<Arc<str>>),
    Iteration(Anchored<Arc<str>>),
    Operation(Anchored<Arc<str>>),
    Surface(Anchored<CatalogDeclarationSurface>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ParsedDeclarationIdentity {
    Name(Arc<str>),
    Ordinal(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedValue {
    pub(super) key: Anchored<Arc<str>>,
    pub(super) fields: Vec<ParsedValueField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ParsedValueField {
    Spelling(Anchored<CatalogTokenSpelling>),
    Type(Anchored<CatalogTypeSurface>),
    Availability(Anchored<Arc<str>>),
    Representation(Anchored<Arc<str>>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Anchored<T> {
    pub(super) value: T,
    pub(super) anchor: CatalogSourceAnchor,
}

impl<T> Anchored<T> {
    pub(super) const fn new(value: T, anchor: CatalogSourceAnchor) -> Self {
        Self { value, anchor }
    }
}
