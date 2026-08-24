use std::collections::BTreeMap;

use bray_codegen::{
    CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenReachabilityBuilder,
    CodegenTarget,
};
use bray_ir::{MirUnit, MirUnitId, MirUnitKey};

use super::error::NativeProductPlanningError;
use super::super::super::Compilation;
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn codegen_reachability(
        &self,
        roots: impl IntoIterator<Item = ConcreteCodegenInstance>,
        generated_host: Option<(MirUnit, Vec<ConcreteCodegenInstance>)>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        let roots: Vec<_> = roots.into_iter().collect();

        let mut realizations: BTreeMap<_, _> = roots
            .iter()
            .cloned()
            .map(|instance| (instance.key().clone(), instance))
            .collect();

        let mut builder = CodegenReachabilityBuilder::try_new(
            roots.iter().map(|instance| instance.key().clone()),
        )
        .map_err(NativeProductPlanningError::InvalidReachability)?;

        let generated_host = generated_host.map(|(mir, source_roots)| {
            for root in &source_roots {
                realizations.insert(root.key().clone(), root.clone());
            }

            (CodegenInstanceKey::non_generic(&mir), mir, source_roots)
        });

        loop {
            let frontier = builder.take_frontier();

            if frontier.is_empty() {
                break;
            }

            for key in frontier.iter() {
                cancellation.check()?;

                let realization = realizations
                    .get(key)
                    .cloned()
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                if matches!(
                    key.template(),
                    MirUnitKey::ExternalCallable(_) | MirUnitKey::ExternalRuntimeDefault(_)
                ) {
                    builder
                        .push_external(key.clone())
                        .map_err(NativeProductPlanningError::InvalidReachability)?;

                    continue;
                }

                let mir = if let Some((host_key, host_mir, _)) = &generated_host
                    && key == host_key
                {
                    host_mir.clone()
                } else {
                    match realization.generated_lifecycle_reference() {
                        Some(reference) => self.codegen_generated_lifecycle_mir(
                            key,
                            reference,
                            MirUnitId::new(0),
                            cancellation,
                        ),
                        None => {
                            self.codegen_mir_for_plan(key, MirUnitId::new(0), None, cancellation)
                        }
                    }?
                };

                let mut concrete_dependencies = self.concrete_codegen_dependencies_for_mir(
                    &realization,
                    &mir,
                    target,
                    cancellation,
                )?;

                if let Some((host_key, _, source_roots)) = &generated_host
                    && key == host_key
                {
                    for root in source_roots {
                        if !concrete_dependencies
                            .iter()
                            .any(|dependency| dependency.key() == root.key())
                        {
                            concrete_dependencies.push(root.clone());
                        }
                    }
                }

                concrete_dependencies.sort_unstable_by(|left, right| left.key().cmp(right.key()));
                concrete_dependencies.dedup_by(|left, right| left.key() == right.key());

                let dependencies = concrete_dependencies
                    .iter()
                    .map(|dependency| {
                        CodegenInstanceDependency::definition(dependency.key().clone())
                    })
                    .collect::<Vec<_>>();

                for dependency in concrete_dependencies {
                    match realizations.entry(dependency.key().clone()) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(dependency);
                        }
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            if entry.get() != &dependency {
                                return Err(FactQueryError::InfrastructureFailure.into());
                            }
                        }
                    }
                }

                let instance = CodegenInstance::try_new(key.clone(), mir, dependencies)
                    .map_err(NativeProductPlanningError::InvalidCodegenInstance)?;

                builder
                    .push_instance(instance)
                    .map_err(NativeProductPlanningError::InvalidReachability)?;
            }
        }

        let graph = builder
            .finish()
            .map_err(NativeProductPlanningError::InvalidReachability)?;

        Ok(ConcreteCodegenReachability::new(graph, realizations))
    }

    #[cfg(test)]
    pub(in crate::compilation) fn imported_codegen_instance_count_for_test(
        &self,
    ) -> Result<usize, NativeProductPlanningError> {
        let target = self
            .selected_target()
            .target()
            .codegen_target()
            .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

        let semantic = self.product_semantics()?;

        let roots =
            self.product_root_instances(semantic.value(), None, &target, &self.state.cancellation)?;

        let reachability =
            self.codegen_reachability(roots, None, &target, &self.state.cancellation)?;

        Ok(reachability
            .graph()
            .instances()
            .iter()
            .filter(|instance| {
                matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
            })
            .count())
    }
}
