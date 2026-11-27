use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundReferenceTarget, StorageAccessId, StoragePath, StoragePlan, StorageReferencePath, StorageReferenceStep};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{CallableInstanceData, DependencyFailureKind, TypeAssociatedLifecycleSlot, TypeData, TypeId};

use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerUnitView, ExecutionCondition, ExecutionPlace,
    TrustedCallContract,
};

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct Plan {
    cleanup_callees: BTreeSet<bray_symbols::CallableSymbolId>,
    pub(crate) expression_cleanup_calls:
        BTreeMap<bray_bound_tree::BoundExpressionId, BTreeSet<bray_bound_tree::BodyBehaviorCall>>,
    suspending: BTreeSet<StorageAccessId>,
    pub(in crate::analysis::guarantee) future_capture_cleanup: BTreeMap<
        bray_bound_tree::BoundExpressionId,
        Vec<TypeId>,
    >,
    pub(crate) scoped_quiescence:
        BTreeMap<bray_bound_tree::StorageScope, bray_bound_tree::BoundExpressionId>,
    pub(in crate::analysis) requirements:
        std::sync::Arc<crate::asynchronous::StorageCleanupRequirements>,
    pub(super) steps: BTreeMap<StorageAccessId, Vec<Step>>,
    pub(in crate::analysis) lifecycles: BTreeMap<(TypeId, TypeAssociatedLifecycleSlot), Lifecycle>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum Step {
    Opaque {
        phases: bray_bound_tree::AsyncCleanupPhases,
        ty: TypeId,
        releases_storage: bool,
        failures: &'static [DependencyFailureKind],
        selection: Selection,
    },
    Lifecycle {
        phases: bray_bound_tree::AsyncCleanupPhases,
        ty: TypeId,
        place: Option<ExecutionPlace>,
        whole: bool,
        represented: bool,
        selection: Selection,
    },
}

impl Step {
    pub(super) fn phases(&self) -> bray_bound_tree::AsyncCleanupPhases {
        match self {
            Self::Opaque { phases, .. } | Self::Lifecycle { phases, .. } => *phases,
        }
    }

    pub(super) fn selection(&self) -> &Selection {
        match self {
            Self::Opaque { selection, .. } | Self::Lifecycle { selection, .. } => selection,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct Selection {
    pub(super) owner: Option<StoragePath>,
    pub(super) route: Option<StorageReferencePath>,
    pub(super) guards: Vec<SelectionGuard>,
    pub(super) repeat_from: Option<usize>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct SelectionGuard {
    pub(super) subject: StoragePath,
    pub(super) operator: bray_bound_tree::BoundOperator,
    pub(super) expected: ExecutionCondition,
    pub(super) payload: Option<StoragePath>,
}

impl Selection {
    pub(super) fn target(&self) -> Option<&StoragePath> {
        self.owner.as_ref().filter(|_| self.route.is_some())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::analysis) struct Lifecycle {
    pub(in crate::analysis) call: bray_bound_tree::LifecycleCallable,
    pub(in crate::analysis::guarantee) receiver: BoundReferenceTarget,
    pub(in crate::analysis) contract: TrustedCallContract,
}

impl Plan {
    pub(in crate::analysis) fn storage_scope(&self, identity: bray_bound_tree::StorageIdentityId) -> Option<bray_bound_tree::StorageScope> {
        self.requirements.get(identity)
            .and_then(bray_bound_tree::AsyncStorageRequirement::owner)
    }

    pub(in crate::analysis) fn opaque_types(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.steps.values().flatten().filter_map(|step| match step {
            Step::Opaque { ty, .. } => Some(*ty),
            Step::Lifecycle { .. } => None,
        })
    }

    pub(in crate::analysis::guarantee) fn repeats_action(&self, action: bray_bound_tree::StorageCleanupAction) -> bool {
        action.kind != bray_bound_tree::StorageCleanupActionKind::Broadcast
            && self.steps.get(&action.access).into_iter().flatten().skip(action.part).any(|step| {
                step.selection().repeat_from.is_some_and(|first| first <= action.part)
            })
    }

    pub(in crate::analysis) fn fallible_scope_owners<'a>(
        &'a self,
        storage: &'a StoragePlan,
        dependencies: &'a bray_bound_tree::CheckedDependencyContracts,
        scope: bray_bound_tree::StorageScope,
    ) -> impl Iterator<Item = bray_bound_tree::StorageIdentityId> + 'a {
        self.requirements.scope_candidates(scope).filter(move |&identity| {
            storage.root_access(identity)
                .is_none_or(|access| !self.has_infallible_cleanup(dependencies, access))
        })
    }

    fn has_infallible_cleanup(
        &self,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        access: StorageAccessId,
    ) -> bool {
        self.steps.get(&access).is_some_and(|steps| {
            steps.iter().all(|step| {
                let ty = match step {
                    Step::Opaque { failures, .. } => return failures.is_empty(),
                    Step::Lifecycle { ty, .. } => *ty,
                };

                let Some(destructor) = self.lifecycles.get(&(ty, TypeAssociatedLifecycleSlot::Destructor)) else {
                    return false;
                };

                // Finalization can produce a typed incident independently of call failure.
                !self.lifecycles.contains_key(&(ty, TypeAssociatedLifecycleSlot::Finalizer))
                    && destructor.is_infallible_destructor(&dependencies.lifecycle_dependencies()[&destructor.call.callable])
            })
        })
    }

    pub(crate) fn finalizer(&self, ty: TypeId) -> Option<bray_bound_tree::LifecycleCallable> {
        self.lifecycles
            .get(&(ty, TypeAssociatedLifecycleSlot::Finalizer))
            .map(|lifecycle| lifecycle.call.clone())
    }

    pub(crate) fn asynchronous_scopes(
        &self,
        storage: &StoragePlan,
    ) -> BTreeSet<bray_bound_tree::StorageScope> {
        self.requirements
            .entries()
            .iter()
            .filter_map(|requirement| {
                let scope = requirement.owner()?;
                let access = storage.root_access(requirement.identity())?;

                self.requires_suspension(access).then_some(scope)
            })
            .chain(self.scoped_quiescence.keys().copied())
            .collect()
    }

    pub(crate) fn asynchronous_replacements(
        &self,
        storage: &StoragePlan,
    ) -> BTreeSet<bray_bound_tree::BoundExpressionId> {
        storage
            .access_plans()
            .iter()
            .filter(|plan| {
                plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment
                    && self.requires_suspension(plan.access())
            })
            .map(|plan| plan.expression())
            .collect()
    }

    pub(crate) fn requires_suspension(&self, access: StorageAccessId) -> bool {
        self.suspending.contains(&access)
    }

    pub(crate) fn deferred_cleanup_calls(
        &self,
        access: StorageAccessId,
    ) -> impl Iterator<Item = bray_bound_tree::BodyBehaviorCall> + '_ {
        self.steps
            .get(&access)
            .into_iter()
            .flatten()
            .filter_map(|step| match step {
                Step::Lifecycle { ty, .. } => Some(*ty),
                Step::Opaque { .. } => None,
            })
            .flat_map(|ty| self.lifecycle_calls(ty))
    }

    fn lifecycle_calls(
        &self,
        ty: TypeId,
    ) -> impl Iterator<Item = bray_bound_tree::BodyBehaviorCall> + '_ {
        [
            TypeAssociatedLifecycleSlot::Finalizer,
            TypeAssociatedLifecycleSlot::Destructor,
        ]
        .into_iter()
        .filter_map(move |slot| self.lifecycles.get(&(ty, slot)))
        .map(|lifecycle| {
            let phase = if lifecycle.call.result.execution()
                == bray_symbols::CallableExecution::Asynchronous
            {
                bray_bound_tree::BodyBehaviorPhase::DeferredExecution
            } else {
                bray_bound_tree::BodyBehaviorPhase::Invocation
            };

            bray_bound_tree::BodyBehaviorCall::new(
                bray_bound_tree::BoundCallableTarget::Declaration(lifecycle.call.callable),
                phase,
            )
        })
    }

    pub(in crate::analysis) fn represented_paths(
        &self,
    ) -> impl Iterator<Item = &bray_bound_tree::StoragePath> {
        self.steps.values().flatten().filter_map(|step| step.selection().target())
    }

    pub(in crate::analysis) fn owner_roots(
        &self,
    ) -> impl Iterator<Item = bray_bound_tree::StorageIdentityId> + '_ {
        self.steps.values().flatten().filter_map(|step| {
            step.selection().owner.as_ref().map(|path| path.root)
        })
    }

    pub(in crate::analysis) fn reporting_accesses<'a>(
        &'a self,
        dependencies: &'a bray_bound_tree::CheckedDependencyContracts,
    ) -> impl Iterator<Item = StorageAccessId> + 'a {
        self.steps.iter().filter_map(|(access, steps)| {
            steps
                .iter()
                .any(|step| {
                    let Step::Lifecycle { ty, .. } = step else {
                        return false;
                    };

                    [
                        TypeAssociatedLifecycleSlot::Finalizer,
                        TypeAssociatedLifecycleSlot::Destructor,
                    ]
                    .into_iter()
                    .filter_map(|slot| self.lifecycles.get(&(*ty, slot)))
                    .any(|lifecycle| {
                        let contract =
                            &dependencies.lifecycle_dependencies()[&lifecycle.call.callable];

                        contract.captured_values().iter().filter_map(|outcome| outcome.report.as_deref())
                    .chain(contract.failure_outcomes().into_iter().flatten().filter_map(|row| row.report.as_deref()))
                            .any(|requirements| !requirements.is_empty())
                            || (lifecycle
                                .call
                                .callable
                                .definition()
                                .callable_symbol()
                                .kind()
                                == bray_symbols::SymbolKind::Finalizer
                                && !contract.requirements().is_empty())
                    })
                })
                .then_some(*access)
        })
    }

    pub(in crate::analysis) fn callees(
        &self,
    ) -> impl Iterator<Item = bray_symbols::CallableSymbolId> + '_ {
        self.cleanup_callees.iter().copied()
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
                references.entry(lifecycle.call.callable)
            {
                let template = request.callable_result_dependencies(
                    lifecycle.call.callable.definition().callable_symbol(),
                )?;

                let template = crate::dependency::resolved_result_template(
                    request,
                    Some(lifecycle.call.callable),
                    template,
                    None,
                )?;

                entry.insert(template);
            }
        }

        Ok(references)
    }

    pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        expressions: &bray_bound_tree::CheckedExpressionSemantics,
        graph: &crate::analysis::ControlFlowGraph,
    ) -> Result<DiagnosticResult<Self>, CheckerQueryError<C::UpstreamError>> {
        let mut prepared = Self::default();

        let mut diagnostics = DiagnosticBag::new();

        prepared.requirements = cleanup_shapes.storage_requirements(storage)?;

        let abandoned = graph.operations().iter().filter_map(|operation| {
            let crate::analysis::AnalysisOperationKind::ScopeExit {
                scope: bray_bound_tree::StorageScope::AbandonedTransfer { value, .. }, ..
            } = operation.kind() else { return None; };

            Some(value)
        }).collect::<BTreeSet<_>>();

        let mut accesses = storage
            .identity_entries()
            .filter_map(|(identity, slot)| {
                if !slot.is_invocation_owned(storage.kind())
                    || bray_bound_tree::storage_identity_transfers_at_unit_exit(storage, identity)
                        && !abandoned.contains(&identity)
                {
                    return None;
                }

                Some((
                    storage.root_access(identity)?,
                    storage.storage_type(identity)?,
                ))
            })
            .collect::<BTreeMap<_, _>>();

        for access in prepared.requirements.replacement_accesses() {
            let ty = storage.access(access)
                .expect("replacement retains its checked access")
                .reached_type();

            accesses.insert(access, ty);
        }

        let mut types = BTreeSet::new();

        for (access, ty) in accesses {
            let reached = storage
                .access(access)
                .expect("checked cleanup retains its storage access");

            if cleanup_shapes.requires_suspension(ty)? {
                prepared.suspending.insert(access);
            }

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

            let mut steps = Vec::new();

            super::selection::visit_cleanup_parts(request.semantic_values(), &parts, |ty, part, indices, repeat| {
                    let phases = part.map_or(bray_bound_tree::AsyncCleanupPhases::CancellationThenLifecycle,
                        bray_bound_tree::StorageCleanupPart::phases);

                    let mut selection = cleanup_selection(request.semantic_values(), storage, access, part, indices);

                    selection.repeat_from = repeat.map(|length| steps.len() + 1 - length);

                    let step = if !cleanup_shapes.has_source_lifecycle(ty, part)? {
                        let releases_storage = part.is_some_and(|part| part.release().is_some());

                        let failures = if releases_storage {
                            &DependencyFailureKind::ALL
                        } else {
                            opaque_cleanup_failure_kinds(cleanup_shapes, request, ty)?
                        };

                        Step::Opaque { phases, ty, releases_storage, failures, selection }
                    } else {
                        types.insert(ty);

                        let place = selection.target().and_then(|path| ExecutionPlace::storage_path(storage, path));

                        Step::Lifecycle {
                            phases,
                            ty,
                            selection,
                            place,
                            whole: part.is_some_and(|part| part.projections().is_empty()),
                            represented: part.is_some(),
                        }
                    };

                    steps.push(step);

                    Ok::<(), CheckerQueryError<C::UpstreamError>>(())
            })?;

            if parts.is_empty() {
                // Opaque native cleanup has no represented lifecycle whose effects we can apply.
                let failures = opaque_cleanup_failure_kinds(cleanup_shapes, request, ty)?;

                steps.push(Step::Opaque {
                    phases: bray_bound_tree::AsyncCleanupPhases::CancellationThenLifecycle,
                    ty,
                    releases_storage: false,
                    failures,
                    selection: Selection {
                        owner: storage.logical_path(access).cloned(),
                        route: Some(StorageReferencePath::new([].into(), [].into())),
                        ..Selection::default()
                    },
                });
            }

            prepared.steps.insert(access, steps);
        }

        let input_parts = super::inputs::parts(
            cleanup_shapes,
            request,
            expressions,
            &mut types,
            &mut diagnostics,
        )?;

        let future_results = expressions.selections().entries().iter().filter_map(|entry| {
            let bray_bound_tree::SemanticSelection::Call(call) = entry.selection() else {
                return None;
            };

            match call.resolution().result() {
                bray_bound_tree::BoundCallResult::LazyFuture(future) => Some(future.completion_type()),
                bray_bound_tree::BoundCallResult::Immediate(_) => None,
            }
        });

        prepared.cleanup_callees = crate::dependency::cleanup_callees(request,
            types.iter().copied().chain(prepared.opaque_types()).chain(input_parts.keys().copied()).chain(future_results))?;

        prepared.prepare_lifecycles(request, types, &mut diagnostics)?;

        prepared.cleanup_callees.extend(prepared.lifecycles.values()
            .map(|lifecycle| lifecycle.call.callable.definition().callable_symbol()));

        let mut input_cleanup = BTreeMap::new();

        for (ty, parts) in input_parts {
            if cleanup_shapes.requires_suspension(ty)? {
                let calls = parts
                    .into_iter()
                    .filter(|(_, projection)| {
                        projection.as_ref().is_none_or(|part| {
                            part.release().is_none() && part.phases().includes_lifecycle()
                        })
                    })
                    .flat_map(|(part, _)| prepared.lifecycle_calls(part))
                    .collect();

                input_cleanup.insert(ty, calls);
            }
        }

        prepared.select_inputs(
            request.unit(),
            expressions.types(),
            expressions.selections(),
            graph,
            &input_cleanup,
        );

        prepared.select_buffer_cleanup(
            expressions.selections(),
            request.semantic_values(),
            &input_cleanup,
        );

        prepared.select_scoped_quiescence(cleanup_shapes, request, expressions, graph)?;
        prepared.select_future_cleanup(cleanup_shapes, request, expressions.selections())?;

        Ok(DiagnosticResult::new(prepared, diagnostics))
    }

    fn prepare_lifecycles<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        types: BTreeSet<TypeId>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
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

                let mut lifecycle = lifecycle(
                    request.semantic_values(),
                    request.available_compiler_known_symbols(),
                    *callable,
                    signature,
                )
                .expect("selected lifecycle contract uses checked semantic terms");

                request.context().classify_trusted_completion(&mut lifecycle.contract, diagnostics)?;

                self.lifecycles.insert((ty, slot), lifecycle);
            }
        }

        Ok(())
    }
}

