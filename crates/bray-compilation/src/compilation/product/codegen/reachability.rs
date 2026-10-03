use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;
use std::sync::Arc;

use bray_base::StableDigestHasher;
use bray_codegen::{
    CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenOptions,
    CodegenReachabilityBuilder, CodegenTarget, OptimizationLevel,
};
use bray_ir::{MirUnit, MirUnitId, MirUnitKey};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::error::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use super::error::{NativeProductPlanningError, native_batch_error};
use super::reuse::SelectedNativeUnit;
use super::{ConcreteCodegenDemand, ConcreteCodegenRoot, NativeDemand};
use crate::fact::{
    BatchWork, CancellationToken, CompilationFactKey, FactQueryError, OptimizedMirQueryKey,
};

enum ReachabilityEvaluation {
    External(Option<SelectedNativeUnit>),
    Instance {
        instance: CodegenInstance,
        demands: Vec<ConcreteCodegenDemand>,
    },
}

fn finish_codegen_reachability(
    mut builder: CodegenReachabilityBuilder,
    previous: Option<ConcreteCodegenReachability>,
    roots: &[ConcreteCodegenRoot],
    completed: Vec<(ConcreteCodegenInstance, ReachabilityEvaluation)>,
) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
    let (mut realizations, mut demands, mut selected_native) = match previous {
        Some(previous) => {
            let (_, realizations, previous_demands, selected_native) = previous.into_parts();

            let demands = previous_demands
                .iter()
                .filter(|demand| demand.predecessor().is_some())
                .cloned()
                .collect::<BTreeSet<_>>();

            (realizations, demands, selected_native)
        }
        None => (BTreeMap::new(), BTreeSet::new(), BTreeMap::new()),
    };

    let mut evaluations = BTreeMap::new();

    demands.extend(
        roots
            .iter()
            .map(|root| NativeDemand::root(root.key().clone(), root.reason())),
    );

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

        if let ReachabilityEvaluation::Instance {
            demands: dependencies,
            ..
        } = &evaluation
        {
            demands.extend(dependencies.iter().map(|dependency| {
                NativeDemand::dependency(key.clone(), dependency.key().clone(), dependency.reason())
            }));
        }

        if evaluations.insert(key.clone(), evaluation).is_some() {
            return Err(FactQueryError::from(ProductQueryFailure::Conflict {
                context: ProductQueryContext::Instance(key),
                data: ProductDataKind::ReachabilityEvaluation,
            })
            .into());
        }
    }

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
                ReachabilityEvaluation::External(selected) => {
                    if let Some(selected) = selected {
                        selected_native.insert(key.clone(), selected);
                    }

                    builder.push_external(key.clone())
                }
                ReachabilityEvaluation::Instance { instance, .. } => {
                    builder.push_instance(instance)
                }
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

    Ok(ConcreteCodegenReachability::new(
        graph,
        realizations,
        demands.into_iter().collect::<Vec<_>>(),
        selected_native,
    ))
}

impl Compilation {
    pub(super) fn optimized_mir_for_plan(
        &self,
        realization: &ConcreteCodegenInstance,
        raw: &MirUnit,
        options: CodegenOptions,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let mut hasher = StableDigestHasher::new();

        raw.hash(&mut hasher);

        let key = OptimizedMirQueryKey::new(
            realization.key().clone(),
            raw.unit(),
            options,
            hasher.finalize(),
        );

        let cell = self.state.optimized_mir.cell(key.clone())?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::OptimizedMir(Arc::new(key)),
            cancellation,
            || {
                let optimized = match options.optimization() {
                    OptimizationLevel::None => Ok(raw.clone()),
                    OptimizationLevel::Basic => {
                        self.simplify_concrete_mir(realization, raw, cancellation)
                    }
                    OptimizationLevel::Full => {
                        let basic = options.with_optimization(OptimizationLevel::Basic);

                        self.optimized_mir_for_plan(realization, raw, basic, target, cancellation)
                            .and_then(|base| {
                                self.inline_concrete_mir(
                                    realization,
                                    &base,
                                    options,
                                    target,
                                    cancellation,
                                )
                                .and_then(
                                    |inlined| match inlined {
                                        Some(inlined) => self.simplify_concrete_mir(
                                            realization,
                                            &inlined,
                                            cancellation,
                                        ),
                                        None => Ok(base),
                                    },
                                )
                            })
                    }
                };

                Ok(optimized.map(Arc::new))
            },
        )?;

