use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::entry::ParsedDeclarationIdentity;
use super::super::{
    CatalogDeclarationKind, CatalogDiagnostic, CatalogDiagnosticKind, CatalogKeyDomain,
    CatalogPath, CatalogRelatedKey, RecognizedStandardLibraryDeclarationIdentity,
};
use super::model::{RawDeclaration, ValidatedDeclarationOwner, ValidatedScope};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExternalIdentityComponent {
    Scope(CatalogPath),
    Declaration(
        CatalogDeclarationKind,
        RecognizedStandardLibraryDeclarationIdentity,
    ),
}

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
    scopes: &[ValidatedScope],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let mut identities = BTreeMap::new();

    for (index, declaration) in declarations.iter().enumerate() {
        let Some(semantic_identity) = external_identity(index, declarations, scopes) else {
            continue;
        };

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

fn external_identity(
    declaration: usize,
    declarations: &[RawDeclaration],
    scopes: &[ValidatedScope],
) -> Option<Vec<ExternalIdentityComponent>> {
    let declaration = declarations.get(declaration)?;
    let kind = declaration.kind?;

    // The complete owner-relative key must survive independently of validation scratch records.
    let recognized_identity = declaration.recognized_identity.clone()?;

    let mut identity = match declaration.owner? {
        ValidatedDeclarationOwner::Scope(scope) => {
            let scope = scopes.get(scope)?;

            let super::super::CatalogScopeLocation::Module(path) = &scope.location else {
                return None;
            };

            // Validation keys own their small immutable path so equal paths across distinct
            // catalog scopes compare as the same external owner.
            vec![ExternalIdentityComponent::Scope(path.clone())]
        }
        ValidatedDeclarationOwner::Declaration(owner) => {
            external_identity(owner, declarations, scopes)?
        }
    };

    identity.push(ExternalIdentityComponent::Declaration(
        kind,
        recognized_identity,
    ));

    Some(identity)
}
