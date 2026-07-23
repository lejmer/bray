use std::collections::BTreeMap;

use crate::CompilerKnownIterationRole;

use super::super::{CatalogDeclarationKind, CatalogDiagnostic, CatalogDiagnosticKind};
use super::model::{RawDeclaration, ValidatedDeclarationOwner};

pub(super) fn validate_iteration_protocol(
    declarations: &[RawDeclaration],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let mut roles = BTreeMap::new();

    for (index, declaration) in declarations.iter().enumerate() {
        let Some(role) = declaration.iteration_role else {
            continue;
        };

        if roles.insert(role, index).is_some() {
            diagnostics.push(CatalogDiagnostic::new(
                declaration.anchor,
                CatalogDiagnosticKind::DuplicateIterationRole { role },
            ));

            continue;
        }

        if let Some(kind) = declaration.kind
            && !role_accepts(role, kind)
        {
            diagnostics.push(CatalogDiagnostic::new(
                declaration.anchor,
                CatalogDiagnosticKind::IncompatibleIterationRole {
                    role,
                    declaration: kind,
                },
            ));
        }
    }

    if roles.is_empty() {
        return;
    }

    if CompilerKnownIterationRole::ALL
        .iter()
        .any(|role| !roles.contains_key(role))
    {
        let anchor = roles
            .values()
            .next()
            .map_or(declarations[0].anchor, |index| declarations[*index].anchor);

        diagnostics.push(CatalogDiagnostic::new(
            anchor,
            CatalogDiagnosticKind::IncompleteIterationProtocol,
        ));

        return;
    }

    validate_members(
        declarations,
        &roles,
        CompilerKnownIterationRole::IterableTrait,
        [
            CompilerKnownIterationRole::IterableElement,
            CompilerKnownIterationRole::IterableCursor,
            CompilerKnownIterationRole::IterableIterate,
        ],
        diagnostics,
    );

    validate_members(
        declarations,
        &roles,
        CompilerKnownIterationRole::IteratorTrait,
        [
            CompilerKnownIterationRole::IteratorElement,
            CompilerKnownIterationRole::IteratorNext,
        ],
        diagnostics,
    );
}

fn validate_members<const N: usize>(
    declarations: &[RawDeclaration],
    roles: &BTreeMap<CompilerKnownIterationRole, usize>,
    trait_role: CompilerKnownIterationRole,
    member_roles: [CompilerKnownIterationRole; N],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let Some(trait_index) = roles.get(&trait_role).copied() else {
        return;
    };

    for role in member_roles {
        let Some(member_index) = roles.get(&role).copied() else {
            continue;
        };

        if declarations[member_index].owner
            != Some(ValidatedDeclarationOwner::Declaration(trait_index))
        {
            diagnostics.push(CatalogDiagnostic::new(
                declarations[member_index].anchor,
                CatalogDiagnosticKind::InvalidIterationComponentOwner { role },
            ));
        }
    }
}

const fn role_accepts(role: CompilerKnownIterationRole, kind: CatalogDeclarationKind) -> bool {
    match role {
        CompilerKnownIterationRole::IterableTrait | CompilerKnownIterationRole::IteratorTrait => {
            matches!(kind, CatalogDeclarationKind::Trait)
        }
        CompilerKnownIterationRole::IterableElement
        | CompilerKnownIterationRole::IterableCursor
        | CompilerKnownIterationRole::IteratorElement => {
            matches!(kind, CatalogDeclarationKind::TraitTypeMember)
        }
        CompilerKnownIterationRole::IterableIterate | CompilerKnownIterationRole::IteratorNext => {
            matches!(kind, CatalogDeclarationKind::TraitCallableMember)
        }
    }
}
