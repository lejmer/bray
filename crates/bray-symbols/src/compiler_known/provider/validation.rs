use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::{
    CatalogDeclarationKind, CatalogScopeLocation, CompilerKnownCatalog, CompilerKnownDeclarationId,
    CompilerKnownDeclarationOwner, CompilerKnownScopeId,
};

use super::super::CompilerKnownSymbolBuildError;
use crate::SymbolKind;
use crate::surface_kind::declaration_symbol_kind;

type DescriptorSurface = (CompilerKnownDeclarationOwner, CatalogDeclarationKind);

pub(super) fn resolve_declaration_symbol_kinds(
    catalog: &CompilerKnownCatalog,
) -> Result<BTreeMap<CompilerKnownDeclarationId, SymbolKind>, CompilerKnownSymbolBuildError> {
    let mut descriptors = BTreeMap::new();

    for (index, descriptor) in catalog.compiler_known_declarations().iter().enumerate() {
        validate_declaration_id(index, descriptor.id())?;
        descriptors.insert(descriptor.id(), (descriptor.owner(), descriptor.kind()));
    }

    resolve_symbol_kinds(
        descriptors.keys().copied(),
        |id| descriptors.get(&id).copied(),
        |id| {
            catalog
                .compiler_known_scope(id)
                .map(|scope| match scope.location() {
                    CatalogScopeLocation::Ambient => SymbolKind::CompilerKnownEnvironment,
                    CatalogScopeLocation::Module(_) => SymbolKind::Module,
                })
        },
    )
}

pub(super) fn validate_scope_id(
    index: usize,
    actual: CompilerKnownScopeId,
) -> Result<(), CompilerKnownSymbolBuildError> {
    let Some(expected) = CompilerKnownScopeId::try_from_index(index) else {
        return Err(CompilerKnownSymbolBuildError::SymbolCapacityExceeded { index });
    };

    if actual != expected {
        return Err(CompilerKnownSymbolBuildError::NonCanonicalScopeId { expected, actual });
    }

    Ok(())
}

pub(super) fn validate_declaration_id(
    index: usize,
    actual: CompilerKnownDeclarationId,
) -> Result<(), CompilerKnownSymbolBuildError> {
    let Some(expected) = CompilerKnownDeclarationId::try_from_index(index) else {
        return Err(CompilerKnownSymbolBuildError::SymbolCapacityExceeded { index });
    };

    if actual != expected {
        return Err(CompilerKnownSymbolBuildError::NonCanonicalDeclarationId { expected, actual });
    }

    Ok(())
}

fn resolve_symbol_kinds(
    declarations: impl IntoIterator<Item = CompilerKnownDeclarationId>,
    descriptor: impl Fn(CompilerKnownDeclarationId) -> Option<DescriptorSurface>,
    scope_kind: impl Fn(CompilerKnownScopeId) -> Option<SymbolKind>,
) -> Result<BTreeMap<CompilerKnownDeclarationId, SymbolKind>, CompilerKnownSymbolBuildError> {
    let mut resolved = BTreeMap::new();
    let mut resolving = BTreeSet::new();

    for declaration in declarations {
        resolve_symbol_kind(
            declaration,
            &descriptor,
            &scope_kind,
            &mut resolved,
            &mut resolving,
        )?;
    }

    Ok(resolved)
}

