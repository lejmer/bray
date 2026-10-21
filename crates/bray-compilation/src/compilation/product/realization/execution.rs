use std::collections::BTreeMap;

use bray_codegen::{CodegenInstance, CodegenInstanceKey, CodegenTarget};
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirGeneratedLifecycleRole, MirOperand, MirOperationId,
    MirOperationKind, MirTerminatorKind, MirUnit, MirValueOrigin,
};
use bray_runtime_interface::ExecutionLaneRequirement;

use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product) fn frame_execution_requirements<'a>(
        &self,
        instances: impl IntoIterator<Item = (&'a CodegenInstance, &'a ConcreteCodegenInstance)>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            BTreeMap<CodegenInstanceKey, Vec<[&'static [ExecutionLaneRequirement]; 3]>>,
            BTreeMap<CodegenInstanceKey, &'static [ExecutionLaneRequirement]>,
        ),
        CodegenPreparationError,
    > {
        let instances = instances.into_iter().collect::<Vec<_>>();

        let helpers = instances
            .iter()
            .copied()
            .filter(|(_, owner)| owner.generated_lifecycle_reference().is_some())
            .map(|(instance, owner)| (instance.key(), (instance, owner)))
            .collect::<BTreeMap<_, _>>();

        let frames =
            instances
                .iter()
                .copied()
                .filter(|(instance, _)| {
                    instance.mir().frame_descriptor().is_some()
                || instance.mir().operations_with_ids().any(|(_, operation)| matches!(
                    operation.kind(), MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                        initializer: bray_ir::MirFrameInitializer::TaskObservation { .. }, ..
                    })
                ))
                })
                .collect::<Vec<_>>();

        if frames.is_empty() {
            return Ok((BTreeMap::new(), BTreeMap::new()));
        }

        let mut roots = BTreeMap::new();

        // Resolve each selected lifecycle operation once, including callbacks retained
        // by observation frames. Construction does not execute those callbacks.
        for (instance, owner) in &frames {
            let selected =
                self.frame_operation_dependencies(instance, owner, target, cancellation)?;

            roots.insert(instance.key(), selected);
        }

        let demanded = bray_base::transitive_dependencies(
            roots
                .values()
                .flat_map(|operations| operations.values().flatten().cloned()),
            |key| {
                let (instance, _) = helpers
                    .get(key)
                    .expect("frame operation retains its concrete lifecycle dependency");

                instance
                    .dependencies()
                    .iter()
                    .filter(|dependency| helpers.contains_key(dependency.instance()))
                    .map(|dependency| dependency.instance().clone())
                    .collect::<Vec<_>>()
            },
        );

        let helpers = demanded.iter().map(|key| helpers[key]).collect::<Vec<_>>();

        let positions = helpers
            .iter()
            .enumerate()
            .map(|(index, (instance, _))| (instance.key(), index))
            .collect::<BTreeMap<_, _>>();

        let mut local = Vec::with_capacity(helpers.len());
        let mut edges = Vec::with_capacity(helpers.len());
        let mut calls = BTreeMap::new();

        for (instance, _) in &helpers {
            cancellation.check()?;

            let mut requirements = ExecutionLaneRequirement::set([]);

            // Lowering already selected every local invocation. Structural helper edges
            // below retain child work, so do not select the lifecycle action again here.
            let invocations = instance.mir().operations().iter().filter_map(|operation| {
                let call = match operation.kind() {
                    MirOperationKind::Call(call)
                    | MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                        initializer: bray_ir::MirFrameInitializer::Callable(call),
                        ..
                    }) => call,
                    _ => return None,
                };

                match call.target() {
                    bray_ir::MirCallTarget::Direct(reference) => Some(reference.instance()),
                    _ => None,
                }
            });

            for callable in invocations {
                let selected =
                    self.lifecycle_invocation_lanes(callable, &mut calls, cancellation)?;

                requirements = ExecutionLaneRequirement::set(
                    requirements.iter().chain(selected.iter()).copied(),
                );
            }

            local.push(requirements);

            // Reuse concrete expansion's structural edges. Deferred bodies have
            // independent frame entries and contribute only invocation contracts.
            edges.push(
                instance
                    .dependencies()
                    .iter()
                    .filter_map(|dependency| positions.get(dependency.instance()).copied())
                    .collect::<Vec<_>>(),
            );
        }

        let requirements = close_requirements(local, &edges);

        let helper_requirements = positions
            .iter()
            .map(|(key, position)| (*key, requirements[*position]))
            .collect::<BTreeMap<_, _>>();

        let mut result = BTreeMap::new();

        for (instance, _) in frames {
            cancellation.check()?;

            if instance.mir().frame_descriptor().is_none() {
                continue;
            }

            let states = self.frame_entry_requirements(
                instance.mir(),
                &roots[instance.key()],
                &helper_requirements,
                &mut calls,
                cancellation,
            )?;

            result.insert(instance.key().clone(), states);
        }

        let helpers = helper_requirements
            .into_iter()
            .map(|(key, lanes)| (key.clone(), lanes))
            .collect();

        Ok((result, helpers))
    }

    fn frame_operation_dependencies(
        &self,
        instance: &CodegenInstance,
        owner: &ConcreteCodegenInstance,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<BTreeMap<MirOperationId, Vec<CodegenInstanceKey>>, CodegenPreparationError> {
        let mut selected = BTreeMap::new();
        let mut references = BTreeMap::new();

        for (id, operation) in instance.mir().operations_with_ids() {
            if matches!(
                operation.kind(),
                MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                    initializer: bray_ir::MirFrameInitializer::Callable(_),
                    ..
                })
            ) {
                selected.insert(id, Vec::new());
                continue;
            }

            let mut dependencies = Vec::new();

            for reference in operation.kind().helper_references() {
                if MirGeneratedLifecycleRole::from_reference(&reference).is_none() {
                    continue;
                }

                let key = match references.entry(reference) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        let concrete = self.concrete_codegen_helper_reference(
                            owner,
                            entry.key(),
                            cancellation,
                        )?;

                        let key = if self.codegen_lifecycle_is_trivial(&concrete, cancellation)? {
                            None
                        } else {
                            Some(
                                self.concrete_codegen_lifecycle(concrete, target)?
                                    .key()
                                    .clone(),
                            )
                        };

                        entry.insert(key)
                    }
                };

                if let Some(key) = key {
                    dependencies.push(key.clone());
                }
            }

            selected.insert(id, dependencies);
        }

        Ok(selected)
    }

    fn frame_entry_requirements(
        &self,
        mir: &MirUnit,
        operations: &BTreeMap<MirOperationId, Vec<CodegenInstanceKey>>,
        helpers: &BTreeMap<&CodegenInstanceKey, &'static [ExecutionLaneRequirement]>,
        calls: &mut BTreeMap<bray_symbols::CallableInstanceData, Vec<ExecutionLaneRequirement>>,
        cancellation: &CancellationToken,
    ) -> Result<Vec<[&'static [ExecutionLaneRequirement]; 3]>, CodegenPreparationError> {
        let frame = mir
            .frame_descriptor()
            .expect("selected frame retains its descriptor");

        let mut local = Vec::with_capacity(mir.blocks().len());

        for block in mir.blocks() {
            let mut lanes = ExecutionLaneRequirement::set([]);

            for id in block.operations() {
                let operation = mir
                    .operation(*id)
                    .expect("checked block owns its operation");

                if matches!(
                    operation.kind(),
                    MirOperationKind::Async(MirAsyncOperation::CreateFrame { .. })
                ) {
                    continue;
                }

                for helper in &operations[id] {
                    lanes = ExecutionLaneRequirement::set(
                        lanes.iter().chain(helpers[helper].iter()).copied(),
                    );
                }

                if let MirOperationKind::Call(call) = operation.kind() {
                    if let Some(behavior) = call.phase_behaviors() {
                        lanes = ExecutionLaneRequirement::set(
                            lanes.iter().copied().chain(
                                bray_lowering::lower_execution_requirements(
                                    self.available_compiler_known_symbols(),
                                    behavior
                                        .invocation()
                                        .execution_requirements()
                                        .iter()
                                        .copied(),
                                ),
                            ),
                        );
                    } else if let bray_ir::MirCallTarget::Direct(reference) = call.target() {
                        // Structural expansion preserves the selected call, rather than
                        // a source invocation's phase summary. Resolve its contract once.
                        let selected = self.lifecycle_invocation_lanes(
                            reference.instance(),
                            calls,
                            cancellation,
                        )?;

                        lanes = ExecutionLaneRequirement::set(
                            lanes.iter().chain(selected.iter()).copied(),
                        );
                    }
                }
            }

            local.push(lanes);
        }

        // Both traversals start from the same small per-block lane sets.
        let ordinary = close_requirements(local.clone(), &callback_edges(mir, false));
        let cancelled = close_requirements(local, &callback_edges(mir, true));

        let states = frame
            .states()
            .iter()
            .map(|state| {
                let resume = ExecutionLaneRequirement::set(
                    ordinary[state.entry().slot() as usize]
                        .iter()
                        .copied()
                        .chain(state.retention().lane_requirements().iter().copied()),
                );

                [
                    resume,
                    cancelled[state.entry().slot() as usize],
                    ordinary[state.retention().broadcast().slot() as usize],
                ]
            })
            .collect();

        Ok(states)
    }

    fn lifecycle_invocation_lanes<'cache>(
        &self,
        callable: bray_symbols::CallableInstanceData,
        calls: &'cache mut BTreeMap<
            bray_symbols::CallableInstanceData,
            Vec<ExecutionLaneRequirement>,
        >,
        cancellation: &CancellationToken,
    ) -> Result<&'cache [ExecutionLaneRequirement], CodegenPreparationError> {
        let entry = match calls.entry(callable) {
            std::collections::btree_map::Entry::Occupied(entry) => return Ok(entry.into_mut()),
            std::collections::btree_map::Entry::Vacant(entry) => entry,
        };

        let binding = self.binding_context(cancellation)?;
        let mut diagnostics = bray_diagnostics::DiagnosticBag::new();

        let signature =
            self.resolve_callable_instance_signature(&binding, callable, &mut diagnostics)?;

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        let signature =
            signature.expect("selected lifecycle invocation retains its checked signature");

        let known = self.available_compiler_known_symbols();

        let requirements = bray_checker::invocation_execution_requirements(
            self.semantic_value_store()?,
            known,
            signature.callable_type(),
        );

        Ok(entry.insert(bray_lowering::lower_execution_requirements(
            known,
            requirements,
        )))
    }
}

