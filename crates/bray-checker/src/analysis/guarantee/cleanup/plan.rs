use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundReferenceTarget, CheckedAsync, StorageAccessId, StoragePlan};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{CallableInstanceData, TypeAssociatedLifecycleSlot, TypeData, TypeId};

use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerUnitView, ExecutionCondition, ExecutionPlace,
    TrustedCallContract,
};

pub(crate) struct Cleanup<'a> {
    pub(super) selected: Option<(&'a CheckedAsync, &'a bray_bound_tree::StorageFlow)>,
    pub(in crate::analysis) plan: &'a Plan,
    pub(in crate::analysis::guarantee) borrowed_inputs: BTreeSet<ExecutionPlace>,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct Plan {
    pub(super) requirements: std::sync::Arc<[bray_bound_tree::AsyncStorageRequirement]>,
    pub(super) replacements:
        BTreeMap<StorageAccessId, bray_bound_tree::AsyncStorageCleanupRequirement>,
    pub(super) steps: BTreeMap<StorageAccessId, Vec<Step>>,
    pub(in crate::analysis) lifecycles: BTreeMap<(TypeId, TypeAssociatedLifecycleSlot), Lifecycle>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum Step {
    Opaque {
        ty: TypeId,
        path: Option<bray_bound_tree::StoragePath>,
    },
    Lifecycle {
        ty: TypeId,
        place: Option<ExecutionPlace>,
        path: Option<bray_bound_tree::StoragePath>,
        whole: bool,
        conditional: bool,
        guards: Vec<(
            bray_bound_tree::StoragePath,
            bray_bound_tree::BoundOperator,
            ExecutionCondition,
        )>,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::analysis) struct Lifecycle {
    pub(in crate::analysis) callable: CallableInstanceData,
    pub(in crate::analysis::guarantee) result: TypeId,
    pub(in crate::analysis::guarantee) receiver: BoundReferenceTarget,
    pub(in crate::analysis::guarantee) synchronous: bool,
    pub(in crate::analysis) contract: TrustedCallContract,
}

impl<'a> Cleanup<'a> {
    pub(crate) fn new(
        plan: &'a Plan,
        dependencies: &'a bray_bound_tree::CheckedDependencyContracts,
    ) -> Self {
        let references = dependencies.lifecycle_dependencies();
        let borrowed_inputs = plan.borrowed_inputs(references);

        Self {
            plan,
            borrowed_inputs,
            selected: None,
        }
    }

    pub(crate) fn with_selection(
        mut self,
        cleanup: &'a CheckedAsync,
        flow: &'a bray_bound_tree::StorageFlow,
    ) -> Self {
        self.selected = Some((cleanup, flow));

        self
    }
}

impl Plan {
    pub(in crate::analysis) fn has_opaque_effects(&self) -> bool {
        self.steps.iter().any(|(access, steps)| {
            matches!(
                self.replacements.get(access),
                Some(bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_))
            ) && steps.iter().any(|step| matches!(step, Step::Opaque { .. }))
        })
    }

    pub(in crate::analysis) fn represented_paths(
        &self,
    ) -> impl Iterator<Item = &bray_bound_tree::StoragePath> {
        self.steps.values().flatten().filter_map(|step| match step {
            Step::Opaque { path, .. } | Step::Lifecycle { path, .. } => path.as_ref(),
        })
    }

    pub(in crate::analysis) fn callees(
        &self,
    ) -> impl Iterator<Item = bray_symbols::CallableSymbolId> + '_ {
        self.lifecycles
            .values()
            .map(|lifecycle| lifecycle.callable.definition().callable_symbol())
    }

    pub(crate) fn check_dependencies<C: CheckerRequestContext + ?Sized>(
        &self,
        request: CheckerUnitView<'_, C>,
    ) -> Result<
        BTreeMap<CallableInstanceData, bray_symbols::DependencyContractTemplateData>,
        CheckerQueryError<C::UpstreamError>,
    > {
        let mut references = BTreeMap::new();

        for lifecycle in self.lifecycles.values() {
            if let std::collections::btree_map::Entry::Vacant(entry) =
                references.entry(lifecycle.callable)
            {
                let template = request.callable_result_dependencies(
                    lifecycle.callable.definition().callable_symbol(),
                )?;

                let template = crate::dependency::resolved_result_template(
                    request,
                    Some(lifecycle.callable),
                    template,
                )?;

                entry.insert(template);
            }
        }

        Ok(references)
    }

    fn borrowed_inputs(
        &self,
        references: &BTreeMap<CallableInstanceData, bray_symbols::DependencyContractTemplateData>,
    ) -> BTreeSet<ExecutionPlace> {
        self.steps
            .values()
            .flatten()
            .filter_map(|step| match step {
                Step::Lifecycle {
                    ty,
                    place: Some(place),
                    ..
                } if place.is_callable_input() => Some((ty, place)),
                _ => None,
            })
            .flat_map(|(ty, place)| {
                [
                    TypeAssociatedLifecycleSlot::Finalizer,
                    TypeAssociatedLifecycleSlot::Destructor,
                ]
                .into_iter()
                .filter_map(move |slot| self.lifecycles.get(&(*ty, slot)))
                .flat_map(move |lifecycle| {
                    references[&lifecycle.callable]
                        .borrows()
                        .iter()
                        .filter_map(move |relation| {
                            (relation.dereferences() == 0
                                && relation.target_projections().is_empty()
                                && relation.entry() == relation.exit()
                                && relation.exit().subject_root()
                                    == bray_symbols::DependencySubjectRoot::Receiver)
                                .then(|| {
                                    super::super::trusted::project_place(
                                        place.clone(),
                                        relation.exit(),
                                    )
                                })
                                .flatten()
                        })
                })
            })
            .collect()
    }

    pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) -> Result<DiagnosticResult<Self>, CheckerQueryError<C::UpstreamError>> {
        let mut prepared = Self::default();

        let mut diagnostics = DiagnosticBag::new();

        if request.unit().key().kind() == bray_bound_tree::BoundUnitKind::ContractClause {
            return Ok(DiagnosticResult::new(prepared, diagnostics));
        }

        prepared.requirements = cleanup_shapes.storage_requirements(storage)?;

        let mut accesses = storage
            .identity_entries()
            .filter_map(|(identity, _)| {
                if bray_bound_tree::storage_identity_transfers_at_unit_exit(storage, identity) {
                    return None;
                }

                Some((
                    storage.root_access(identity)?,
                    storage.storage_type(identity)?,
                ))
            })
            .collect::<BTreeMap<_, _>>();

        for plan in storage
            .access_plans()
            .iter()
            .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment)
        {
            let access = storage
                .access(plan.access())
                .expect("replacement retains its checked access");

            accesses.insert(plan.access(), access.reached_type());
        }

        let mut types = BTreeSet::new();

        for (access, ty) in accesses {
            let reached = storage
                .access(access)
                .expect("checked cleanup retains its storage access");

            prepared
                .replacements
                .insert(access, cleanup_shapes.requirement(ty)?);

            let expanded = crate::asynchronous::full_cleanup_parts(
                cleanup_shapes,
                ty,
                reached.source(),
                storage.root_identity(access).is_some_and(|identity| {
                    bray_bound_tree::storage_identity_is_destructor_receiver(
                        request.unit(),
                        storage,
                        identity,
                    ) && storage.is_root_access(access)
                }),
            )?;

            let (parts, additional) = expanded.into_parts();

            diagnostics.add_range(additional);

            let mut steps: Vec<_> = parts
                .into_iter()
                .map(|(ty, part)| {
                    let path = part.as_ref().and_then(|part| part.storage_path(storage, access));

                    if part.as_ref().is_some_and(|part| {
                        part.release().is_some() || !part.phases().includes_lifecycle()
                    }) || !matches!(request.semantic_values().type_data(ty).as_ref(), TypeData::Named { .. }) {
                        Step::Opaque { ty, path }
                    } else {
                        types.insert(ty);

                        let place = path.as_ref().and_then(|path| ExecutionPlace::storage_path(storage, path));

                        Step::Lifecycle {
                            ty,
                            guards: cleanup_guards(request.semantic_values(), storage, access, part.as_ref()),
                            path,
                            place,
                            conditional: part.as_ref().is_none_or(|part| part.projections().iter().any(|projection| {
                                matches!(projection.projection(),
                                    bray_bound_tree::StorageCleanupProjectionKind::Component(
                                        bray_bound_tree::StorageProjection::NullableValue
                                        | bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }))
                            })),
                            whole: part
                                .as_ref()
                                .is_some_and(|part| part.projections().is_empty()),
                        }
                    }
                })
                .collect();

            if steps.is_empty() {
                // Opaque native cleanup has no represented lifecycle whose effects we can apply.
                steps.push(Step::Opaque {
                    ty,
                    path: storage.logical_path(access).cloned(),
                });
            }

            prepared.steps.insert(access, steps);
        }

        for ty in types {
            for slot in [
                TypeAssociatedLifecycleSlot::Finalizer,
                TypeAssociatedLifecycleSlot::Destructor,
            ] {
                let selected = request.context().lifecycle_callable(ty, slot)?;

                diagnostics.add_range(selected.diagnostics().iter().cloned());

                if selected.diagnostics().has_errors() {
                    continue;
                }

                let Some((callable, signature)) = selected.value() else {
                    continue;
                };

                let lifecycle = lifecycle(request.semantic_values(), *callable, signature)
                    .expect("selected lifecycle contract uses checked semantic terms");

                prepared.lifecycles.insert((ty, slot), lifecycle);
            }
        }

        Ok(DiagnosticResult::new(prepared, diagnostics))
    }
}

