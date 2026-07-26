use std::collections::BTreeMap;
use std::sync::Arc;

use crate::RepresentationRole;

use super::super::{
    CatalogDeclarationKind, CatalogDiagnostic, CatalogDiagnosticKind, CatalogEntryKind,
    CatalogKeyDomain, CatalogRelatedKey, CatalogSourceInventory, CatalogSurfaceContext,
};
use super::CatalogFragmentValidator;
use super::model::{RawDeclaration, ValidatedDeclarationOwner, ValidatedScope};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Unvisited,
    Visiting,
    Complete,
}

pub(super) fn resolve_declarations(
    declarations: &mut [RawDeclaration],
    scopes: &[ValidatedScope],
    inventory: &'static CatalogSourceInventory,
    validator: &mut impl CatalogFragmentValidator,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    resolve_owners(declarations, scopes, diagnostics);

    let mut states = vec![VisitState::Unvisited; declarations.len()];

    for index in 0..declarations.len() {
        resolve_kind(
            index,
            declarations,
            inventory,
            validator,
            diagnostics,
            &mut states,
        );
    }
}

fn resolve_owners(
    declarations: &mut [RawDeclaration],
    scopes: &[ValidatedScope],
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let scope_indexes = scopes
        .iter()
        .enumerate()
        .map(|(index, scope)| (Arc::clone(&scope.key), index))
        .collect::<BTreeMap<_, _>>();

    let declaration_indexes = declarations
        .iter()
        .enumerate()
        .map(|(index, declaration)| (Arc::clone(&declaration.key), index))
        .collect::<BTreeMap<_, _>>();

    for declaration in declarations {
        declaration.owner = match &declaration.owner_key {
            Some(owner_key) => match declaration_indexes.get(owner_key) {
                Some(index) => Some(ValidatedDeclarationOwner::Declaration(*index)),
                None => {
                    diagnostics.push(
                        CatalogDiagnostic::new(
                            declaration.anchor,
                            CatalogDiagnosticKind::UnknownOwner,
                        )
                        .with_related_keys([CatalogRelatedKey::new(
                            declaration_domain(declaration),
                            Arc::clone(owner_key),
                        )]),
                    );

                    None
                }
            },
            None => scope_indexes
                .get(&declaration.scope_key)
                .copied()
                .map(ValidatedDeclarationOwner::Scope),
        };
    }
}

fn resolve_kind(
    index: usize,
    declarations: &mut [RawDeclaration],
    inventory: &'static CatalogSourceInventory,
    validator: &mut impl CatalogFragmentValidator,
    diagnostics: &mut Vec<CatalogDiagnostic>,
    states: &mut [VisitState],
) -> Option<CatalogDeclarationKind> {
    match states.get(index).copied() {
        Some(VisitState::Complete) => return declarations.get(index)?.kind,
        Some(VisitState::Visiting) => {
            let declaration = declarations.get(index)?;

            diagnostics.push(
                CatalogDiagnostic::new(declaration.anchor, CatalogDiagnosticKind::OwnershipCycle)
                    .with_related_keys([CatalogRelatedKey::new(
                        declaration_domain(declaration),
                        Arc::clone(&declaration.key),
                    )]),
            );

            return None;
        }
        Some(VisitState::Unvisited) => {}
        None => return None,
    }

    states[index] = VisitState::Visiting;

    let declaration = declarations.get(index)?;
    let owner = declaration.owner;
    let surface = declaration.surface;
    let anchor = declaration.anchor;

    let context = match owner {
        Some(ValidatedDeclarationOwner::Scope(_)) => CatalogSurfaceContext::Scope,
        Some(ValidatedDeclarationOwner::Declaration(owner_index)) => {
            let Some(owner_kind) = resolve_kind(
                owner_index,
                declarations,
                inventory,
                validator,
                diagnostics,
                states,
            ) else {
                states[index] = VisitState::Complete;

                return None;
            };

            CatalogSurfaceContext::Declaration(owner_kind)
        }
        None => {
            states[index] = VisitState::Complete;

            return None;
        }
    };

    let source = inventory.source(surface.anchor().source())?;

    let kind = match validator.validate_declaration_surface(*source, surface, context) {
        Ok(surface_kind) => match declarations[index].declared_kind {
            Some(CatalogDeclarationKind::TrustedCapability)
                if surface_kind != CatalogDeclarationKind::Predicate =>
            {
                diagnostics.push(CatalogDiagnostic::new(
                    anchor,
                    CatalogDiagnosticKind::InvalidDeclarationSurface,
                ));

                None
            }
            Some(kind) => Some(kind),
            None => Some(surface_kind),
        },
        Err(fragment_diagnostics) => {
            diagnostics.extend_from_slice(fragment_diagnostics.diagnostics());

            None
        }
    };

    if let Some(kind) = kind {
        validate_context(context, kind, anchor, diagnostics);

        if let Some(hook) = declarations[index].implementation_hook
            && !implementation_kind(kind)
        {
            diagnostics.push(CatalogDiagnostic::new(
                anchor,
                CatalogDiagnosticKind::IncompatibleImplementationHook {
                    hook,
                    declaration: kind,
                },
            ));
        }

        if let Some(role) = declarations[index].representation_role {
            validate_declaration_representation(role, kind, anchor, diagnostics);
        }

        declarations[index].kind = Some(kind);
    }

    states[index] = VisitState::Complete;

    kind
}