fn opaque_cleanup_failure_kinds<C: CheckerRequestContext + ?Sized>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<&'static [DependencyFailureKind], CheckerQueryError<C::UpstreamError>> {
    use bray_compiler_known::RepresentationRole;

    match crate::representation::type_representation(request, ty) {
        Some(RepresentationRole::PanicReport) => return Ok(&[]),
        Some(RepresentationRole::String) => return Ok(&[DependencyFailureKind::Panic]),
        _ => {}
    }

    if let TypeData::Named { definition, substitution } = request.semantic_values().type_data(ty).as_ref()
        && let Some(element) = request.context().raw_buffer_element(*definition, *substitution)?
        && cleanup_shapes.requirement(element)? == bray_bound_tree::AsyncStorageCleanupRequirement::None
    {
        // The initialized prefix needs no lifecycle work, so the generated
        // backing release only deallocates. A total deallocation cannot fail.
        if request.context().standard_deallocation_completes()? {
            return Ok(&[]);
        }

        return Ok(&[DependencyFailureKind::Panic]);
    }

    Ok(&DependencyFailureKind::ALL)
}

fn lifecycle(
    values: &bray_symbols::SemanticValueStore,
    symbols: &bray_symbols::AvailableCompilerKnownSymbols,
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
        .map(|receiver| BoundReferenceTarget::Surface(receiver.parameter().into()))
        .into_iter()
        .chain(
            signature
                .parameters()
                .iter()
                .map(|parameter| BoundReferenceTarget::Surface(parameter.parameter().into())),
        )
        .collect::<Vec<_>>();

    let behavior = callable.phase_behaviors().invocation();

    let completion = callable
        .phase_behaviors()
        .deferred_execution()
        .unwrap_or(behavior);

    // Failed finalizer attempts can still be followed by destruction.
    let completes = callable.execution() == bray_symbols::CallableExecution::Synchronous
        && (!behavior.may_unwind()
            || behavior.execution_properties().contains(&bray_symbols::ExecutionProperty::Total));

    let contract = TrustedCallContract::try_from_phases(values, symbols, behavior, completion, completes,
        |predicate| predicate.condition().map(|term| crate::execution_condition_from_term(values, term, &references))
            .transpose().map(|condition| condition.unwrap_or_else(ExecutionCondition::unknown)))?;

    Ok(Lifecycle {
        call: bray_bound_tree::LifecycleCallable::from_signature(
            values, symbols, target, signature,
        )?,
        receiver: BoundReferenceTarget::Surface(
            signature
                .receiver()
                .expect("selected lifecycle has a receiver")
                .parameter()
                .into(),
        ),
        contract,
    })
}

