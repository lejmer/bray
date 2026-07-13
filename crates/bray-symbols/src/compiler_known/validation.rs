use std::collections::BTreeSet;

use bray_compiler_known::{
    AvailabilityRule, CatalogScopeLocation, CompilerKnownDeclarationId,
    CompilerKnownDeclarationOwner, CompilerKnownRepresentationTarget,
};
use bray_diagnostics::DiagnosticBag;

use super::{
    audit::{
        CompilerKnownCatalogAudit, CompilerKnownCatalogAuditError, CompilerKnownCatalogAuditReport,
        CompilerKnownTargetProfile,
    },
    role::RepresentationTarget,
};
use crate::{
    AnySymbolId, ModulePathKey, NeverCancelSymbolCompletion, SymbolCompletionLevel,
    SymbolFactCompletionRequest, SymbolFactForcer, SymbolGraph, SymbolKey, SymbolKeyData,
    SymbolRootKey,
};

impl<'graph> CompilerKnownCatalogAudit<'graph> {
    /// Audits stable identities, ownership, target views, roles, and completion coverage.
    pub fn new(graph: &'graph SymbolGraph) -> Result<Self, CompilerKnownCatalogAuditError> {
        audit_stable_identities(graph)?;
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
                completion_facts: plan.requests().len(),
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

impl SymbolFactForcer for CompilerKnownCatalogAudit<'_> {
    type Error = CompilerKnownCatalogAuditError;

    fn force(&self, request: SymbolFactCompletionRequest) -> Result<DiagnosticBag, Self::Error> {
        if !request.kind().is_applicable_to(request.symbol()) {
            return Err(CompilerKnownCatalogAuditError::InvalidCompletionFact {
                symbol: request.symbol(),
            });
        }

        let provider = self.graph.compiler_known_provider();

        let is_scope = provider
            .scope_symbols()
            .values()
            .copied()
            .any(|symbol| symbol.into_any() == request.symbol());

        if is_scope {
            return Ok(DiagnosticBag::new());
        }

        let Some(declaration) = provider.declaration_id(request.symbol()) else {
            return Err(CompilerKnownCatalogAuditError::InvalidCompletionFact {
                symbol: request.symbol(),
            });
        };

        let catalog = provider.catalog();

        let Some(descriptor) = catalog.compiler_known_declaration(declaration) else {
            return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                declaration,
            ));
        };

        let Some(surface) = catalog.declaration_surface(descriptor.surface()) else {
            return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                declaration,
            ));
        };

        if surface.kind() != descriptor.kind() {
            return Err(CompilerKnownCatalogAuditError::InvalidDeclaration(
                declaration,
            ));
        }

        Ok(DiagnosticBag::new())
    }
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

    for binding in catalog.role_registry().representations() {
        let expected = match binding.target() {
            CompilerKnownRepresentationTarget::Declaration(declaration) => {
                RepresentationTarget::Symbol(declaration_symbol(graph, declaration)?)
            }
            CompilerKnownRepresentationTarget::Value(value) => RepresentationTarget::Value(value),
        };

        if provider
            .role_registry()
            .representation_target(binding.role())
            != Some(expected)
        {
            return Err(CompilerKnownCatalogAuditError::InvalidRepresentationRole(
                binding.role(),
            ));
        }
    }

    let hooks = catalog
        .role_registry()
        .implementations()
        .iter()
        .map(|binding| binding.hook())
        .collect::<BTreeSet<_>>();

    for hook in hooks {
        let expected = catalog
            .role_registry()
            .implementation_declarations(hook)
            .map(|declaration| declaration_symbol(graph, declaration))
            .collect::<Result<Vec<_>, _>>()?;

        if provider.role_registry().implementation_symbol_ids(hook) != expected {
            return Err(CompilerKnownCatalogAuditError::InvalidImplementationRole(
                hook,
            ));
        }
    }

    Ok(())
}

fn audit_target_view(
    graph: &SymbolGraph,
    profile: CompilerKnownTargetProfile,
    mut rule_is_available: impl FnMut(AvailabilityRule) -> bool,
) -> Result<(), CompilerKnownCatalogAuditError> {
    let provider = graph.compiler_known_provider();
    let catalog = provider.catalog();
    let view = graph.available_compiler_known_symbols(&mut rule_is_available);

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

    for binding in catalog.role_registry().representations() {
        let expected = match binding.target() {
            CompilerKnownRepresentationTarget::Declaration(declaration) => {
                let symbol = declaration_symbol(graph, declaration)?;

                view.contains(symbol)
                    .then_some(RepresentationTarget::Symbol(symbol))
            }
            CompilerKnownRepresentationTarget::Value(value) => view
                .contains_value(value)
                .then_some(RepresentationTarget::Value(value)),
        };

        if view.representation_target(binding.role()) != expected {
            return Err(
                CompilerKnownCatalogAuditError::InvalidTargetRepresentationRole {
                    profile,
                    role: binding.role(),
                },
            );
        }
    }

    let hooks = catalog
        .role_registry()
        .implementations()
        .iter()
        .map(|binding| binding.hook())
        .collect::<BTreeSet<_>>();

    for hook in hooks {
        let expected = provider
            .role_registry()
            .implementation_symbol_ids(hook)
            .iter()
            .copied()
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
