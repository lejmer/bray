use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundReferenceTarget, CheckedAsync, StorageAccessId, StoragePlan};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{CallableInstanceData, TypeAssociatedLifecycleSlot, TypeData, TypeId};

use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerUnitView, ExecutionCondition, ExecutionPlace,
    TrustedCallContract,
};

#[derive(Default)]
pub(in crate::analysis::guarantee) struct Cleanup<'a> {
    pub(super) selected: Option<(&'a CheckedAsync, &'a bray_bound_tree::StorageFlow)>,
    pub(super) steps: BTreeMap<StorageAccessId, Vec<Step>>,
    pub(super) lifecycles: BTreeMap<(TypeId, TypeAssociatedLifecycleSlot), Lifecycle>,
}

pub(super) enum Step {
    Invalidate,
    Lifecycle {
        ty: TypeId,
        place: Option<ExecutionPlace>,
        whole: bool,
        conditional: bool,
    },
}

pub(in crate::analysis::guarantee) struct Lifecycle {
    pub(in crate::analysis::guarantee) callable: CallableInstanceData,
    pub(in crate::analysis::guarantee) result: TypeId,
    pub(in crate::analysis::guarantee) receiver: BoundReferenceTarget,
    pub(in crate::analysis::guarantee) synchronous: bool,
    pub(in crate::analysis::guarantee) contract: TrustedCallContract,
    pub(in crate::analysis::guarantee) borrows: Vec<bray_symbols::DependencyBorrowRelation>,
}

impl<'a> Cleanup<'a> {
    pub(in crate::analysis::guarantee) fn borrowed_inputs(&self) -> BTreeSet<ExecutionPlace> {
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
                    lifecycle.borrows.iter().filter_map(move |relation| {
                        (relation.exit().subject_root()
                            == bray_symbols::DependencySubjectRoot::Receiver)
                            .then(|| {
                                super::super::trusted::project_place(place.clone(), relation.exit())
                            })
                            .flatten()
                    })
                })
            })
            .collect()
    }

    pub(in crate::analysis::guarantee) fn requires_validation(&self) -> bool {
        self.lifecycles
            .values()
            .any(|lifecycle| !lifecycle.contract.requirements.is_empty())
    }

    pub(in crate::analysis::guarantee) fn prepare<C: CheckerRequestContext + ?Sized>(
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        cleanup: &'a CheckedAsync,
        flow: &'a bray_bound_tree::StorageFlow,
    ) -> Result<DiagnosticResult<Self>, CheckerQueryError<C::UpstreamError>> {
        let mut prepared = Self {
            selected: Some((cleanup, flow)),
            ..Self::default()
        };

        let mut diagnostics = DiagnosticBag::new();

        if request.unit().key().kind() == bray_bound_tree::BoundUnitKind::ContractClause {
            prepared.selected = None;

            return Ok(DiagnosticResult::new(prepared, diagnostics));
        }

        let accesses = cleanup
            .scope_exits()
            .iter()
            .flat_map(|plan| plan.lifecycle_resolution().iter().copied())
            .map(|access| {
                let parts = storage
                    .root_identity(access)
                    .and_then(|identity| cleanup.storage_requirement(identity))
                    .and_then(|requirement| requirement.parts());

                (access, parts)
            })
            .chain(
                cleanup
                    .replacements()
                    .iter()
                    .filter(|plan| {
                        matches!(
                            plan.cleanup(),
                            bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                        )
                    })
                    .map(|plan| (plan.access(), plan.parts())),
            )
            .collect::<BTreeMap<_, _>>();

        let mut types = BTreeSet::new();

        for (access, selected) in accesses {
            let reached = storage
                .access(access)
                .expect("checked cleanup retains its storage access");

            let expanded = crate::asynchronous::full_cleanup_parts(
                request,
                reached.reached_type(),
                reached.source(),
                selected,
            )?;

            let (parts, additional) = expanded.into_parts();

            diagnostics.add_range(additional);

            let mut steps: Vec<_> = parts
                .into_iter()
                .map(|(ty, part)| {
                    if part.as_ref().is_some_and(|part| {
                        part.release().is_some() || !part.phases().includes_lifecycle()
                    }) || !matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        TypeData::Named { .. }
                    ) {
                        Step::Invalidate
                    } else {
                        types.insert(ty);

                        Step::Lifecycle {
                            ty,
                            place: part.as_ref().and_then(|part| {
                                ExecutionPlace::cleanup_part(storage, access, part)
                            }),
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

            if steps.is_empty() && selected.is_none() {
                // Opaque native cleanup has no represented lifecycle whose effects we can apply.
                steps.push(Step::Invalidate);
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

                let mut lifecycle = lifecycle(request.semantic_values(), *callable, signature)
                    .expect("selected lifecycle contract uses checked semantic terms");

                lifecycle.contract.completes |= lifecycle.synchronous
                    && request.target_cannot_unwind(
                        bray_bound_tree::BoundCallableTarget::Declaration(*callable),
                    );

                let template = request
                    .context()
                    .callable_result_dependencies(callable.definition().callable_symbol())?;

                let template = crate::dependency::resolved_result_template(
                    request,
                    Some(*callable),
                    template,
                )?;

                lifecycle.borrows = template.borrows().to_vec();

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
        borrows: Vec::new(),
    })
}
