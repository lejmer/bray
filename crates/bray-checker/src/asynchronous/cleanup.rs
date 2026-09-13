use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncCleanupPhases, AsyncScopeExitPlan, AsyncStorageCleanupRequirement,
    AsyncStorageExitDecision, AsyncStorageExitDisposition, AsyncStorageExitRecoveryCause,
    AsyncStorageRequirement, StorageCleanupType, StorageFlow, StoragePlan,
    storage_identity_transfers_at_unit_exit,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{GenericArgument, TypeData, TypeExpressionTemplate, TypeId};

use super::parts::CleanupExpansion;
use crate::storage::storage_scope_owners;
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

pub(crate) fn cleanup_scopes<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<BTreeSet<bray_bound_tree::BoundBlockId>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let owners = storage_scope_owners(request).map_err(CheckerQueryError::with_upstream)?;
    let mut resolver = CleanupShapeResolver::new(request);
    let mut scopes = BTreeSet::new();

    for (id, identity) in storage.identity_entries() {
        let (Some(scope), Some(ty)) = (owners.scope(Some(identity)), storage.storage_type(id))
        else {
            continue;
        };

        let shape = resolver.resolve(ty)?;

        if shape.cancellation || shape.lifecycle {
            scopes.insert(scope);
        }
    }

    Ok(scopes)
}

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
    pub(super) request: CheckerUnitView<'request, C>,
    completed: BTreeMap<TypeId, CleanupShape>,
    pub(super) cleanup_types: BTreeMap<TypeId, StorageCleanupType>,
    active: BTreeSet<TypeId>,
    // Query diagnostics are cloned because the cleanup result owns them independently.
    pub(super) diagnostics: DiagnosticBag,
}

impl<'request, C> CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn new(request: CheckerUnitView<'request, C>) -> Self {
        Self {
            request,
            completed: BTreeMap::new(),
            cleanup_types: BTreeMap::new(),
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
            | TypeData::Generator(_) => CleanupShape::BOTH,
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
            | TypeData::Callable(_)
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

        for member in representation.value().storage().member_types() {
            shape.merge(self.member_shape(member, substitution)?);
        }

        Ok(shape)
    }

    fn member_shape(
        &mut self,
        template: &TypeExpressionTemplate,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<CleanupShape, CheckerQueryError<C::UpstreamError>> {
        let Some(ty) = self.member_type(template, substitution)? else {
            return Ok(CleanupShape {
                recovered: true,
                ..CleanupShape::BOTH
            });
        };

        self.resolve(ty)
    }

    pub(super) fn member_type(
        &mut self,
        template: &TypeExpressionTemplate,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Result<Option<TypeId>, CheckerQueryError<C::UpstreamError>> {
        let checked =
            crate::constant::checked_substituted_type(self.request, template, substitution)?;

        let (ty, diagnostics) = checked.into_parts();

        self.diagnostics.add_range(diagnostics);

        Ok(ty)
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

    pub(super) fn report_incomplete_lifecycle(
        &mut self,
        ty: TypeId,
        source: bray_bound_tree::BoundSourceAnchor,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        use bray_diagnostics::{
            Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
            SeverityKind,
        };

        let span = self.request.source(source)?.span();
        let ty = crate::diagnostic::diagnostic_type(self.request.context(), ty)?;

        self.diagnostics.add(
            Diagnostic::new(
                crate::diagnostic::diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::CheckingIncompleteLifecycleStorage,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::IncompleteLifecycleStorage,
                span,
            ))
            .with_arg(DiagnosticArg::actual_type(ty)),
        );

        Ok(())
    }
}

pub(super) fn scope_exit_plans<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &StorageFlow,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
) -> Result<
    (
        Vec<AsyncStorageRequirement>,
        Vec<StorageCleanupType>,
        Vec<AsyncScopeExitPlan>,
        Vec<bray_bound_tree::StorageReplacementPlan>,
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
    let replacements = super::replacement::replacement_plans(storage, flow, &mut cleanup_shapes)?;

    let requirements_by_identity = requirements
        .iter()
        .map(|requirement| (requirement.identity(), requirement))
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

            let disposition = requirement.exit_disposition(storage, exit);

            is_recovered |= matches!(disposition, AsyncStorageExitDisposition::Recovered(_));
            dispositions.push(AsyncStorageExitDecision::new(identity, disposition));

            if let AsyncStorageExitDisposition::Cleanup { access, phases, .. } = disposition {
                if phases.includes_cancellation() {
                    cancellation.push(access);
                }

                if phases.includes_lifecycle() {
                    lifecycle.push(access);
                }
            }
        }

        match dependencies.lifecycle_order(request.unit(), storage, &lifecycle) {
            Ok(ordered) => lifecycle = ordered,
            Err(access) => {
                is_recovered = true;

                for decision in &mut dispositions {
                    if Some(decision.identity()) == storage.root_identity(access) {
                        *decision = AsyncStorageExitDecision::new(
                            decision.identity(),
                            AsyncStorageExitDisposition::Recovered(
                                AsyncStorageExitRecoveryCause::UnavailableCleanupOrder,
                            ),
                        );
                    }
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

    Ok((
        requirements,
        cleanup_shapes.cleanup_types.into_values().collect(),
        plans,
        replacements,
        cleanup_shapes.diagnostics,
    ))
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
        let mut cleanup = match (
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

        let owner = owners.scope(storage.identity(identity));
        let transfers = storage_identity_transfers_at_unit_exit(storage, identity);

        let moved = flow
            .cleanup_moved_projections(storage, identity, owner)
            .collect::<Vec<_>>();

        let destructor = bray_bound_tree::storage_identity_is_destructor_receiver(
            request.unit(),
            storage,
            identity,
        );

        let parts = match (
            storage.storage_type(identity),
            storage
                .root_access(identity)
                .and_then(|access| storage.access(access)),
        ) {
            (Some(ty), Some(root)) if destructor || !moved.is_empty() => cleanup_shapes
                .represented_parts(
                    ty,
                    &moved,
                    if destructor {
                        CleanupExpansion::DestructorReceiver
                    } else {
                        CleanupExpansion::MovedPaths
                    },
                    root.source(),
                )?,
            _ => None,
        };

        if destructor {
            cleanup = match &parts {
                Some(parts) => cleanup_requirement(CleanupShape {
                    cancellation: parts
                        .iter()
                        .any(|part| part.phases().includes_cancellation()),
                    lifecycle: parts.iter().any(|part| part.phases().includes_lifecycle()),
                    recovered: false,
                }),
                None => AsyncStorageCleanupRequirement::Recovered(
                    AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                ),
            };
        }

        let mut requirement = AsyncStorageRequirement::new(identity, owner, transfers, cleanup);

        if let Some(parts) = parts {
            requirement = requirement.with_parts(parts);
        }

        requirements.push(requirement);
    }

    Ok(requirements)
}

pub(super) const fn cleanup_requirement(shape: CleanupShape) -> AsyncStorageCleanupRequirement {
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
