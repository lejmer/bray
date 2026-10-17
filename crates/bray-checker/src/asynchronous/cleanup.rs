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
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn cleanup_scopes<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<
    (
        BTreeSet<bray_bound_tree::BoundBlockId>,
        BTreeSet<bray_bound_tree::BoundExpressionId>,
    ),
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let owners = storage_scope_owners(request).map_err(CheckerQueryError::with_upstream)?;
    let resolver = cleanup_shapes;
    let mut scopes = BTreeSet::new();
    let mut cleanup_free_operations = BTreeSet::new();

    for (id, identity) in storage.identity_entries() {
        let (Some(scope), Some(ty)) = (owners.scope(Some(identity)), storage.storage_type(id))
        else {
            continue;
        };

        let shape = resolver.resolve(ty)?;

        if shape.cancellation || shape.lifecycle {
            scopes.insert(scope);
        } else if !shape.recovered
            && let bray_bound_tree::StorageIdentity::Temporary(expression) = identity
            && !matches!(
                request.unit().tree().expression(expression),
                Some(bray_bound_tree::BoundExpression::Assignment(_))
            )
        {
            cleanup_free_operations.insert(expression);
        }
    }

    let mut replacements = BTreeMap::new();

    for plan in storage
        .access_plans()
        .iter()
        .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment)
    {
        let ty = storage
            .access(plan.access())
            .expect("assignment retains its checked destination")
            .reached_type();

        let shape = resolver.resolve(ty)?;

        let free = !shape.cancellation && !shape.lifecycle && !shape.recovered;

        replacements
            .entry(plan.expression())
            .and_modify(|previous| *previous &= free)
            .or_insert(free);
    }

    cleanup_free_operations.extend(
        replacements
            .into_iter()
            .filter_map(|(expression, free)| free.then_some(expression)),
    );

    Ok((scopes, cleanup_free_operations))
}

pub(crate) fn cleanup_free_storage<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<
    (BTreeSet<bray_bound_tree::StorageIdentityId>, DiagnosticBag),
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let resolver = cleanup_shapes;
    let mut identities = BTreeSet::new();

    for (identity, provenance) in storage.identity_entries() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        if !matches!(
            provenance,
            bray_bound_tree::StorageIdentity::Temporary(_)
                | bray_bound_tree::StorageIdentity::LocalOwned(_)
                | bray_bound_tree::StorageIdentity::Parameter(_)
                | bray_bound_tree::StorageIdentity::Receiver(_)
                | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
                | bray_bound_tree::StorageIdentity::PredicateParameter(_)
        ) {
            continue;
        }

        let Some(ty) = storage.storage_type(identity) else {
            continue;
        };

        let shape = resolver.resolve(ty)?;

        if !shape.cancellation && !shape.lifecycle && !shape.recovered {
            identities.insert(identity);
        }
    }

    Ok((identities, std::mem::take(&mut resolver.diagnostics)))
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct CleanupShape {
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

pub(crate) struct CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'request, C>,
    pub(crate) completed: std::sync::Arc<BTreeMap<TypeId, CleanupShape>>,
    pub(super) cleanup_types: BTreeMap<TypeId, StorageCleanupType>,
    pub(super) recursive_cleanup: bool,
    pub(super) expanded_types: BTreeSet<TypeId>,
    pub(super) full_parts:
        BTreeMap<TypeId, std::sync::Arc<[(TypeId, Option<bray_bound_tree::StorageCleanupPart>)]>>,
    requirements: Option<std::sync::Arc<[AsyncStorageRequirement]>>,
    active: BTreeSet<TypeId>,
    // Query diagnostics are cloned because the cleanup result owns them independently.
    pub(crate) diagnostics: DiagnosticBag,
}