fn lifecycle(
    values: &bray_symbols::SemanticValueStore,
    target: CallableInstanceData,
    signature: &bray_symbols::CallableSignature,
) -> Result<Lifecycle, bray_symbols::SemanticValueStoreError> {
    use bray_symbols::TypeData;

    let data = values.type_data(signature.callable_type());

    let TypeData::Callable(callable) = data.as_ref() else {
        panic!("selected lifecycle must have a callable type");
    };

    let references = signature
        .receiver()
        .map(|receiver| bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into()))
        .into_iter()
        .chain(signature.parameters().iter().map(|parameter| {
            bray_bound_tree::BoundReferenceTarget::Surface(parameter.parameter().into())
        }))
        .collect::<Vec<_>>();

    let behavior = callable.phase_behaviors().invocation();

    let completion = callable
        .phase_behaviors()
        .deferred_execution()
        .unwrap_or(behavior);

    let mut contract = crate::TrustedCallContract {
        // Failed finalizer attempts can still be followed by destruction.
        completes: callable.execution() == bray_symbols::CallableExecution::Synchronous
            && (!behavior.may_unwind()
                || behavior
                    .execution_properties()
                    .contains(&bray_symbols::ExecutionProperty::Total)),
        ..Default::default()
    };

    for (predicates, completion) in [
        (behavior.predicate_requirements(), false),
        (completion.predicate_guarantees(), true),
    ] {
        for predicate in predicates {
            let condition = match predicate
                .condition()
                .map(|term| crate::execution_condition_from_term(values, term, &references))
                .transpose()
            {
                Ok(condition) => condition.unwrap_or(ExecutionCondition::Unknown),
                Err(error) => return Err(error),
            };

            let destination = match (completion, predicate.is_trusted()) {
                (false, true) => &mut contract.requirements,
                (false, false) => &mut contract.preconditions,
                (true, true) => &mut contract.guarantees,
                (true, false) => &mut contract.postconditions,
            };

            destination.push(condition);
        }
    }

    Ok(Lifecycle {
        callable: target,
        result: signature.result(),
        receiver: BoundReferenceTarget::Surface(
            signature
                .receiver()
                .expect("selected lifecycle has a receiver")
                .parameter()
                .into(),
        ),
        synchronous: callable.execution() == bray_symbols::CallableExecution::Synchronous,
        contract,
    })
}

