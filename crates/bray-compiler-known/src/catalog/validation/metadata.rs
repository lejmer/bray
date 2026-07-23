use std::sync::Arc;

use crate::{
    AvailabilityRule, CompilerKnownIterationRole, CompilerKnownOperationRole, ImplementationHook,
    RepresentationRole,
};

use super::super::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogMetadataKind, CatalogSourceAnchor,
};

pub(super) fn availability(
    spelling: Option<(&Arc<str>, CatalogSourceAnchor)>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> AvailabilityRule {
    let Some((spelling, anchor)) = spelling else {
        return AvailabilityRule::Always;
    };

    match AvailabilityRule::from_catalog_spelling(spelling) {
        Some(rule) => rule,
        None => {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::UnknownMetadata {
                    metadata: CatalogMetadataKind::Availability,
                    spelling: Arc::clone(spelling),
                },
            ));

            AvailabilityRule::Always
        }
    }
}

pub(super) fn representation(
    spelling: Option<(&Arc<str>, CatalogSourceAnchor)>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> Option<RepresentationRole> {
    let (spelling, anchor) = spelling?;

    match RepresentationRole::from_catalog_spelling(spelling) {
        Some(role) => Some(role),
        None => {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::UnknownMetadata {
                    metadata: CatalogMetadataKind::Representation,
                    spelling: Arc::clone(spelling),
                },
            ));

            None
        }
    }
}

pub(super) fn implementation(
    spelling: Option<(&Arc<str>, CatalogSourceAnchor)>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> Option<ImplementationHook> {
    let (spelling, anchor) = spelling?;

    match ImplementationHook::from_catalog_spelling(spelling) {
        Some(hook) => Some(hook),
        None => {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::UnknownMetadata {
                    metadata: CatalogMetadataKind::Implementation,
                    spelling: Arc::clone(spelling),
                },
            ));

            None
        }
    }
}

pub(super) fn operation(
    spelling: Option<(&Arc<str>, CatalogSourceAnchor)>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> Option<CompilerKnownOperationRole> {
    let (spelling, anchor) = spelling?;

    match CompilerKnownOperationRole::from_catalog_spelling(spelling) {
        Some(role) => Some(role),
        None => {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::UnknownMetadata {
                    metadata: CatalogMetadataKind::Operation,
                    spelling: Arc::clone(spelling),
                },
            ));

            None
        }
    }
}

pub(super) fn iteration(
    spelling: Option<(&Arc<str>, CatalogSourceAnchor)>,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) -> Option<CompilerKnownIterationRole> {
    let (spelling, anchor) = spelling?;

    match CompilerKnownIterationRole::from_catalog_spelling(spelling) {
        Some(role) => Some(role),
        None => {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::UnknownMetadata {
                    metadata: CatalogMetadataKind::Iteration,
                    spelling: Arc::clone(spelling),
                },
            ));

            None
        }
    }
}
