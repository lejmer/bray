use std::collections::BTreeMap;

use bray_codegen::{
    CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenReachabilityBuilder,
    CodegenTarget,
};
use bray_ir::{MirUnit, MirUnitId, MirUnitKey};

use super::super::super::Compilation;
use super::super::error::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use super::error::{NativeProductPlanningError, native_batch_error};
use crate::fact::{BatchWork, CancellationToken, FactQueryError};

enum ReachabilityEvaluation {
    External,
    Instance(CodegenInstance),
}

impl Compilation {
    pub(super) fn codegen_reachability(
        &self,
        roots: impl IntoIterator<Item = ConcreteCodegenInstance>,
        generated_host: Option<(MirUnit, Vec<ConcreteCodegenInstance>)>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        let roots: Vec<_> = roots.into_iter().collect();

        let generated_host = generated_host
            .map(|(mir, source_roots)| (CodegenInstanceKey::non_generic(&mir), mir, source_roots));

        let completed = self
            .state
            .fact_runtime
            .complete_batch(roots.clone(), cancellation, |realization| {
                self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeReachability,
                    || {
                        cancellation.check()?;

                        let key = realization.key();

                        if matches!(
                            key.template(),
                            MirUnitKey::ExternalCallable(_) | MirUnitKey::ExternalRuntimeDefault(_)
                        ) {
                            return Ok(BatchWork::leaf(ReachabilityEvaluation::External));
                        }

                        let mir = if let Some((host_key, host_mir, _)) = &generated_host
                            && key == host_key
                        {
                            host_mir.clone()
                        } else if matches!(key.template(), MirUnitKey::CompilerProvidedCallable(_))
                        {
                            self.codegen_compiler_provided_mir(
                                &realization,
                                MirUnitId::new(0),
                                cancellation,
                            )?
                        } else {
                            match realization.generated_lifecycle_reference() {
                                Some(reference) => self.codegen_generated_lifecycle_mir(
                                    key,
                                    reference,
                                    MirUnitId::new(0),
                                    cancellation,
                                ),
                                None => self.codegen_mir_for_plan(
                                    key,
                                    MirUnitId::new(0),
                                    None,
                                    cancellation,
                                ),
                            }?
                        };

                        let mir =
                            self.specialize_codegen_lifecycle_mir(&realization, mir, cancellation)?;

                        let mut concrete_dependencies = self
                            .concrete_codegen_dependencies_for_mir(
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

                        concrete_dependencies.sort_unstable();
                        concrete_dependencies.dedup();

                        let dependencies = concrete_dependencies
                            .iter()
                            .map(|dependency| {
                                CodegenInstanceDependency::definition(dependency.key().clone())
                            })
                            .collect::<Vec<_>>();

                        let instance = CodegenInstance::try_new(key.clone(), mir, dependencies)
                            .map_err(NativeProductPlanningError::InvalidCodegenInstance)?;

                        Ok(BatchWork::new(
                            ReachabilityEvaluation::Instance(instance),
                            concrete_dependencies,
                        ))
                    },
                )
            })
            .map_err(native_batch_error)?;

        let mut realizations = BTreeMap::new();
        let mut evaluations = BTreeMap::new();

        for (realization, evaluation) in completed {
            let key = realization.key().clone();

            match realizations.entry(key.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(realization);
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    if entry.get() != &realization {
                        return Err(FactQueryError::from(ProductQueryFailure::Conflict {
                            context: ProductQueryContext::Instance(key),
                            data: ProductDataKind::ReachabilityRealization,
                        })
                        .into());
                    }
                }
            }

            if evaluations.insert(key.clone(), evaluation).is_some() {
                return Err(FactQueryError::from(ProductQueryFailure::Conflict {
                    context: ProductQueryContext::Instance(key),
                    data: ProductDataKind::ReachabilityEvaluation,
                })
                .into());
            }
        }

        let mut builder = CodegenReachabilityBuilder::try_new(
            roots.iter().map(|instance| instance.key().clone()),
        )
        .map_err(NativeProductPlanningError::InvalidReachability)?;

        loop {
            let frontier = builder.take_frontier();

            if frontier.is_empty() {
                break;
            }

            for key in frontier.iter() {
                let evaluation = evaluations.remove(key).ok_or_else(|| {
                    FactQueryError::from(ProductQueryFailure::missing(
                        ProductQueryContext::Instance(key.clone()),
                        ProductDataKind::ReachabilityEvaluation,
                    ))
                })?;

                match evaluation {
                    ReachabilityEvaluation::External => builder.push_external(key.clone()),
                    ReachabilityEvaluation::Instance(instance) => builder.push_instance(instance),
                }
                .map_err(NativeProductPlanningError::InvalidReachability)?;
            }
        }

        if let Some(key) = evaluations.keys().next() {
            return Err(FactQueryError::from(ProductQueryFailure::Conflict {
                context: ProductQueryContext::Instance(key.clone()),
                data: ProductDataKind::ReachabilityEvaluation,
            })
            .into());
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
