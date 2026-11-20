use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::CheckerRequestContext;

use crate::analysis::storage_flow::memory::Place;

pub(super) type Demands = BTreeMap<
    bray_symbols::TypeId,
    BTreeSet<(
        Arc<[bray_bound_tree::StorageProjection]>,
        bray_symbols::TypeId,
    )>,
>;

type ParentPaths = BTreeMap<
    bray_symbols::TypeId,
    BTreeSet<(
        bray_symbols::TypeId,
        Arc<[bray_bound_tree::StorageProjection]>,
    )>,
>;

impl crate::analysis::storage_flow::model::StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_entry_references<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        mut demands: Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (access, reached) in storage.access_entries() {
            if storage.reference_route(access).is_none() { continue; }

            let path = storage.logical_path(access).expect("a typed route has a logical path");

            let Some(root) = storage.identity_type(path.root) else { continue; };

            let root = request.semantic_values().unborrowed_type(root);

            demands.entry(root).or_default().insert((Arc::clone(&path.projections), reached.reached_type()));
        }

        self.prepare_summary_demands(request, storage, selections, dependencies, &mut demands)?;

        let types = storage.identity_entries().filter_map(|(identity, _)| storage.storage_type(identity))
            .map(|ty| request.semantic_values().unborrowed_type(ty)).collect::<BTreeSet<_>>();

        for owner in types {
            if matches!(request.semantic_values().type_data(owner).as_ref(), bray_symbols::TypeData::Generator(_) | bray_symbols::TypeData::OwnedIndirection { .. }) {
                let target = crate::storage::projected_value_type(request, owner, bray_symbols::DependencyProjection::OwnedTarget)?;
                let ty = target.value().expect("an owning formal has a checked backing type");

                demands.entry(owner).or_default().insert((Arc::from([bray_bound_tree::StorageProjection::OwnedTarget]), ty));
            }
        }

        self.close_entry_demands(request, &mut demands)?;

        self.install_reference_paths(request, storage, &demands)?;

        self.prepare_entry_pointee_fields(request, storage, &demands)
    }

    fn prepare_entry_pointee_fields<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        demands: &Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        // Prepare demanded fields of the original incoming addresses, not an unbounded
        // pointer-type closure. Writes retire these records through the ordinary value store.
        let incoming = self.entry_references.iter().filter_map(|((slot, _), (path, pointee, kind))| {
            (*kind == bray_symbols::DependencyReferenceKind::Address
                && self.entry_buffer_pointer(path, *pointee).is_none())
                .then(|| pointee.map(|pointee| (slot.clone(), pointee))).flatten()
        }).collect::<Vec<_>>();

        for (slot, pointee) in incoming {
            let owner = request.semantic_values().unborrowed_type(pointee);

            for (projections, reached) in demands.get(&owner).into_iter().flatten() {
                let projections = slot.projections.iter().copied()
                    .chain([bray_bound_tree::StorageProjection::AddressTarget])
                    .chain(projections.iter().copied()).collect::<Arc<[_]>>();

                let root = storage.storage_type(slot.root).expect("incoming reference has its checked storage type");

                let Some((reached, prepared)) = crate::storage::reference_path(request, root,
                    Arc::clone(&projections), Some(*reached))? else { continue; };

                self.prepare_value_routes(request, storage, Place { root: slot.root, projections }, reached, &prepared);
            }
        }

        Ok(())
    }

    fn close_entry_demands<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut parents = ParentPaths::new();

        for (root, paths) in demands.iter() {
            for (path, _) in paths {
                let mut ty = *root;

                for (index, projection) in path.iter().enumerate() {
                    // Only inline containment transports demands, including protected
                    // payloads. References and ownership lead to finite explicit demands.
                    if matches!(projection, bray_bound_tree::StorageProjection::OwnedTarget | bray_bound_tree::StorageProjection::AddressTarget)
                        || crate::representation::reference_kind(request, ty).is_some() {
                        break;
                    }

                    let projected = crate::storage::projected_storage_type(request, ty, *projection)?;

                    let Some(projected) = *projected.value() else {
                        break;
                    };

                    ty = projected;

                    if crate::representation::reference_kind(request, ty).is_some() {
                        break;
                    }

                    parents.entry(ty).or_default().insert((*root, path[..=index].into()));
                }
            }
        }

        // A contract can cross a buffer nested in an input without any source
        // expression naming that buffer. Its demanded pointer still owns backing.
        self.prepare_buffer_fields(request, parents.keys().copied(), demands)?;

        let components = bray_base::strongly_connected_components(parents.keys().copied(), |ty| {
            parents
                .get(&ty)
                .into_iter()
                .flatten()
                .map(|(parent, _)| *parent)
                .collect::<Vec<_>>()
        })
        .into_iter()
        .enumerate()
        .flat_map(|(component, types)| types.into_iter().map(move |ty| (ty, component)))
        .collect::<BTreeMap<_, _>>();

        // Recovered recursive inline types must not expand paths indefinitely. Their
        // representation diagnostic belongs to type analysis, not demand preparation.
        for (ty, containing) in &mut parents {
            containing.retain(|(parent, _)| components[ty] != components[parent]);
        }

        // Well-formed inline type containment is acyclic. Each demanded path is prepared
        // once, independent of how many whole-value moves lead to its eventual use.
        let mut pending = demands
            .iter()
            .flat_map(|(root, paths)| {
                paths
                    .iter()
                    .map(move |(path, reached)| (*root, Arc::clone(path), *reached))
            })
            .collect::<Vec<_>>();

        while let Some((ty, path, reached)) = pending.pop() {
            if request.is_cancelled() {
                return Err(crate::CheckerQueryError::Cancelled);
            }

            for (parent, prefix) in parents.get(&ty).into_iter().flatten() {
                let path = prefix
                    .iter()
                    .chain(path.iter())
                    .copied()
                    .collect::<std::sync::Arc<[_]>>();

                if demands
                    .entry(*parent)
                    .or_default()
                    .insert((Arc::clone(&path), reached))
                {
                    pending.push((*parent, path, reached));
                }
            }
        }

        Ok(())
    }

    pub(in crate::analysis::storage_flow) fn initialize_entry_references(
        &self,
        values: &bray_symbols::SemanticValueStore,
        memory: &mut crate::analysis::storage_flow::memory::State,
        initialized: &BTreeSet<bray_bound_tree::StorageIdentityId>,
    ) {
        let mut references = self.entry_references.iter().collect::<Vec<_>>();
        let mut backings = self.owned_backings.iter().filter(|(owner, _)| initialized.contains(&owner.root)).collect::<Vec<_>>();

        references.sort_by_key(|((_, layer), (path, _, _))| (
            path.steps().len(),
            path.projections().len(), *layer,
        ));

        backings.sort_by_key(|(_, (path, _))| (path.steps().len(), path.projections().len()));

        let mut backings = backings.into_iter().peekable();

        for ((slot, layer), (path, ty, kind)) in references {
            while let Some((owner, (route, ty))) = backings.next_if(|(_, (route, _))| route.steps().len() <= path.steps().len()) {
                Self::initialize_entry_backing(values, memory, owner.root, route, *ty);
            }

            if let Some(fields) = self.entry_buffer_pointer(path, *ty) {
                let owner_path = path.prefix(path.projections().len() - 1, path.steps().len());

                let owners = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(slot.root)), &owner_path, None);

                for owner in owners.into_iter().flatten() {
                    let pointer = bray_bound_tree::StorageProjection::ProductField(fields.pointer);

                    memory.declare_owned_buffer(owner.clone(), owner.project(&[pointer]), fields.element);

                    if self.footprint.complete_reference_types.get(&fields.element) != Some(&true) {
                        for region in memory.owned_regions(&owner, pointer).into_iter().flatten() {
                            let element = region.project(&[bray_bound_tree::StorageProjection::AnyElement]);

                            memory.seed_whole_source(element.clone(), element);
                        }
                    }
                }

                continue;
            }

            let locations = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(slot.root)), path, None);

            for location in locations.into_iter().flatten() {
                memory.entry_reference_at(slot.clone(), *layer, location, *ty, *kind);
            }
        }

        for (owner, (route, ty)) in backings {
            Self::initialize_entry_backing(values, memory, owner.root, route, *ty);
        }

        for (root, path) in &self.entry_whole_sources {
            if !initialized.contains(root) {
                continue;
            }

            let locations = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(*root)), path, None);

            for location in locations.into_iter().flatten() {
                memory.seed_whole_source(location.clone(), location);
            }
        }
    }

    fn initialize_entry_backing(
        values: &bray_symbols::SemanticValueStore,
        memory: &mut crate::analysis::storage_flow::memory::State,
        root: bray_bound_tree::StorageIdentityId,
        path: &bray_bound_tree::StorageReferencePath,
        ty: bray_symbols::TypeId,
    ) {
        let owners = memory.reference_locations(values, crate::analysis::storage_flow::memory::Target::storage(Place::root(root)), path, None);

        for owner in owners.into_iter().flatten() {
            memory.declare_entry_backing(owner, ty);
        }
    }

    fn install_reference_paths<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        demands: &Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        for (identity, _) in storage.identity_entries() {
            let Some(ty) = storage.storage_type(identity) else {
                continue;
            };

            let owner = request.semantic_values().unborrowed_type(ty);

            for (projections, reached) in demands.get(&owner).into_iter().flatten() {
                let Some((reached, prepared)) = crate::storage::reference_path(
                    request,
                    ty,
                    Arc::clone(projections),
                    Some(*reached),
                )?
                else {
                    continue;
                };

                let reference = prepared
                    .reads()
                    .next()
                    .map(|(_, index, target, _)| {
                        (
                            Place {
                                root: identity,
                                projections: projections[..index].into(),
                            },
                            target,
                        )
                    })
                    .or_else(|| {
                        crate::representation::reference_target_type(request, reached).map(
                            |target| {
                                (
                                    Place {
                                        root: identity,
                                        projections: Arc::clone(projections),
                                    },
                                    target,
                                )
                            },
                        )
                    });

                // Owning elements have separate storage. Whole-value reference
                // publication covers the descriptor's physical fields.
                if let Some((path, target)) = reference
                    && bray_bound_tree::StorageProjection::remains_in_allocation(&path.projections)
                {
                    self.value_references
                        .entry(identity)
                        .or_default()
                        .insert(path, target);
                }

                self.prepare_value_routes(
                    request,
                    storage,
                    Place {
                        root: identity,
                        projections: Arc::clone(projections),
                    },
                    reached,
                    &prepared,
                );
            }
        }

        Ok(())
    }

    fn prepare_summary_demands<C: CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        demands: &mut Demands,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut paths = std::collections::BTreeSet::new();

        for (site, contracts) in dependencies.calls() {
            for (subject, _, path) in contracts.storage().subjects() {
                let staged = site
                    .expression()
                    .and_then(|expression| selections.expression(expression))
                    .filter(|selection| selection.stages_inputs())
                    .and_then(|selection| {
                        selection.invocation_input_site(*site, subject.subject_root())
                    })
                    .and_then(|input| match input {
                        bray_bound_tree::BoundExecutionSite::CallInput {
                            expression,
                            ordinal,
                        } => storage.invocation_input(expression, ordinal),
                        _ => None,
                    })
                    .and_then(|access| storage.logical_path(access));

                let source = super::invocation::invocation_argument(
                    request.view(),
                    selections,
                    *site,
                    subject.subject_root(),
                )
                .and_then(|argument| self.expression_places(argument).first());

                let Some(base) = staged.or(source) else {
                    continue;
                };

                let mut place = base.clone();

                place.projections = base
                    .projections
                    .iter()
                    .copied()
                    .chain(path.projections().iter().copied())
                    .collect();

                paths.insert(place);
            }
        }

        for path in paths {
            let Some(ty) = storage.identity_type(path.root) else {
                // Synthesized result slots can exist before their value type is installed.
                continue;
            };

            let owner = request.semantic_values().unborrowed_type(ty);

            let Some((ty, _)) =
                crate::storage::reference_path(request, ty, path.projections.clone(), None)?
            else {
                continue;
            };

            demands
                .entry(owner)
                .or_default()
                .insert((path.projections, ty));
        }

        Ok(())
    }

    fn prepare_value_routes<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        path: Place,
        ty: bray_symbols::TypeId,
        prepared: &bray_bound_tree::StorageReferencePath,
    ) {
        for (position, step) in prepared.steps().iter().enumerate() {
            if let bray_bound_tree::StorageReferenceStep::Backing { index, target, pointer: None } = *step {
                let owner = Place { root: path.root, projections: path.projections[..index].into() };

                self.owned_backings.entry(owner).or_insert_with(|| (prepared.prefix(index, position), target));
            }
        }

        if !storage.identity(path.root).is_some_and(|identity| identity.is_initialized_at_entry()) {
            return;
        }

        for (step, index, target, kind) in prepared
            .reads()
            .filter(|(step, index, _, _)| *step != 0 || *index != 0)
        {
            let prefix = Place {
                root: path.root,
                projections: path.projections[..index].into(),
            };

            self.entry_references
                .entry((
                    prefix.clone(),
                    1 + prepared.steps()[..step]
                        .iter().rev()
                        .take_while(|operation| matches!(operation,
                            bray_bound_tree::StorageReferenceStep::ReadBorrow { index: previous, .. }
                            | bray_bound_tree::StorageReferenceStep::ReadAddress { index: previous, .. }
                            if *previous == index))
                        .count(),
                ))
                .or_insert_with(|| {
                    (
                        prepared.prefix(index, step),
                        Some(target),
                        kind,
                    )
                });
        }

        let layer = 1 + prepared
            .reads().rev()
            .take_while(|(_, index, _, _)| *index == path.projections.len())
            .count();

        if let Some(kind) = crate::representation::reference_kind(request, ty)
            && (kind == bray_symbols::DependencyReferenceKind::Address
                || !path.projections.is_empty() || layer > 1) {
            let target = crate::representation::reference_target_type(request, ty);

            self.entry_references
                .insert((path, layer), (prepared.clone(), target, kind));
        }
    }
}