        result
            .as_ref()
            .map(|mir| mir.as_ref().clone())
            .map_err(Clone::clone)
    }

    pub(super) fn codegen_reachability(
        &self,
        roots: impl IntoIterator<Item = ConcreteCodegenRoot>,
        generated_host: Option<(MirUnit, Vec<ConcreteCodegenRoot>)>,
        target: &CodegenTarget,
        options: CodegenOptions,
        allow_bitcode: bool,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        self.codegen_reachability_from(
            roots,
            generated_host,
            None,
            target,
            options,
            allow_bitcode,
            cancellation,
        )
    }

    pub(super) fn extend_codegen_reachability(
        &self,
        previous: ConcreteCodegenReachability,
        roots: impl IntoIterator<Item = ConcreteCodegenRoot>,
        generated_host: (MirUnit, Vec<ConcreteCodegenRoot>),
        target: &CodegenTarget,
        options: CodegenOptions,
        allow_bitcode: bool,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        self.codegen_reachability_from(
            roots,
            Some(generated_host),
            Some(previous),
            target,
            options,
            allow_bitcode,
            cancellation,
        )
    }

    fn codegen_reachability_from(
        &self,
        roots: impl IntoIterator<Item = ConcreteCodegenRoot>,
        generated_host: Option<(MirUnit, Vec<ConcreteCodegenRoot>)>,
        previous: Option<ConcreteCodegenReachability>,
        target: &CodegenTarget,
        options: CodegenOptions,
        allow_bitcode: bool,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        let roots: Vec<_> = roots.into_iter().collect();

        let root_instances = roots
            .iter()
            .map(ConcreteCodegenRoot::instance)
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let root_keys = root_instances
            .iter()
            .map(ConcreteCodegenInstance::key)
            .cloned()
            .collect::<Vec<_>>();

        let generated_host = generated_host
            .map(|(mir, source_roots)| (CodegenInstanceKey::non_generic(&mir), mir, source_roots));

        let builder = match previous.as_ref() {
            Some(previous) => CodegenReachabilityBuilder::try_extend(previous.graph(), root_keys),
            None => CodegenReachabilityBuilder::try_new(root_keys),
        }
        .map_err(NativeProductPlanningError::InvalidReachability)?;

        let completed = self
            .state
            .fact_runtime
            .complete_batch(root_instances, cancellation, |realization| {
                self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeReachability,
                    || {
                        cancellation.check()?;

                        let key = realization.key();

                        if matches!(
                            key.template(),
                            MirUnitKey::ExternalCallable(_) | MirUnitKey::ExternalRuntimeDefault(_)
                        ) {
                            return Ok(BatchWork::leaf(ReachabilityEvaluation::External(None)));
                        }

                        if let Some(selected) = self.selected_imported_native_unit(
                            key,
                            options,
                            allow_bitcode,
                            cancellation,
                        )? {
                            return Ok(BatchWork::leaf(ReachabilityEvaluation::External(Some(
                                selected,
                            ))));
                        }

                        let mir = if let Some((host_key, host_mir, _)) = &generated_host
                            && key == host_key
                        {
                            host_mir.clone()
                        } else if matches!(key.template(), MirUnitKey::CompilerProvidedCallable(_))
                        {
                            self.codegen_heap_method_mir(
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

                        let mir = self.optimized_mir_for_plan(
                            &realization,
                            &mir,
                            options,
                            target,
                            cancellation,
                        )?;

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
                                concrete_dependencies.push(ConcreteCodegenDemand::definition(
                                    root.instance().clone(),
                                    root.reason(),
                                ));
                            }
                        }

                        concrete_dependencies.sort_unstable();
                        concrete_dependencies.dedup();

                        let dependencies = concrete_dependencies
                            .iter()
                            .map(|dependency| {
                                CodegenInstanceDependency::new(
                                    dependency.scheduling(),
                                    dependency.key().clone(),
                                )
                            })
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>();

                        let instance = CodegenInstance::try_new(key.clone(), mir, dependencies)
                            .map_err(NativeProductPlanningError::InvalidCodegenInstance)?;

                        let scheduled = concrete_dependencies
                            .iter()
                            .filter(|dependency| {
                                previous.as_ref().is_none_or(|previous| {
                                    previous.instance(dependency.key()).is_none()
                                })
                            })
                            .map(ConcreteCodegenDemand::instance)
                            .cloned()
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>();

                        Ok(BatchWork::new(
                            ReachabilityEvaluation::Instance {
                                instance,
                                demands: concrete_dependencies,
                            },
                            scheduled,
                        ))
                    },
                )
            })
            .map_err(native_batch_error)?;

        finish_codegen_reachability(builder, previous, &roots, completed)
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

        let reachability = self.codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &self.state.cancellation,
        )?;

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
