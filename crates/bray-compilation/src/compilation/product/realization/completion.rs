use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{CodegenInstance, CodegenInstanceKey, CodegenTarget};
use bray_ir::{MirCleanupPhase, MirHelperReference, MirUnitId};
use bray_runtime_interface::ExecutionLaneRequirement;

use super::super::specialization::ConcreteCodegenInstance;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product) fn completion_execution_requirements<'a>(
        &self,
        instances: impl IntoIterator<Item = (&'a CodegenInstance, &'a ConcreteCodegenInstance)>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<BTreeMap<CodegenInstanceKey, Vec<ExecutionLaneRequirement>>, CodegenPreparationError>
    {
        let instances = instances.into_iter().collect::<Vec<_>>();
        let mut roots = BTreeMap::new();

        for (instance, owner) in &instances {
            let Some(frame) = instance
                .mir()
                .frame_descriptor()
                .filter(|frame| frame.completion_state().is_some())
            else {
                continue;
            };

            let reference = self.concrete_codegen_helper_reference(
                owner,
                &MirHelperReference::Cleanup {
                    phase: MirCleanupPhase::LifecycleResolution,
                    ty: frame.result_type(),
                },
                cancellation,
            )?;

            if !self.codegen_lifecycle_is_trivial(&reference, cancellation)? {
                roots.insert(
                    instance.key().clone(),
                    self.concrete_codegen_lifecycle(reference, target)?
                        .key()
                        .clone(),
                );
            }
        }

        if roots.is_empty() {
            return Ok(BTreeMap::new());
        }

        let helpers = instances
            .into_iter()
            .filter(|(_, owner)| owner.generated_lifecycle_reference().is_some())
            .map(|(instance, owner)| (instance.key(), (instance, owner)))
            .collect::<BTreeMap<_, _>>();

        let demanded = bray_base::transitive_dependencies(roots.values().cloned(), |key| {
            let (instance, _) = helpers
                .get(key)
                .expect("completed result disposal retains its concrete lifecycle dependency");

            instance
                .dependencies()
                .iter()
                .filter(|dependency| helpers.contains_key(dependency.instance()))
                .map(|dependency| dependency.instance().clone())
                .collect::<Vec<_>>()
        });

        let helpers = demanded.iter().map(|key| helpers[key]).collect::<Vec<_>>();

        let positions = helpers
            .iter()
            .enumerate()
            .map(|(index, (instance, _))| (instance.key(), index))
            .collect::<BTreeMap<_, _>>();

        let mut local = Vec::with_capacity(helpers.len());
        let mut edges = Vec::with_capacity(helpers.len());
        let mut calls = BTreeMap::new();

        for (instance, owner) in &helpers {
            cancellation.check()?;

            let lowered = self.codegen_generated_lifecycle(
                instance.key(),
                owner
                    .generated_lifecycle_reference()
                    .expect("selected lifecycle retains its role"),
                MirUnitId::new(0),
                cancellation,
            )?;

            let mut requirements = BTreeSet::new();

            for call in lowered.local_calls() {
                let selected = match calls.entry(call.callable) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(self.lifecycle_invocation_lanes(call.callable, cancellation)?)
                    }
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                };

                requirements.extend(selected.iter().copied());
            }

            local.push(requirements);

            // Structural children reuse concrete expansion's dependency edges. Source calls
            // contribute checked invocation contracts. Deferred bodies have their own frames.
            edges.push(
                instance
                    .dependencies()
                    .iter()
                    .filter_map(|dependency| positions.get(dependency.instance()).copied())
                    .collect::<Vec<_>>(),
            );
        }

        let requirements = close_requirements(local, &edges);

        Ok(roots
            .into_iter()
            .map(|(frame, helper)| {
                (
                    frame,
                    requirements[positions[&helper]].iter().copied().collect(),
                )
            })
            .collect())
    }

    fn lifecycle_invocation_lanes(
        &self,
        callable: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ExecutionLaneRequirement>, CodegenPreparationError> {
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

        Ok(bray_lowering::lower_execution_requirements(
            known,
            requirements,
        ))
    }
}

fn close_requirements(
    mut local: Vec<BTreeSet<ExecutionLaneRequirement>>,
    edges: &[Vec<usize>],
) -> Vec<BTreeSet<ExecutionLaneRequirement>> {
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
            local[index].insert(requirement);
        }
    }

    local
}