fn validate_context(
    context: CatalogSurfaceContext,
    child: CatalogDeclarationKind,
    anchor: super::super::CatalogSourceAnchor,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let valid = match child {
        CatalogDeclarationKind::TrustedCapability => context == CatalogSurfaceContext::Scope,
        CatalogDeclarationKind::StructField => {
            context == CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Struct)
        }
        CatalogDeclarationKind::UnionVariant => {
            context == CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Union)
        }
        CatalogDeclarationKind::UnionPayloadField => {
            context == CatalogSurfaceContext::Declaration(CatalogDeclarationKind::UnionVariant)
        }
        CatalogDeclarationKind::TraitConstantMember
        | CatalogDeclarationKind::TraitTypeMember
        | CatalogDeclarationKind::TraitPredicateMember
        | CatalogDeclarationKind::TraitCallableMember
        | CatalogDeclarationKind::TraitFinalizerRequirement
        | CatalogDeclarationKind::TraitDestructorRequirement
        | CatalogDeclarationKind::TraitScopeEnterRequirement
        | CatalogDeclarationKind::TraitScopeExitRequirement => {
            context == CatalogSurfaceContext::Declaration(CatalogDeclarationKind::Trait)
        }
        CatalogDeclarationKind::ImplementationTypeMemberBinding => matches!(
            context,
            CatalogSurfaceContext::Declaration(
                CatalogDeclarationKind::InherentImplementation
                    | CatalogDeclarationKind::UnnamedTraitImplementation
                    | CatalogDeclarationKind::NamedTraitImplementation
            )
        ),
        _ => true,
    };

    if !valid {
        let owner = match context {
            CatalogSurfaceContext::Scope => None,
            CatalogSurfaceContext::Declaration(kind) => Some(kind),
        };

        diagnostics.push(CatalogDiagnostic::new(
            anchor,
            CatalogDiagnosticKind::InvalidDeclarationContext { owner, child },
        ));
    }
}

fn implementation_kind(kind: CatalogDeclarationKind) -> bool {
    matches!(
        kind,
        CatalogDeclarationKind::Function
            | CatalogDeclarationKind::Predicate
            | CatalogDeclarationKind::TypeConstructorMember
            | CatalogDeclarationKind::TypeCallableMember
            | CatalogDeclarationKind::TraitCallableMember
    )
}

fn validate_declaration_representation(
    role: RepresentationRole,
    kind: CatalogDeclarationKind,
    anchor: super::super::CatalogSourceAnchor,
    diagnostics: &mut Vec<CatalogDiagnostic>,
) {
    let value_role = matches!(
        role,
        RepresentationRole::BooleanTrue
            | RepresentationRole::BooleanFalse
            | RepresentationRole::UnitValue
            | RepresentationRole::NoneValue
    );

    let type_declaration = matches!(
        kind,
        CatalogDeclarationKind::Struct | CatalogDeclarationKind::Union
    );

    if value_role || !type_declaration {
        diagnostics.push(CatalogDiagnostic::new(
            anchor,
            CatalogDiagnosticKind::IncompatibleRepresentationRole {
                role,
                entry: CatalogEntryKind::Declaration,
            },
        ));
    }
}

fn declaration_domain(declaration: &RawDeclaration) -> CatalogKeyDomain {
    match declaration.catalog_kind {
        super::super::CatalogKind::CompilerKnown => CatalogKeyDomain::CompilerKnownDeclaration,
        super::super::CatalogKind::RecognizedStandardLibrary => {
            CatalogKeyDomain::RecognizedStandardLibraryDeclaration
        }
    }
}
