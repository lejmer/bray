use std::sync::Arc;

use crate::{
    AvailabilityRule, CompilerKnownIterationRole, CompilerKnownOperationRole, ImplementationHook,
    RepresentationRole,
};

use super::super::{
    CatalogDeclarationKind, CatalogDeclarationSurface, CatalogKind, CatalogScopeLocation,
    CatalogSourceAnchor, CatalogTokenSpelling, CatalogTypeSurface,
    RecognizedStandardLibraryDeclarationIdentity,
};

#[derive(Clone, Debug)]
pub(crate) struct ValidatedCatalog {
    pub(crate) compiler_known_scopes: Vec<ValidatedScope>,
    pub(crate) compiler_known_declarations: Vec<ValidatedDeclaration>,
    pub(crate) compiler_known_values: Vec<ValidatedValue>,
    pub(crate) recognized_scopes: Vec<ValidatedScope>,
    pub(crate) recognized_declarations: Vec<ValidatedDeclaration>,
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedScope {
    pub(crate) key: Arc<str>,
    pub(crate) location: CatalogScopeLocation,
    pub(crate) anchor: CatalogSourceAnchor,
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedDeclaration {
    pub(crate) key: Arc<str>,
    pub(crate) owner: ValidatedDeclarationOwner,
    pub(crate) recognized_identity: Option<RecognizedStandardLibraryDeclarationIdentity>,
    pub(crate) kind: CatalogDeclarationKind,
    pub(crate) surface: CatalogDeclarationSurface,
    pub(crate) representation_role: Option<RepresentationRole>,
    pub(crate) implementation_hook: Option<ImplementationHook>,
    pub(crate) iteration_role: Option<CompilerKnownIterationRole>,
    pub(crate) operation_role: Option<CompilerKnownOperationRole>,
    pub(crate) availability_rule: AvailabilityRule,
    pub(crate) anchor: CatalogSourceAnchor,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ValidatedDeclarationOwner {
    Scope(usize),
    Declaration(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedValue {
    pub(crate) key: Arc<str>,
    pub(crate) owner_scope: usize,
    pub(crate) spelling: CatalogTokenSpelling,
    pub(crate) type_surface: CatalogTypeSurface,
    pub(crate) representation_role: RepresentationRole,
    pub(crate) availability_rule: AvailabilityRule,
    pub(crate) anchor: CatalogSourceAnchor,
}

#[derive(Clone, Debug)]
pub(super) struct RawScope {
    pub(super) catalog_kind: CatalogKind,
    pub(super) key: Arc<str>,
    pub(super) location: CatalogScopeLocation,
    pub(super) anchor: CatalogSourceAnchor,
}

#[derive(Clone, Debug)]
pub(super) struct RawDeclaration {
    pub(super) catalog_kind: CatalogKind,
    pub(super) scope_key: Arc<str>,
    pub(super) key: Arc<str>,
    pub(super) owner_key: Option<Arc<str>>,
    pub(super) owner: Option<ValidatedDeclarationOwner>,
    pub(super) recognized_identity: Option<RecognizedStandardLibraryDeclarationIdentity>,
    pub(super) declared_kind: Option<CatalogDeclarationKind>,
    pub(super) kind: Option<CatalogDeclarationKind>,
    pub(super) surface: CatalogDeclarationSurface,
    pub(super) representation_role: Option<RepresentationRole>,
    pub(super) implementation_hook: Option<ImplementationHook>,
    pub(super) iteration_role: Option<CompilerKnownIterationRole>,
    pub(super) operation_role: Option<CompilerKnownOperationRole>,
    pub(super) availability_rule: AvailabilityRule,
    pub(super) anchor: CatalogSourceAnchor,
}

#[derive(Clone, Debug)]
pub(super) struct RawValue {
    pub(super) scope_key: Arc<str>,
    pub(super) key: Arc<str>,
    pub(super) spelling: CatalogTokenSpelling,
    pub(super) type_surface: CatalogTypeSurface,
    pub(super) representation_role: RepresentationRole,
    pub(super) availability_rule: AvailabilityRule,
    pub(super) anchor: CatalogSourceAnchor,
}
