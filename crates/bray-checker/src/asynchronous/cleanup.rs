use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncScopeExitPlan, BoundUnitKind, StorageAccessId, StorageFlowFacts, StorageIdentity,
    StorageIdentityId, StoragePlan,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{CallableSymbolId, GenericArgument, TypeData, TypeId};

use crate::{CheckerFactError, CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CleanupShape {
    pub(super) cancellation: bool,
    pub(super) lifecycle: bool,
    recovered: bool,
}

impl CleanupShape {
    const BOTH: Self = Self {
        cancellation: true,
        lifecycle: true,
        recovered: false,
    };

    const LIFECYCLE: Self = Self {
        cancellation: false,
        lifecycle: true,
        recovered: false,
    };

    fn merge(&mut self, other: Self) {
        self.cancellation |= other.cancellation;
        self.lifecycle |= other.lifecycle;
        self.recovered |= other.recovered;
    }
}

pub(super) struct CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'request, C>,
    completed: BTreeMap<TypeId, CleanupShape>,
    active: BTreeSet<TypeId>,
    diagnostics: DiagnosticBag,
}

impl<'request, C> CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn new(request: CheckerUnitView<'request, C>) -> Self {
        Self {
            request,
            completed: BTreeMap::new(),
            active: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(super) fn resolve(&mut self, ty: TypeId) -> Result<CleanupShape, CheckerFactError> {
        if let Some(shape) = self.completed.get(&ty) {
            return Ok(*shape);
        }

        if !self.active.insert(ty) {
            return Ok(CleanupShape::BOTH);
        }

        let data = self.request.semantic_values().type_data(ty).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })?;

        let shape = match data.as_ref() {
            TypeData::Error => CleanupShape {
                recovered: true,
                ..CleanupShape::BOTH
            },
            TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Generator(_)
            | TypeData::Callable(_) => CleanupShape::BOTH,
            TypeData::Named {
                definition,
                substitution,
            } => self.named_shape(*definition, *substitution)?,
            TypeData::Tuple(elements) => self.aggregate(elements.iter().copied())?,
            TypeData::Array { element, .. } | TypeData::Nullable(element) => {
                self.resolve(*element)?
            }
            TypeData::OwnedIndirection { target, .. } => {
                let mut shape = self.resolve(*target)?;

                shape.lifecycle = true;

                shape
            }
            TypeData::Slice(_) | TypeData::Borrow { .. } | TypeData::TraitView(_) => {
                CleanupShape::default()
            }
        };

        self.active.remove(&ty);
        self.completed.insert(ty, shape);

        Ok(shape)
    }

    fn named_shape(
        &mut self,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerFactError> {
        let role = match definition {
            bray_symbols::NamedTypeSymbolId::Struct(definition) => self
                .request
                .available_compiler_known_symbols()
                .provider()
                .role_registry()
                .symbol_representation(definition),
            bray_symbols::NamedTypeSymbolId::Union(definition) => self
                .request
                .available_compiler_known_symbols()
                .provider()
                .role_registry()
                .symbol_representation(definition),
        };

        match role {
            Some(RepresentationRole::Future | RepresentationRole::Task) => Ok(CleanupShape::BOTH),
            Some(RepresentationRole::String | RepresentationRole::PanicReport) => {
                Ok(CleanupShape::LIFECYCLE)
            }
            Some(
                RepresentationRole::Result
                | RepresentationRole::RunResult
                | RepresentationRole::ConversionError,
            ) => {
                let substitution = self
                    .request
                    .semantic_values()
                    .generic_substitution_data(substitution)
                    .map_err(|_| {
                        CheckerFactError::Infrastructure(
                            CheckerInfrastructureError::SemanticValueUnavailable,
                        )
                    })?;

                self.aggregate(substitution.bindings().iter().filter_map(|binding| {
                    match binding.argument() {
                        GenericArgument::Type(ty) => Some(ty),
                        GenericArgument::Constant(_) => None,
                    }
                }))
            }
            Some(_) => Ok(CleanupShape::default()),
            None => {
                let representation = self.request.declared_type_representation(definition)?;
                let recovered = representation.value().is_recovered();

                self.diagnostics
                    .add_range(representation.diagnostics().clone());

                Ok(if representation.value().is_plain_storage() {
                    CleanupShape {
                        recovered,
                        ..CleanupShape::default()
                    }
                } else {
                    CleanupShape {
                        recovered,
                        ..CleanupShape::BOTH
                    }
                })
            }
        }
    }

    fn aggregate(
        &mut self,
        types: impl IntoIterator<Item = TypeId>,
    ) -> Result<CleanupShape, CheckerFactError> {
        let mut shape = CleanupShape::default();

        for ty in types {
            shape.merge(self.resolve(ty)?);
        }

        Ok(shape)
    }

    pub(super) fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }
}

pub(super) fn scope_exit_plans<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &StorageFlowFacts,
) -> Result<(Vec<AsyncScopeExitPlan>, DiagnosticBag), CheckerFactError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut cleanup_shapes = CleanupShapeResolver::new(request);
    let mut plans = Vec::new();

    for exit in flow.exits() {
        let mut cancellation = Vec::new();
        let mut lifecycle = Vec::new();
        let mut is_recovered = exit.is_recovered();

        for identity in exit.initialized().iter().rev().copied() {
            if scope_exit_transfers_identity(request, storage, identity) {
                continue;
            }

            let Some(access) = root_access(storage, identity) else {
                is_recovered = true;

                continue;
            };

            if exit
                .moved()
                .iter()
                .any(|moved| storage.access_contains(*moved, access))
            {
                continue;
            }

            let Some(access_data) = storage.access(access) else {
                is_recovered = true;

                continue;
            };

            let shape = cleanup_shapes.resolve(access_data.reached_type())?;

            is_recovered |= shape.recovered;

            if shape.cancellation {
                cancellation.push(access);
            }

            if shape.lifecycle {
                lifecycle.push(access);
            }
        }

        plans.push(AsyncScopeExitPlan::new(
            exit.scope(),
            cancellation,
            lifecycle,
            exit.moved().iter().copied(),
            is_recovered,
        ));
    }

    Ok((plans, cleanup_shapes.into_diagnostics()))
}

fn scope_exit_transfers_identity<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    identity: StorageIdentityId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    match storage.identity(identity) {
        Some(StorageIdentity::Result(_)) => true,
        Some(StorageIdentity::Receiver(_)) => {
            request.unit().key().kind() == BoundUnitKind::CallableBody
                && matches!(
                    request.containing_callable(),
                    Some(CallableSymbolId::Destructor(_))
                )
        }
        Some(
            StorageIdentity::LocalOwned(_)
            | StorageIdentity::Parameter(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::PredicateParameter(_)
            | StorageIdentity::PostconditionResult(_)
            | StorageIdentity::Temporary(_)
            | StorageIdentity::IterationCursor(_)
            | StorageIdentity::IterationElement(_)
            | StorageIdentity::Allocation(_)
            | StorageIdentity::CompilerCreated(_)
            | StorageIdentity::Alternative { .. }
            | StorageIdentity::Error(_),
        )
        | None => false,
    }
}

fn root_access(storage: &StoragePlan, identity: StorageIdentityId) -> Option<StorageAccessId> {
    storage.access_entries().find_map(|(access, _)| {
        (storage.root_identity(access) == Some(identity) && storage.is_root_access(access))
            .then_some(access)
    })
}
