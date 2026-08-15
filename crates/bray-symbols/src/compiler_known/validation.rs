use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::{
    AvailabilityRule, CatalogScopeLocation, CompilerKnownDeclarationId,
    CompilerKnownDeclarationOwner, CompilerKnownRepresentationTarget, ImplementationHook,
    RepresentationRole,
};

use super::{
    audit::{
        CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError, CompilerKnownCatalogAuditReport,
        CompilerKnownTargetProfile,
    },
    role::RepresentationTarget,
};
use crate::{
    AnySymbolId, ModulePathKey, NeverCancelSymbolCompletion, SymbolCompletionLevel, SymbolGraph,
    SymbolKey, SymbolKeyData, SymbolRootKey,
};

impl<'graph> CompilerKnownCatalogAudit<'graph> {
    /// Audits stable identities, ownership, target views, roles, and completion coverage.
    pub fn new(graph: &'graph SymbolGraph) -> Result<Self, CompilerKnownCatalogAuditError> {
        audit_stable_identities(graph)?;
        audit_closed_catalog(graph)?;
        audit_roles(graph)?;

        let root = graph.roots().compiler_known().into();

        let plan = graph
            .completion_plan(
                root,
                SymbolCompletionLevel::DeclarationSurface,
                &NeverCancelSymbolCompletion,
            )
            .map_err(|_| CompilerKnownCatalogAuditError::InvalidCompletionPlan)?;

        audit_completion_coverage(graph, &plan)?;

        let provider = graph.compiler_known_provider();
        let catalog = provider.catalog();

        Ok(Self {
            graph,
            report: CompilerKnownCatalogAuditReport {
                scopes: catalog.compiler_known_scopes().len(),
                declarations: catalog.compiler_known_declarations().len(),
                values: catalog.compiler_known_values().len(),
                representation_roles: catalog.role_registry().representations().len(),
                implementation_roles: catalog.role_registry().implementations().len(),
                completion_units: plan.units().len(),
                completion_queries: plan.requests().len(),
                target_profiles: 0,
            },
        })
    }

    /// Audits one target-filtered catalog view against its availability predicate.
    pub fn audit_target_view(
        &mut self,
        profile: CompilerKnownTargetProfile,
        mut rule_is_available: impl FnMut(AvailabilityRule) -> bool,
    ) -> Result<(), CompilerKnownCatalogAuditError> {
        audit_target_view(self.graph, profile, &mut rule_is_available)?;
        self.report.target_profiles += 1;

        Ok(())
    }

    /// Returns the deterministic audit summary.
    pub const fn report(&self) -> CompilerKnownCatalogAuditReport {
        self.report
    }
}

fn audit_closed_catalog(graph: &SymbolGraph) -> Result<(), CompilerKnownCatalogAuditError> {
    let catalog = graph.compiler_known_provider().catalog();

    for role in RepresentationRole::ALL {
        if catalog
            .role_registry()
            .representation_target(*role)
            .is_none()
        {
            return Err(CompilerKnownCatalogAuditError::MissingRepresentationRole(
                *role,
            ));
        }
    }

    for hook in ImplementationHook::ALL {
        let compiler_known = catalog
            .role_registry()
            .implementation_declarations(*hook)
            .next()
            .is_some();

        let recognized = catalog
            .recognized_standard_library_declarations()
            .iter()
            .any(|descriptor| descriptor.implementation_hook() == Some(*hook));

        if !compiler_known && !recognized {
            return Err(CompilerKnownCatalogAuditError::MissingImplementationHook(
                *hook,
            ));
        }
    }

    Ok(())
}