fn close_requirements(
    mut local: Vec<&'static [ExecutionLaneRequirement]>,
    edges: &[Vec<usize>],
) -> Vec<&'static [ExecutionLaneRequirement]> {
    let mut callers = vec![Vec::new(); local.len()];

    for (caller, children) in edges.iter().enumerate() {
        for &child in children {
            callers[child].push(caller);
        }
    }

    // There are three lane requirements. Each visits the existing helper graph once,
    // including cycles, without re-expanding types or iterating to a fixed point.
    for requirement in ExecutionLaneRequirement::ALL {
        let roots = local
            .iter()
            .enumerate()
            .filter_map(|(index, own)| own.contains(&requirement).then_some(index));

        let affected =
            bray_base::transitive_dependencies(roots, |index| callers[*index].iter().copied());

        for index in affected {
            local[index] =
                ExecutionLaneRequirement::set(local[index].iter().copied().chain([requirement]));
        }
    }

    local
}

fn callback_edges(mir: &MirUnit, cancelling: bool) -> Vec<Vec<usize>> {
    mir.blocks()
        .iter()
        .map(|block| {
            let mut successors = Vec::new();
            let mut visit = |block: MirBlockId| successors.push(block.slot() as usize);

            match block.terminator().kind() {
                MirTerminatorKind::Suspend { .. } => {}
                MirTerminatorKind::Branch {
                    condition,
                    then_edge,
                    else_edge,
                } if observes_cancellation(mir, condition) => visit(if cancelling {
                    then_edge.target()
                } else {
                    else_edge.target()
                }),
                terminator => terminator.for_each_successor(visit),
            }

            successors
        })
        .collect()
}

fn observes_cancellation(mir: &MirUnit, operand: &MirOperand) -> bool {
    let MirOperand::Value(value) = operand else {
        return false;
    };

    let MirValueOrigin::Operation(operation) = mir
        .value(*value)
        .expect("checked branch condition has its definition")
        .origin()
    else {
        return false;
    };

    matches!(
        mir.operation(operation)
            .expect("checked value has its producer")
            .kind(),
        MirOperationKind::Async(MirAsyncOperation::ObserveFrameCancellation)
    )
}
