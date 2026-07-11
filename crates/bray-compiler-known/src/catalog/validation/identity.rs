use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::entry::ParsedDeclarationIdentity;
use super::super::{
    CatalogDiagnostic, CatalogDiagnosticKind, CatalogKeyDomain, CatalogRelatedKey,
    RecognizedStandardLibraryDeclarationIdentity,
};
use super::model::RawDeclaration;

pub(super) fn recognized_identity(
    identity: &ParsedDeclarationIdentity,
) -> RecognizedStandardLibraryDeclarationIdentity {
    match identity {
        ParsedDeclarationIdentity::Name(value) => {
            RecognizedStandardLibraryDeclarationIdentity::owned_name(value)
        }
        ParsedDeclarationIdentity::Ordinal(value) => {
            RecognizedStandardLibraryDeclarationIdentity::Ordinal(*value)
        }
    }
}

pub(super) fn validate_recognized_identity_uniqueness(
    declarations: &[RawDeclaration],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let mut identities = BTreeMap::new();

    for declaration in declarations {
        let (Some(owner), Some(kind), Some(identity)) = (
            declaration.owner,
            declaration.kind,
            declaration.recognized_identity.as_ref(),
        ) else {
            continue;
        };

        let semantic_identity = (owner, kind, identity);

        if let Some(first) = identities.insert(semantic_identity, Arc::clone(&declaration.key)) {
            diagnostics.push(
                CatalogDiagnostic::new(
                    declaration.anchor,
                    CatalogDiagnosticKind::DuplicateRecognizedDeclarationIdentity,
                )
                .with_related_keys([CatalogRelatedKey::new(
                    CatalogKeyDomain::RecognizedStandardLibraryDeclaration,
                    first,
                )]),
            );
        }
    }
}