fn audit_stable_identities(graph: &SymbolGraph) -> Result<(), CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();
    let catalog = provider.catalog();

    if provider.scope_symbols().len() != catalog.compiler_known_scopes().len() {
        return Err(CompilerKnownCatalogAuditError::StableScopeCount);
    }

    for scope in catalog.compiler_known_scopes() {
        let Some(symbol) = provider
            .scope_symbol(scope.key())
            .map(|symbol| symbol.into_any())
        else {
            return Err(CompilerKnownCatalogAuditError::InvalidScope(scope.id()));
        };

        let (expected_key, expected_owner) = match scope.location() {
            CatalogScopeLocation::Ambient => (SymbolKey::compiler_known_environment(), None),
            CatalogScopeLocation::Module(path) => {
                let Some(path) = ModulePathKey::try_new(path.segments()) else {
                    return Err(CompilerKnownCatalogAuditError::InvalidScope(scope.id()));
                };

                (
                    SymbolKey::module(SymbolRootKey::CompilerKnownEnvironment, path),
                    Some(graph.roots().compiler_known().into()),
                )
            }
        };

        if graph.symbol_key(symbol) != Some(&expected_key)
            || graph.containing_symbol(symbol) != expected_owner
        {
            return Err(CompilerKnownCatalogAuditError::InvalidScope(scope.id()));
        }
    }

    if provider.declaration_symbols().len() != catalog.compiler_known_declarations().len() {
        return Err(CompilerKnownCatalogAuditError::StableDeclarationCount);
    }

    for descriptor in catalog.compiler_known_declarations() {
        let Some(symbol) = provider
            .declaration_symbols()
            .get(descriptor.key())
            .copied()
        else {
            return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                descriptor.id(),
            ));
        };

        let Some(key) = graph.symbol_key(symbol) else {
            return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                descriptor.id(),
            ));
        };

        if !matches!(
            key.data(),
            SymbolKeyData::CompilerKnownDeclaration { key, kind }
                if key == descriptor.key() && *kind == symbol.kind()
        ) {
            return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                descriptor.id(),
            ));
        }

        let expected_owner = expected_owner(graph, descriptor.owner())?;

        if graph.containing_symbol(symbol) != Some(expected_owner) {
            return Err(CompilerKnownCatalogAuditError::InvalidOwner(
                descriptor.id(),
            ));
        }
    }

    Ok(())
}

fn expected_owner(
    graph: &SymbolGraph,
    owner: CompilerKnownDeclarationOwner,
) -> Result<AnySymbolId, CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();
    let catalog = provider.catalog();

    match owner {
        CompilerKnownDeclarationOwner::Scope(scope) => {
            let Some(descriptor) = catalog.compiler_known_scope(scope) else {
                return Err(CompilerKnownCatalogAuditError::InvalidScope(scope));
            };

            provider
                .scope_symbol(descriptor.key())
                .map(|symbol| symbol.into_any())
                .ok_or(CompilerKnownCatalogAuditError::InvalidScope(scope))
        }
        CompilerKnownDeclarationOwner::Declaration(declaration) => {
            let Some(descriptor) = catalog.compiler_known_declaration(declaration) else {
                return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                    declaration,
                ));
            };

            provider
                .declaration_symbols()
                .get(descriptor.key())
                .copied()
                .ok_or(CompilerKnownCatalogAuditError::InvalidDeclaration(
                    declaration,
                ))
        }
    }
}

fn audit_roles(graph: &SymbolGraph) -> Result<(), CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();
    let catalog = provider.catalog();
    let expected = expected_roles(graph)?;

    if catalog.role_registry().representations().len() != expected.representations.len() {
        return Err(CompilerKnownCatalogAuditError::InvalidRepresentationRegistry);
    }

    for (role, expected_target) in &expected.representations {
        let Some(catalog_target) = catalog.role_registry().representation_target(*role) else {
            return Err(CompilerKnownCatalogAuditError::InvalidRepresentationRole(
                *role,
            ));
        };

        let catalog_target = representation_target(graph, catalog_target)?;

        if catalog_target != *expected_target
            || provider.role_registry().representation_target(*role) != Some(*expected_target)
        {
            return Err(CompilerKnownCatalogAuditError::InvalidRepresentationRole(
                *role,
            ));
        }
    }

    let expected_implementation_count = expected
        .implementations
        .values()
        .map(Vec::len)
        .sum::<usize>();

    if catalog.role_registry().implementations().len() != expected_implementation_count {
        return Err(CompilerKnownCatalogAuditError::InvalidImplementationRegistry);
    }

    for (hook, expected_symbols) in &expected.implementations {
        let catalog_symbols = catalog
            .role_registry()
            .implementation_declarations(*hook)
            .map(|declaration| declaration_symbol(graph, declaration))
            .collect::<Result<Vec<_>, _>>()?;

        if catalog_symbols != *expected_symbols
            || provider.role_registry().implementation_symbol_ids(*hook) != expected_symbols
        {
            return Err(CompilerKnownCatalogAuditError::InvalidImplementationRole(
                *hook,
            ));
        }
    }

    Ok(())
}

struct ExpectedCompilerKnownRoles {
    representations: BTreeMap<bray_compiler_known::RepresentationRole, RepresentationTarget>,
    implementations: BTreeMap<bray_compiler_known::ImplementationHook, Vec<AnySymbolId>>,
}