fn cleanup_guards(
    values: &bray_symbols::SemanticValueStore,
    storage: &StoragePlan,
    access: StorageAccessId,
    part: Option<&bray_bound_tree::StorageCleanupPart>,
) -> Vec<(
    bray_bound_tree::StoragePath,
    bray_bound_tree::BoundOperator,
    ExecutionCondition,
)> {
    use bray_bound_tree::{PatternPredicate, StorageCleanupProjectionKind, StorageProjection};

    let Some(part) = part else {
        return Vec::new();
    };

    // Each guard owns its prefix because later projections descend into the selected payload.
    let Some(mut path) = storage.logical_path(access).cloned() else {
        return Vec::new();
    };

    let mut guards = Vec::new();

    for projection in part.projections() {
        let StorageCleanupProjectionKind::Component(component) = projection.projection() else {
            break;
        };

        let predicate = match component {
            StorageProjection::NullableValue => Some(PatternPredicate::NullablePresent),
            StorageProjection::ActiveUnionPayloadField { variant, .. } => {
                Some(PatternPredicate::ActiveUnionVariant(variant))
            }
            _ => None,
        };

        if let Some(predicate) = predicate {
            let (operator, expected) = crate::execution_guarantees::pattern_value(
                values,
                projection.source_type(),
                predicate,
            )
            .expect("cleanup presence guards have checked pattern values");

            guards.push((path.clone(), operator, expected));
        }

        path.projections = path
            .projections
            .iter()
            .copied()
            .chain([component])
            .collect();
    }

    guards
}