fn resolve_symbol_kind(
    declaration: CompilerKnownDeclarationId,
    descriptor: &impl Fn(CompilerKnownDeclarationId) -> Option<DescriptorSurface>,
    scope_kind: &impl Fn(CompilerKnownScopeId) -> Option<SymbolKind>,
    resolved: &mut BTreeMap<CompilerKnownDeclarationId, SymbolKind>,
    resolving: &mut BTreeSet<CompilerKnownDeclarationId>,
) -> Result<SymbolKind, CompilerKnownSymbolBuildError> {
    if let Some(kind) = resolved.get(&declaration).copied() {
        return Ok(kind);
    }

    if !resolving.insert(declaration) {
        return Err(CompilerKnownSymbolBuildError::DeclarationOwnerCycle { declaration });
    }

    let Some((owner, declaration_kind)) = descriptor(declaration) else {
        return Err(CompilerKnownSymbolBuildError::InvalidDeclarationSurface { declaration });
    };

    let owner_kind = match owner {
        CompilerKnownDeclarationOwner::Scope(scope) => {
            let Some(kind) = scope_kind(scope) else {
                return Err(CompilerKnownSymbolBuildError::MissingScopeOwner {
                    declaration,
                    scope,
                });
            };

            kind
        }
        CompilerKnownDeclarationOwner::Declaration(owner) => {
            if descriptor(owner).is_none() {
                return Err(CompilerKnownSymbolBuildError::MissingDeclarationOwner {
                    declaration,
                    owner,
                });
            }

            resolve_symbol_kind(owner, descriptor, scope_kind, resolved, resolving)?
        }
    };

    let kind = declaration_symbol_kind(declaration_kind.into(), owner_kind);

    resolving.remove(&declaration);
    resolved.insert(declaration, kind);

    Ok(kind)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bray_compiler_known::{
        CatalogDeclarationKind, CompilerKnownDeclarationId, CompilerKnownDeclarationOwner,
        CompilerKnownScopeId,
    };

    use super::{resolve_symbol_kinds, validate_declaration_id, validate_scope_id};
    use crate::{CompilerKnownSymbolBuildError, SymbolKind};

    #[test]
    fn nested_implementation_members_use_the_resolved_owner_symbol_kind() {
        let callable = CompilerKnownDeclarationId::new(0);
        let implementation = CompilerKnownDeclarationId::new(1);
        let scope = CompilerKnownScopeId::new(0);

        let descriptors = BTreeMap::from([
            (
                callable,
                (
                    CompilerKnownDeclarationOwner::Declaration(implementation),
                    CatalogDeclarationKind::TypeCallableMember,
                ),
            ),
            (
                implementation,
                (
                    CompilerKnownDeclarationOwner::Scope(scope),
                    CatalogDeclarationKind::NamedTraitImplementation,
                ),
            ),
        ]);

        let result = resolve_symbol_kinds(
            [callable, implementation],
            |id| descriptors.get(&id).copied(),
            |id| (id == scope).then_some(SymbolKind::CompilerKnownEnvironment),
        );

        let kinds = match result {
            Ok(kinds) => kinds,
            Err(error) => panic!("nested test descriptors must resolve: {error:?}"),
        };

        assert_eq!(
            kinds.get(&implementation),
            Some(&SymbolKind::NamedTraitImplementation)
        );

        assert_eq!(
            kinds.get(&callable),
            Some(&SymbolKind::TraitCallableFulfillment)
        );
    }

    #[test]
    fn owner_cycles_are_reported_without_recursion_overflow() {
        let first = CompilerKnownDeclarationId::new(0);
        let second = CompilerKnownDeclarationId::new(1);

        let descriptors = BTreeMap::from([
            (
                first,
                (
                    CompilerKnownDeclarationOwner::Declaration(second),
                    CatalogDeclarationKind::TypeCallableMember,
                ),
            ),
            (
                second,
                (
                    CompilerKnownDeclarationOwner::Declaration(first),
                    CatalogDeclarationKind::NamedTraitImplementation,
                ),
            ),
        ]);

        assert_eq!(
            resolve_symbol_kinds(
                [first, second],
                |id| descriptors.get(&id).copied(),
                |_| Some(SymbolKind::CompilerKnownEnvironment),
            ),
            Err(CompilerKnownSymbolBuildError::DeclarationOwnerCycle { declaration: first })
        );
    }

    #[test]
    fn missing_declaration_owners_are_reported() {
        let declaration = CompilerKnownDeclarationId::new(0);
        let owner = CompilerKnownDeclarationId::new(1);

        assert_eq!(
            resolve_symbol_kinds(
                [declaration],
                |id| {
                    (id == declaration).then_some((
                        CompilerKnownDeclarationOwner::Declaration(owner),
                        CatalogDeclarationKind::TypeCallableMember,
                    ))
                },
                |_| Some(SymbolKind::CompilerKnownEnvironment),
            ),
            Err(CompilerKnownSymbolBuildError::MissingDeclarationOwner { declaration, owner })
        );
    }

    #[test]
    fn missing_scope_owners_are_reported() {
        let declaration = CompilerKnownDeclarationId::new(0);
        let scope = CompilerKnownScopeId::new(0);

        assert_eq!(
            resolve_symbol_kinds(
                [declaration],
                |id| {
                    (id == declaration).then_some((
                        CompilerKnownDeclarationOwner::Scope(scope),
                        CatalogDeclarationKind::Function,
                    ))
                },
                |_| None,
            ),
            Err(CompilerKnownSymbolBuildError::MissingScopeOwner { declaration, scope })
        );
    }

    #[test]
    fn compact_descriptor_ids_require_canonical_positions() {
        assert_eq!(validate_scope_id(0, CompilerKnownScopeId::new(0)), Ok(()));

        assert_eq!(
            validate_declaration_id(0, CompilerKnownDeclarationId::new(1)),
            Err(CompilerKnownSymbolBuildError::NonCanonicalDeclarationId {
                expected: CompilerKnownDeclarationId::new(0),
                actual: CompilerKnownDeclarationId::new(1),
            })
        );
    }
}