fn expected_roles(
    graph: &SymbolGraph,
) -> Result<ExpectedCompilerKnownRoles, CompilerKnownCatalogAuditError> {
    let catalog = graph.compiler_known_provider().catalog();
    let mut representations = BTreeMap::new();
    let mut implementations = BTreeMap::<_, Vec<_>>::new();

    for descriptor in catalog.compiler_known_declarations() {
        let symbol = declaration_symbol(graph, descriptor.id())?;

        if let Some(role) = descriptor.representation_role()
            && representations
                .insert(role, RepresentationTarget::Symbol(symbol))
                .is_some()
        {
            return Err(CompilerKnownCatalogAuditError::InvalidRepresentationRole(
                role,
            ));
        }

        if let Some(hook) = descriptor.implementation_hook() {
            implementations.entry(hook).or_default().push(symbol);
        }
    }

    for value in catalog.compiler_known_values() {
        let role = value.representation_role();

        if representations
            .insert(role, RepresentationTarget::Value(value.id()))
            .is_some()
        {
            return Err(CompilerKnownCatalogAuditError::InvalidRepresentationRole(
                role,
            ));
        }
    }

    Ok(ExpectedCompilerKnownRoles {
        representations,
        implementations,
    })
}

fn representation_target(
    graph: &SymbolGraph,
    target: CompilerKnownRepresentationTarget,
) -> Result<RepresentationTarget, CompilerKnownCatalogAuditError> {
    match target {
        CompilerKnownRepresentationTarget::Declaration(declaration) => {
            declaration_symbol(graph, declaration).map(RepresentationTarget::Symbol)
        }
        CompilerKnownRepresentationTarget::Value(value) => Ok(RepresentationTarget::Value(value)),
    }
}

fn audit_target_view(
    graph: &SymbolGraph,
    profile: CompilerKnownTargetProfile,
    mut rule_is_available: impl FnMut(AvailabilityRule) -> bool,
) -> Result<(), CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();
    let catalog = provider.catalog();
    let view = graph.available_compiler_known_symbols(&mut rule_is_available);
    let expected_roles = expected_roles(graph)?;

    for descriptor in catalog.compiler_known_declarations() {
        let symbol = declaration_symbol(graph, descriptor.id())?;

        let owner_available = match descriptor.owner() {
            CompilerKnownDeclarationOwner::Scope(_) => true,
            CompilerKnownDeclarationOwner::Declaration(owner) => {
                view.contains(declaration_symbol(graph, owner)?)
            }
        };

        let expected = rule_is_available(descriptor.availability_rule()) && owner_available;

        if view.contains(symbol) != expected {
            return Err(CompilerKnownCatalogAuditError::InvalidTargetDeclaration {
                profile,
                declaration: descriptor.id(),
            });
        }
    }

    for value in catalog.compiler_known_values() {
        if view.contains_value(value.id()) != rule_is_available(value.availability_rule()) {
            return Err(CompilerKnownCatalogAuditError::InvalidTargetValue {
                profile,
                value: value.id(),
            });
        }
    }

    for (role, target) in expected_roles.representations {
        let expected = match target {
            RepresentationTarget::Symbol(symbol) => view
                .contains(symbol)
                .then_some(RepresentationTarget::Symbol(symbol)),
            RepresentationTarget::Value(value) => view
                .contains_value(value)
                .then_some(RepresentationTarget::Value(value)),
        };

        if view.representation_target(role) != expected {
            return Err(
                CompilerKnownCatalogAuditError::InvalidTargetRepresentationRole { profile, role },
            );
        }
    }

    for (hook, symbols) in expected_roles.implementations {
        let expected = symbols
            .into_iter()
            .filter(|symbol| view.contains(*symbol))
            .collect::<Vec<_>>();

        if view.implementation_symbol_ids(hook).collect::<Vec<_>>() != expected {
            return Err(
                CompilerKnownCatalogAuditError::InvalidTargetImplementationRole { profile, hook },
            );
        }
    }

    Ok(())
}

fn audit_completion_coverage(
    graph: &SymbolGraph,
    plan: &crate::SymbolCompletionPlan,
) -> Result<(), CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();

    let expected = provider
        .declaration_symbols()
        .values()
        .copied()
        .collect::<BTreeSet<_>>();

    let actual = plan
        .units()
        .iter()
        .map(|unit| unit.symbol())
        .filter(|symbol| provider.declaration_id(*symbol).is_some())
        .collect::<BTreeSet<_>>();

    if actual != expected {
        return Err(CompilerKnownCatalogAuditError::IncompleteCompletion);
    }

    Ok(())
}

fn declaration_symbol(
    graph: &SymbolGraph,
    declaration: CompilerKnownDeclarationId,
) -> Result<AnySymbolId, CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();

    let Some(descriptor) = provider.catalog().compiler_known_declaration(declaration) else {
        return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
            declaration,
        ));
    };

    provider
        .declaration_symbols()
        .get(descriptor.key())
        .copied()
        .ok_or(CompilerKnownCatalogAuditError::InvalidDeclaration(
            declaration,
        ))
}
