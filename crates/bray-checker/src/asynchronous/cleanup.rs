use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncCleanupPhases, AsyncScopeExitPlan, AsyncStorageCleanupRequirement,
    AsyncStorageExitDecision, AsyncStorageExitDisposition, AsyncStorageExitRecoveryCause,
    AsyncStorageRequirement, StorageExitDecision, StorageFlow, StoragePlan,
    storage_identity_transfers_at_unit_exit,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{DeclaredStorageShape, GenericArgument, TypeData, TypeExpressionTemplate, TypeId};

use crate::storage::storage_scope_owners;
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

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

    pub(super) fn resolve(
        &mut self,
        ty: TypeId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        if let Some(shape) = self.completed.get(&ty) {
            return Ok(*shape);
        }

        if !self.active.insert(ty) {
            return Ok(CleanupShape::BOTH);
        }

        let data = self
            .request
            .semantic_values()
            .type_data(ty)
            .map_err(|error| {
                CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueStore(
                    error,
                ))
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
            TypeData::FlexibleArray(_)
            | TypeData::Slice(_)
            | TypeData::Borrow { .. }
            | TypeData::TraitView(_) => CleanupShape::default(),
        };

        self.active.remove(&ty);
        self.completed.insert(ty, shape);

        Ok(shape)
    }

    fn named_shape(
        &mut self,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
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
                    .map_err(|error| {
                        CheckerQueryError::Infrastructure(
                            CheckerInfrastructureError::SemanticValueStore(error),
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
            None => self.declared_shape(definition, substitution),
        }
    }

    fn declared_shape(
        &mut self,
        definition: bray_symbols::NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        let representation = self.request.declared_type_representation(definition)?;
        let lifecycle = self.request.declared_type_has_lifecycle(definition)?;

        let mut shape = CleanupShape {
            lifecycle: *lifecycle.value(),
            recovered: representation.value().is_recovered()
                || lifecycle.diagnostics().has_errors(),
            ..CleanupShape::default()
        };

        self.diagnostics
            .add_range(representation.diagnostics().clone());

        self.diagnostics.add_range(lifecycle.diagnostics().clone());

        match representation.value().storage() {
            DeclaredStorageShape::Structure(members) => {
                for member in members.iter() {
                    shape.merge(self.member_shape(member.ty(), substitution)?);
                }
            }
            DeclaredStorageShape::Union(variants) => {
                for member in variants.iter().flat_map(|variant| variant.members()) {
                    shape.merge(self.member_shape(member.ty(), substitution)?);
                }
            }
        }

        Ok(shape)
    }

    fn member_shape(
        &mut self,
        template: &TypeExpressionTemplate,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        let constants = self.request.checked_constant_terms(template)?;
        self.diagnostics.add_range(constants.diagnostics().clone());

        let Some(ty) = crate::resolve_type_expression_template(
            self.request.semantic_values(),
            template,
            constants.value(),
        )
        .map_err(CheckerQueryError::Infrastructure)?
        else {
            return Ok(CleanupShape {
                recovered: true,
                ..CleanupShape::BOTH
            });
        };

        let ty = self
            .request
            .semantic_values()
            .substitute_type(ty, substitution)
            .map_err(|error| {
                CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueStore(
                    error,
                ))
            })?;

        self.resolve(ty)
    }

    fn aggregate(
        &mut self,
        types: impl IntoIterator<Item = TypeId>,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
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
    flow: &StorageFlow,
) -> Result<
    (
        Vec<AsyncStorageRequirement>,
        Vec<AsyncScopeExitPlan>,
        DiagnosticBag,
    ),
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut cleanup_shapes = CleanupShapeResolver::new(request);
    let owners = storage_scope_owners(request).map_err(CheckerQueryError::with_upstream)?;

    let requirements = storage_requirements(request, storage, flow, &owners, &mut cleanup_shapes)?;

    let requirements_by_identity = requirements
        .iter()
        .map(|requirement| (requirement.identity(), *requirement))
        .collect::<BTreeMap<_, _>>();

    let mut plans = Vec::new();

    for exit in flow.exits() {
        let mut cancellation = Vec::new();
        let mut lifecycle = Vec::new();
        let mut dispositions = Vec::new();
        let mut is_recovered = exit.is_recovered();

        for identity in exit.live().iter().rev().copied() {
            let Some(requirement) = requirements_by_identity.get(&identity).copied() else {
                is_recovered = true;

                dispositions.push(AsyncStorageExitDecision::new(
                    identity,
                    AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                    ),
                ));

                continue;
            };

            let disposition = storage_exit_disposition(storage, exit, requirement);

            is_recovered |= matches!(disposition, AsyncStorageExitDisposition::Recovered(_));
            dispositions.push(AsyncStorageExitDecision::new(identity, disposition));

            if let AsyncStorageExitDisposition::Cleanup { access, phases } = disposition {
                if phases.includes_cancellation() {
                    cancellation.push(access);
                }

                if phases.includes_lifecycle() {
                    lifecycle.push(access);
                }
            }
        }

        plans.push(AsyncScopeExitPlan::new(
            exit.scope(),
            exit.exit(),
            dispositions,
            cancellation,
            lifecycle,
            exit.moved().iter().copied(),
            is_recovered,
        ));
    }

    Ok((requirements, plans, cleanup_shapes.into_diagnostics()))
}