impl<'request, C> CleanupShapeResolver<'request, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(crate) fn new(request: CheckerUnitView<'request, C>) -> Self {
        Self {
            request,
            completed: std::sync::Arc::new(BTreeMap::new()),
            cleanup_types: BTreeMap::new(),
            recursive_cleanup: false,
            expanded_types: BTreeSet::new(),
            full_parts: BTreeMap::new(),
            requirements: None,
            active: BTreeSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(crate) fn requirement(
        &mut self,
        ty: TypeId,
    ) -> Result<AsyncStorageCleanupRequirement, CheckerQueryError<C::UpstreamError>> {
        self.resolve(ty).map(cleanup_requirement)
    }

    pub(crate) fn storage_requirements(
        &mut self,
        storage: &StoragePlan,
    ) -> Result<std::sync::Arc<[AsyncStorageRequirement]>, CheckerQueryError<C::UpstreamError>>
    {
        if let Some(requirements) = &self.requirements {
            return Ok(std::sync::Arc::clone(requirements));
        }

        let owners =
            storage_scope_owners(self.request).map_err(CheckerQueryError::with_upstream)?;

        let requirements: std::sync::Arc<[_]> =
            storage_requirements(self.request, storage, &owners, self)?.into();

        self.requirements = Some(std::sync::Arc::clone(&requirements));

        Ok(requirements)
    }

    fn include_input(&mut self, ty: TypeId) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let cleanup = cleanup_requirement(self.resolve(ty)?);

        self.cleanup_types
            .entry(ty)
            .or_insert_with(|| StorageCleanupType::new(ty, cleanup));

        Ok(())
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

        let data = self.request.semantic_values().type_data(ty);

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

        // Prepared consumers share all known shapes. A newly demanded type extends
        // only this resolver's cache, without mutating the published preparation.
        std::sync::Arc::make_mut(&mut self.completed).insert(ty, shape);

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
                    .generic_substitution_data(substitution);

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
        if let Some(element) = self
            .request
            .context()
            .raw_buffer_element(definition, substitution)?
        {
            let mut shape = self.resolve(element)?;

            shape.lifecycle = true;

            return Ok(shape);
        }

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

        let span = self.request.source(source).span();
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
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &StorageFlow,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    types: &bray_bound_tree::CheckedExpressionTypes,
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
    let requirements = cleanup_shapes.storage_requirements(storage)?;
    let replacements = super::replacement::replacement_plans(storage, flow, cleanup_shapes)?;

    for entry in selections.entries() {
        match entry.selection() {
            bray_bound_tree::SemanticSelection::Operation(
                bray_bound_tree::SelectedOperation::Construction(construction),
            ) => {
                cleanup_shapes.include_input(construction.result_type())?;

                for input in construction.inputs() {
                    let (bray_bound_tree::SelectedConstructionInput::Explicit { ty, .. }
                    | bray_bound_tree::SelectedConstructionInput::Default { ty, .. }) = input;

                    cleanup_shapes.include_input(*ty)?;
                }
            }
            bray_bound_tree::SemanticSelection::Call(call) => {
                for argument in call.arguments() {
                    let ty = match argument {
                        bray_bound_tree::SelectedArgument::Explicit { conversion, .. } => {
                            conversion.target_type()
                        }
                        bray_bound_tree::SelectedArgument::Default { ty, .. } => *ty,
                    };

                    cleanup_shapes.include_input(ty)?;
                }

                if let Some(receiver) = call.receiver() {
                    let ty = receiver
                        .input_type(request.semantic_values()).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in scope_exit_plans: {error:?}"));

                    cleanup_shapes.include_input(ty)?;
                }
            }
            _ => {}
        }
    }

    for (id, expression) in request.unit().tree().expressions() {
        if matches!(expression, bray_bound_tree::BoundExpression::Await(_)) {
            // A completed await owns its result before delivering cancellation.
            // Publish its cleanup shape with the other implicit input owners.
            let ty = types
                .expression(id)
                .expect("checked await retains its completion type")
                .ty();

            cleanup_shapes.include_input(ty)?;
        }

        let bray_bound_tree::BoundExpression::Structured(expression) = expression else {
            continue;
        };

        use bray_bound_tree::BoundStructuredExpressionKind as Kind;

        if matches!(
            expression.kind(),
            Kind::Tuple | Kind::Array | Kind::RepeatedArray | Kind::Range
        ) {
            for operand in expression.operands() {
                let ty = types
                    .expression(*operand).unwrap_or_else(|| panic!("scope_exit_plans requires checked expression type or node, operand: {operand:?}"))
                    .ty();

                cleanup_shapes.include_input(ty)?;
            }
        }
    }

    let requirements_by_identity = requirements
        .iter()
        .map(|requirement| (requirement.identity(), requirement))
        .collect::<BTreeMap<_, _>>();

    let mut plans = Vec::new();

    let scoped_uses = selections
        .entries()
        .iter()
        .filter_map(|entry| {
            let bray_bound_tree::SemanticSelection::ScopedUse(scoped) = entry.selection() else {
                return None;
            };

            let Some(bray_bound_tree::BoundExpression::Structured(expression)) =
                request.view().expression(scoped.expression())
            else {
                panic!("selected scoped use retains its structured expression");
            };

            let [body] = expression.blocks() else {
                panic!("selected scoped use retains its sole body");
            };

            Some((*body, scoped.expression()))
        })
        .collect::<BTreeMap<_, _>>();

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

            let disposition = requirement.exit_disposition(
                storage,
                exit.scope(),
                exit.initialized().contains(&identity),
                exit.fully_moved().contains(&identity),
                exit.moved().iter().cloned(),
                exit.definitely_absent().iter().cloned(),
            );

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

        match dependencies.lifecycle_order(storage, &lifecycle) {
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

        let mut plan = AsyncScopeExitPlan::new(
            exit.scope(),
            exit.exit(),
            dispositions,
            cancellation,
            lifecycle,
            exit.moved().iter().cloned(),
            is_recovered,
        );

        if let Some(expression) = scoped_uses.get(&exit.scope()) {
            plan = plan.with_scoped_exit(*expression);
        }

        plans.push(plan);
    }

    if matches!(
        request.unit().root(),
        bray_bound_tree::BoundUnitRoot::CallableBody {
            execution: bray_symbols::CallableExecution::Asynchronous,
            ..
        } | bray_bound_tree::BoundUnitRoot::AnonymousCallable {
            execution: bray_symbols::CallableExecution::Asynchronous,
            ..
        }
    ) && let Some(result) = types.callable_result_type()
    {
        cleanup_shapes.include_input(result)?;
    }

    Ok((
        requirements.to_vec(),
        // Publication owns a snapshot while preparation remains available to later consumers.
        cleanup_shapes.cleanup_types.values().cloned().collect(),
        plans,
        replacements,
        std::mem::take(&mut cleanup_shapes.diagnostics),
    ))
}

fn storage_requirements<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    owners: &bray_bound_tree::StorageScopeOwners,
    cleanup_shapes: &mut CleanupShapeResolver<'_, C>,
) -> Result<Vec<AsyncStorageRequirement>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut moved_by_identity = BTreeMap::<_, BTreeSet<_>>::new();

    for plan in storage.access_plans() {
        if storage.effective_purpose(*plan) == bray_bound_tree::StorageAccessPurpose::Move
            && !storage.is_root_access(plan.access())
            && let Some(identity) = storage.root_identity(plan.access())
        {
            moved_by_identity
                .entry(identity)
                .or_default()
                .insert(plan.access());
        }
    }

    let mut requirements = Vec::with_capacity(storage.identities().len());

    for (identity, _) in storage.identity_entries() {
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

        let moved = moved_by_identity
            .get(&identity)
            .into_iter()
            .flatten()
            .filter_map(|access| storage.resolved_projections(*access))
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