fn cleanup_selection(
    values: &bray_symbols::SemanticValueStore,
    storage: &StoragePlan,
    access: StorageAccessId,
    part: Option<&bray_bound_tree::StorageCleanupPart>,
    indices: &[bray_symbols::SymbolOrdinal],
) -> Selection {
    use bray_bound_tree::{PatternPredicate, StorageCleanupProjectionKind, StorageProjection};

    let Some(part) = part else {
        return Selection::default();
    };

    // Each guard owns its prefix because later projections descend into the selected payload.
    let Some(mut path) = storage.logical_path(access).cloned() else {
        return Selection::default();
    };

    let mut guards = Vec::new();
    let base = path.projections.len();
    let mut steps = Vec::new();
    let mut indices = indices.iter().copied();

    for projection in part.projections() {
        let predicate = match projection.projection() {
            StorageCleanupProjectionKind::Component(StorageProjection::NullableValue) => {
                Some(PatternPredicate::NullablePresent)
            }
            StorageCleanupProjectionKind::Component(StorageProjection::ActiveUnionPayloadField { variant, .. })
            | StorageCleanupProjectionKind::UnionPayloadElement { variant, .. } => {
                Some(PatternPredicate::ActiveUnionVariant(variant))
            }
            _ => None,
        };

        let component = match projection.projection() {
            StorageCleanupProjectionKind::Component(component) => Some(component),
            StorageCleanupProjectionKind::OwnedTarget(_) => {
                steps.push(StorageReferenceStep::Backing {
                    index: path.projections.len() - base,
                    target: projection.result_type(),
                    pointer: None,
                });

                Some(StorageProjection::OwnedTarget)
            }
            StorageCleanupProjectionKind::ArrayElements(_) => Some(indices.next().map_or(
                StorageProjection::AnyElement, StorageProjection::ElementFromStart,
            )),
            _ => None,
        };

        let payload = component.map(|component| {
            StoragePath {
                root: path.root,
                projections: path.projections.iter().copied().chain([component]).collect(),
            }
        });

        if let Some(predicate) = predicate {
            let (operator, expected) = crate::execution_guarantees::pattern_value(
                values,
                projection.source_type(),
                predicate,
            )
            .expect("cleanup presence guards have checked pattern values");

            guards.push(SelectionGuard {
                subject: path.clone(),
                operator,
                expected,
                payload: payload.as_ref().filter(|payload| payload.is_exact()).cloned(),
            });
        }

        let Some(payload) = payload else {
            return Selection { owner: Some(path), guards, ..Selection::default() };
        };

        path = payload;
    }

    let route = StorageReferencePath::new(path.projections[base..].into(), steps.into());

    Selection { owner: Some(path), route: Some(route), guards, ..Selection::default() }
}