fn storage_requirements<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &StorageFlow,
    owners: &bray_bound_tree::StorageScopeOwners,
    cleanup_shapes: &mut CleanupShapeResolver<'_, C>,
) -> Result<Vec<AsyncStorageRequirement>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let live = flow
        .exits()
        .iter()
        .flat_map(|exit| exit.live().iter().copied())
        .collect::<BTreeSet<_>>();

    let mut requirements = Vec::with_capacity(live.len());

    for identity in live {
        let cleanup = match (
            storage.root_access(identity),
            storage.storage_type(identity),
        ) {
            (None, _) => AsyncStorageCleanupRequirement::Recovered(
                AsyncStorageExitRecoveryCause::UnavailableRootAccess,
            ),
            (_, None) => AsyncStorageCleanupRequirement::Recovered(
                AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
            ),
            (Some(_), Some(ty)) => cleanup_requirement(cleanup_shapes.resolve(ty)?),
        };

        requirements.push(AsyncStorageRequirement::new(
            identity,
            owners.scope(storage.identity(identity)),
            storage_identity_transfers_at_unit_exit(request.unit(), storage, identity),
            cleanup,
        ));
    }

    Ok(requirements)
}

const fn cleanup_requirement(shape: CleanupShape) -> AsyncStorageCleanupRequirement {
    if shape.recovered {
        return AsyncStorageCleanupRequirement::Recovered(
            AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
        );
    }

    match (shape.cancellation, shape.lifecycle) {
        (true, true) => {
            AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::CancellationThenLifecycle)
        }
        (true, false) => AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Cancellation),
        (false, true) => AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
        (false, false) => AsyncStorageCleanupRequirement::None,
    }
}

fn storage_exit_disposition(
    storage: &StoragePlan,
    exit: &StorageExitDecision,
    requirement: AsyncStorageRequirement,
) -> AsyncStorageExitDisposition {
    if requirement.owner() != Some(exit.scope()) {
        return AsyncStorageExitDisposition::Retained;
    }

    if requirement.transfers() {
        return AsyncStorageExitDisposition::Transferred;
    }

    if exit.fully_moved().contains(&requirement.identity()) {
        return AsyncStorageExitDisposition::Moved;
    }

    let is_partial = !exit.initialized().contains(&requirement.identity())
        || exit
            .moved()
            .iter()
            .any(|access| storage.root_identity(*access) == Some(requirement.identity()));

    if is_partial {
        return match requirement.cleanup() {
            AsyncStorageCleanupRequirement::None => AsyncStorageExitDisposition::NoCleanup,
            AsyncStorageCleanupRequirement::Cleanup(_)
            | AsyncStorageCleanupRequirement::Recovered(_) => {
                AsyncStorageExitDisposition::Recovered(
                    AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                )
            }
        };
    }

    match requirement.cleanup() {
        AsyncStorageCleanupRequirement::None => AsyncStorageExitDisposition::NoCleanup,
        AsyncStorageCleanupRequirement::Cleanup(phases) => {
            match storage.root_access(requirement.identity()) {
                Some(access) => AsyncStorageExitDisposition::Cleanup { access, phases },
                None => AsyncStorageExitDisposition::Recovered(
                    AsyncStorageExitRecoveryCause::UnavailableRootAccess,
                ),
            }
        }
        AsyncStorageCleanupRequirement::Recovered(cause) => {
            AsyncStorageExitDisposition::Recovered(cause)
        }
    }
}
