use crate::storage::local_initialization_destinations;
use crate::{CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView};
use bray_bound_tree::{
    BorrowCapabilityId, BoundDependencySubject, BoundExpressionId, Liveness, StorageAccessId,
    StorageAccessPlan, StorageAccessPurpose, StorageIdentity, StorageIdentityId, StoragePlan,
};
use bray_symbols::{BorrowKind, CallableSignatureQuery};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(in crate::analysis::storage_flow) struct StorageFlowInput {
    pub(in crate::analysis::storage_flow) variant_payloads:
        BTreeMap<bray_bound_tree::StoragePath, BTreeSet<bray_bound_tree::StoragePath>>,
    expression_places:
        BTreeMap<BoundExpressionId, Vec<crate::analysis::storage_flow::memory::Place>>,
    definitions: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    initialization_destinations: BTreeMap<BoundExpressionId, StorageIdentityId>,
    mutable_storage: BTreeSet<StorageIdentityId>,
    immutable_field_accesses: BTreeSet<StorageAccessId>,
    storage_last_uses: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    pub(in crate::analysis::storage_flow) atomic_observations:
        Option<bray_bound_tree::StorageInvalidation>,
    pub(in crate::analysis::storage_flow) discriminants:
        BTreeMap<StorageIdentityId, BTreeSet<bray_bound_tree::StoragePath>>,
    pub(in crate::analysis::storage_flow) tag_preserving_assignments:
        BTreeSet<bray_bound_tree::BoundExecutionSite>,
    pub(in crate::analysis::storage_flow) borrowed_identities:
        BTreeMap<StorageIdentityId, Vec<bray_bound_tree::StoragePath>>,
    pub(in crate::analysis::storage_flow) opaque_values: BTreeSet<bray_bound_tree::StoragePath>,
    pub(in crate::analysis::storage_flow) stable_identities:
        BTreeMap<StorageIdentityId, Vec<bray_bound_tree::StoragePath>>,
    pub(in crate::analysis::storage_flow) opaque_candidates: Vec<bray_bound_tree::StoragePath>,
    pub(super) unused_entry_storage: BTreeSet<StorageIdentityId>,
    pub(super) cleanup_free_storage: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis::storage_flow) terminal_dependencies: bool,
    pub(in crate::analysis::storage_flow) terminal_receiver: Option<bray_bound_tree::StoragePath>,
    pub(in crate::analysis::storage_flow) graceful_types: BTreeSet<bray_symbols::TypeId>,
    pub(in crate::analysis::storage_flow) value_dependencies:
        BTreeMap<StorageIdentityId, BTreeSet<BoundDependencySubject>>,
    pub(super) cleanup_dependency_owners:
        BTreeMap<BoundDependencySubject, BTreeSet<StorageIdentityId>>,
    pub(in crate::analysis::storage_flow) lifecycle_paths: BTreeMap<
        (
            bray_symbols::CallableInstanceData,
            bray_symbols::DependencySubject,
        ),
        bray_bound_tree::StorageReferencePath,
    >,
    pub(in crate::analysis::storage_flow) lifecycle_references:
        BTreeMap<bray_symbols::CallableInstanceData, Vec<bray_bound_tree::BoundCallReference>>,
    pub(in crate::analysis::storage_flow) complete_reference_types: BTreeSet<bray_symbols::TypeId>,
    pub(in crate::analysis::storage_flow) raw_buffers:
        BTreeMap<bray_symbols::TypeId, (crate::representation::RawBufferFields, bool)>,
    pub(in crate::analysis::storage_flow) reference_paths:
        BTreeMap<StorageAccessId, bray_bound_tree::StorageReferencePath>,
    pub(in crate::analysis::storage_flow) value_references: BTreeMap<
        StorageIdentityId,
        BTreeMap<crate::analysis::storage_flow::memory::Place, bray_symbols::TypeId>,
    >,
    pub(in crate::analysis::storage_flow) entry_references: BTreeMap<
        (crate::analysis::storage_flow::memory::Place, usize),
        (
            bray_bound_tree::StorageReferencePath,
            Option<bray_symbols::TypeId>,
        ),
    >,
    pub(in crate::analysis::storage_flow) reference_constructions: BTreeMap<
        BoundExpressionId,
        Vec<(
            BoundExpressionId,
            crate::analysis::storage_flow::memory::Place,
            bray_symbols::TypeId,
        )>,
    >,
    pub(in crate::analysis::storage_flow) retained_storage: BTreeSet<StorageAccessId>,
    pub(in crate::analysis::storage_flow) suspension_borrows: BTreeSet<BorrowCapabilityId>,
}

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn new<C>(
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        liveness: &Liveness,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        values: &crate::dependency::ValueInputs,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
    ) -> crate::CheckerQueryResult<(Self, bray_diagnostics::DiagnosticBag), C::UpstreamError>
    where
        C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
    {
        let (mutable_storage, mut diagnostics) =
            crate::analysis::storage_flow::authority::mutable_storage(request, storage)?;

        let initialization_destinations = local_initialization_destinations(request, storage);

        let mut retained_storage = BTreeSet::new();

        for call in dependencies.calls().values() {
            collect_retained_storage(call.result().requirements(), &mut retained_storage);

            for (_, report) in call.report_outputs() {
                collect_retained_storage(report.requirements(), &mut retained_storage);
            }
        }

        let atomic_operations = memory.operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::AtomicStore { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::AtomicExchange { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::AtomicCompareExchange { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::AtomicFetch { .. }
            )
        });

        let atomic = dependencies
            .calls()
            .values()
            .flat_map(|call| {
                call.effects()
                    .iter()
                    .chain(call.deferred_effects().into_iter().flatten())
            })
            .chain(
                dependencies
                    .lifecycle_dependencies()
                    .values()
                    .flat_map(|contract| contract.effects()),
            )
            .any(|effect| {
                matches!(
                    effect,
                    bray_symbols::DependencyStorageEffect::Atomic
                        | bray_symbols::DependencyStorageEffect::Direct {
                            kind: bray_symbols::DependencyMutationKind::Atomic,
                            ..
                        }
                        | bray_symbols::DependencyStorageEffect::Indirect {
                            kind: bray_symbols::DependencyMutationKind::Atomic,
                            ..
                        }
                )
            })
            || atomic_operations;

        // Returned reference origins are needed even when a body has no memory effects.
        // Share the same type-shape preparation with mutation observations.
        let footprint = crate::storage::observation::prepare(request, storage)?;

        let mut tag_preserving_assignments = BTreeMap::<_, bool>::new();

        for plan in storage
            .access_plans()
            .iter()
            .filter(|plan| plan.purpose() == StorageAccessPurpose::Assignment)
        {
            let builtin = matches!(request.view().expression(plan.expression()), Some(bray_bound_tree::BoundExpression::Assignment(assignment)) if assignment.operator() == bray_bound_tree::BoundAssignmentOperator::Assign)
                || matches!(selections.expression(plan.expression()), Some(bray_bound_tree::SemanticSelection::Operation(bray_bound_tree::SelectedOperation::CompoundAssignment(assignment))) if matches!(assignment.target(), bray_bound_tree::OperatorTarget::BuiltIn(_)));

            let ty = storage
                .access(plan.access())
                .expect("assignment retains its destination type")
                .reached_type();

            let preserves = builtin
                && footprint.tag_free_values.contains(&ty)
                && cleanup_shapes.requirement(ty)?
                    == bray_bound_tree::AsyncStorageCleanupRequirement::None;

            tag_preserving_assignments
                .entry(plan.occurrence())
                .and_modify(|known| *known &= preserves)
                .or_insert(preserves);
        }

        let mut demands = BTreeMap::<_, BTreeSet<_>>::new();

        for (path, ty) in &footprint.references {
            let owner = storage
                .storage_type(path.root)
                .expect("prepared observation retains its storage type");

            demands
                .entry(request.semantic_values().unborrowed_type(owner))
                .or_default()
                .insert((std::sync::Arc::clone(&path.projections), *ty));
        }

        let atomic_observations = atomic.then_some(footprint.atomic);
        let stable_identities = footprint.identities;
        let borrowed_identities = footprint.borrowed_identities;
        let discriminants = footprint.discriminants;
        let scalar_values = footprint.scalars;

        let owned_descriptors = storage
            .identity_entries()
            .filter_map(|(root, identity)| {
                identity.is_invocation_owned(storage.kind()).then_some(root)
            })
            .map(bray_bound_tree::StoragePath::root);

        let opaque_values = stable_identities
            .values()
            .chain(borrowed_identities.values())
            .chain(scalar_values.values())
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();

        let opaque_candidates = owned_descriptors
            .chain(
                stable_identities
                    .keys()
                    .copied()
                    .map(bray_bound_tree::StoragePath::root),
            )
            .chain(
                borrowed_identities
                    .keys()
                    .copied()
                    .map(bray_bound_tree::StoragePath::root),
            )
            .chain(
                discriminants
                    .keys()
                    .copied()
                    .map(bray_bound_tree::StoragePath::root),
            )
            .chain(opaque_values.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        let mut input = Self {
            complete_reference_types: footprint.complete_reference_types,
            atomic_observations,
            opaque_values,
            opaque_candidates,
            stable_identities,
            borrowed_identities,
            discriminants,
            tag_preserving_assignments: tag_preserving_assignments
                .into_iter()
                .filter_map(|(site, preserves)| preserves.then_some(site))
                .collect(),
            retained_storage,
            initialization_destinations,
            mutable_storage,
            suspension_borrows: liveness
                .live_across_suspensions()
                .iter()
                .filter_map(|entry| match entry.subject() {
                    bray_bound_tree::BoundDependencySubject::BorrowCapability(borrow) => {
                        Some(borrow)
                    }
                    _ => None,
                })
                .collect(),
            ..Self::default()
        };

        input.prepare_locations(storage, cleanup);
        input.prepare_report_dependencies(storage, liveness, dependencies, cleanup);
        input.prepare_variant_payloads(footprint.references.iter().map(|(path, _)| path));

        input.prepare_reference_constructions(request, storage, values)?;
        input.prepare_reference_paths(request, storage)?;

        let lifecycle_demands =
            input.prepare_lifecycle_references(request, cleanup, dependencies)?;

        for (ty, paths) in lifecycle_demands {
            demands.entry(ty).or_default().extend(paths);
        }

        let buffer_types = storage
            .identity_entries()
            .filter_map(|(identity, _)| storage.storage_type(identity))
            .chain(
                storage
                    .access_entries()
                    .map(|(_, access)| access.reached_type()),
            )
            .chain(cleanup.opaque_types())
            .chain(
                memory
                    .operations()
                    .iter()
                    .flat_map(|operation| {
                        operation
                            .arguments()
                            .iter()
                            .copied()
                            .chain([operation.expression()])
                    })
                    .filter_map(|expression| types.expression(expression).map(|ty| ty.ty())),
            );

        input.prepare_buffer_fields(request, cleanup_shapes, buffer_types, &mut demands)?;

        input.prepare_entry_references(request, storage, selections, dependencies, demands)?;

        input.prepare_terminal_ownership(cleanup_shapes, request, storage, dependencies, memory)?;

        input.immutable_field_accesses =
            crate::analysis::storage_flow::authority::immutable_field_accesses(
                request, storage, &input,
            )?;

        let (cleanup_free, cleanup_diagnostics) =
            crate::asynchronous::cleanup_free_storage(cleanup_shapes, request, storage)?;

        input.prepare_cleanup_retention(request, storage, liveness, cleanup);
        input.prepare_storage_retirement(storage, liveness, &cleanup_free);
        input.cleanup_free_storage = cleanup_free;
        diagnostics.add_range(cleanup_diagnostics);

        Ok((input, diagnostics))
    }

    pub(super) fn prepare_locations(
        &mut self,
        storage: &StoragePlan,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) {
        for plan in storage.access_plans().iter().copied().filter(|plan| {
            matches!(
                plan.occurrence(),
                bray_bound_tree::BoundExecutionSite::Node(_)
            ) && !matches!(
                storage
                    .root_identity(plan.access())
                    .and_then(|identity| storage.identity(identity)),
                Some(StorageIdentity::InvocationInput { .. })
            )
        }) {
            if let Some(place) = storage.logical_path(plan.access()).cloned() {
                self.expression_places
                    .entry(plan.expression())
                    .or_default()
                    .push(place);
            }
        }

        // A simple local receives its value when the initializer completes. Its
        // storage incarnation must begin there, before the value is installed.
        let initialized_at = self
            .initialization_destinations
            .iter()
            .map(|(expression, identity)| {
                (
                    *identity,
                    bray_bound_tree::BoundExecutionSite::from(*expression),
                )
            })
            .collect::<BTreeMap<_, _>>();

        for (storage, identity) in storage.identity_entries() {
            let Some(node) = initialized_at
                .get(&storage)
                .copied()
                .or_else(|| identity_definition_occurrence(identity))
            else {
                continue;
            };

            self.definitions.entry(node).or_default().push(storage);
        }

        self.prepare_variant_payloads(
            storage
                .access_entries()
                .filter_map(|(access, _)| storage.logical_path(access))
                .chain(cleanup.represented_paths()),
        );
    }

    fn prepare_variant_payloads<'a>(
        &mut self,
        paths: impl IntoIterator<Item = &'a bray_bound_tree::StoragePath>,
    ) {
        for path in paths {
            for (depth, projection) in path.projections.iter().enumerate() {
                if matches!(
                    projection,
                    bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }
                ) {
                    let parent = bray_bound_tree::StoragePath {
                        root: path.root,
                        projections: path.projections[..depth].into(),
                    };

                    let payload = bray_bound_tree::StoragePath {
                        root: path.root,
                        projections: path.projections[..=depth].into(),
                    };

                    self.variant_payloads
                        .entry(parent)
                        .or_default()
                        .insert(payload);
                }
            }
        }
    }

    fn prepare_cleanup_retention<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        liveness: &Liveness,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) {
        // Cleanup still uses retained loans after the last ordinary read of its owner.
        // Index established value retention, never physical referents or invocation inputs.
        let owners = cleanup
            .requirements
            .iter()
            .filter(|requirement| {
                !requirement.transfers()
                    && matches!(
                        requirement.cleanup(),
                        bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                    )
            })
            .map(|requirement| requirement.identity())
            .collect::<BTreeSet<_>>();

        for owner in &owners {
            self.cleanup_dependency_owners
                .entry(BoundDependencySubject::Storage(*owner))
                .or_default()
                .insert(*owner);
        }

        let bindings = crate::storage::value_transfer_bindings(request, storage);

        let occurrences = bindings
            .iter()
            .flat_map(|(initializer, bindings)| {
                bindings.iter().filter_map(|binding| {
                    let identity = match binding {
                        bray_bound_tree::StorageBinding::Identity(identity) => *identity,
                        bray_bound_tree::StorageBinding::Access(access) => {
                            storage.root_identity(*access)?
                        }
                    };

                    Some((
                        identity,
                        bray_bound_tree::BoundExecutionSite::from(*initializer),
                    ))
                })
            })
            .chain(
                storage
                    .identity_entries()
                    .filter_map(|(identity, definition)| {
                        Some((identity, definition.definition_occurrence()?))
                    }),
            );

        for (owner, occurrence) in occurrences {
            if !owners.contains(&owner) {
                continue;
            }

            for subject in liveness.retained_by(occurrence) {
                self.cleanup_dependency_owners
                    .entry(subject)
                    .or_default()
                    .insert(owner);
            }
        }
    }

    fn prepare_storage_retirement(
        &mut self,
        storage: &StoragePlan,
        liveness: &Liveness,
        cleanup_free: &BTreeSet<StorageIdentityId>,
    ) {
        if liveness.is_recovered() {
            return;
        }

        for last_use in liveness.last_uses() {
            if let bray_bound_tree::BoundDependencySubject::Storage(identity) = last_use.subject()
                && cleanup_free.contains(&identity)
            {
                self.storage_last_uses
                    .entry(last_use.occurrence())
                    .or_default()
                    .push(identity);
            }
        }

        let entry_borrowed_storage = storage
            .borrow_capability_entries()
            .filter(|(_, capability)| capability.entry_binding().is_some())
            .filter_map(|(_, capability)| storage.root_identity(capability.access()))
            .collect::<BTreeSet<_>>();

        self.unused_entry_storage
            .extend(cleanup_free.iter().copied().filter(|identity| {
                storage
                    .identity(*identity)
                    .is_some_and(|identity| identity.is_initialized_at_entry())
                    && liveness
                        .storage_live_at_entry()
                        .binary_search(identity)
                        .is_err()
                    && !entry_borrowed_storage.contains(identity)
            }));
    }

    pub(in crate::analysis::storage_flow) fn expression_places(
        &self,
        expression: BoundExpressionId,
    ) -> &[crate::analysis::storage_flow::memory::Place] {
        self.expression_places
            .get(&expression)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn storage_last_uses(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> &[StorageIdentityId] {
        self.storage_last_uses
            .get(&occurrence)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn borrowed_access(
        &self,
        plan: StorageAccessPlan,
        storage: &StoragePlan,
    ) -> Option<StorageAccessId> {
        storage
            .borrow_for_plan(plan)
            .and_then(|borrow| storage.borrow_capability(borrow))
            .map(|borrow| {
                if borrow.occurrence() == Some(plan.occurrence()) {
                    borrow.access()
                } else {
                    // Reusing an input capability still accesses the reached field or element.
                    plan.access()
                }
            })
    }

    pub(in crate::analysis::storage_flow) fn definitions(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> &[StorageIdentityId] {
        self.definitions
            .get(&occurrence)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn initialization_destination(
        &self,
        expression: BoundExpressionId,
    ) -> Option<StorageIdentityId> {
        self.initialization_destinations.get(&expression).copied()
    }

    pub(in crate::analysis::storage_flow) fn mutation_authority_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        storage: &StoragePlan,
    ) -> Option<StorageAccessId> {
        match purpose {
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment => Some(plan.access()),
            StorageAccessPurpose::Borrow(BorrowKind::Mutable) => {
                self.borrowed_access(plan, storage)
            }
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Borrow(BorrowKind::Shared)
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => None,
        }
    }

    pub(in crate::analysis::storage_flow) fn fields_allow_mutation(
        &self,
        access: StorageAccessId,
    ) -> bool {
        !self.immutable_field_accesses.contains(&access)
    }

    pub(in crate::analysis::storage_flow) fn storage_is_mutable(
        &self,
        storage: StorageIdentityId,
    ) -> bool {
        self.mutable_storage.contains(&storage)
    }
}

fn identity_definition_occurrence(
    identity: StorageIdentity,
) -> Option<bray_bound_tree::BoundExecutionSite> {
    match identity {
        StorageIdentity::Result(_) => None,
        _ => identity.definition_occurrence(),
    }
}

fn collect_retained_storage(
    requirements: &[bray_bound_tree::BoundDependencyRequirement],
    output: &mut BTreeSet<StorageAccessId>,
) {
    for requirement in requirements {
        match requirement {
            bray_bound_tree::BoundDependencyRequirement::Direct {
                subject: BoundDependencySubject::StorageAccess(access),
                kind,
            } if *kind != bray_bound_tree::BoundDependencyRequirementKind::ValueDependencies => {
                output.insert(*access);
            }
            bray_bound_tree::BoundDependencyRequirement::Guarded(guard) => {
                collect_retained_storage(guard.requirements(), output)
            }
            _ => {}
        }
    }
}
