use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{CodegenInstanceKey, CodegenStaticInstanceKey, CodegenTarget};
use bray_runtime_interface::ExecutionLaneRequirement;
use bray_symbols::{StaticReferenceSelection, TypeId};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::super::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductValueKind,
};
use super::super::specialization::ConcreteCodegenReachability;
use super::statics::ConcreteStaticRealization;
use crate::fact::CancellationToken;

impl Compilation {
    fn static_cleanup_requires_main_thread(
        &self,
        realization: &ConcreteStaticRealization,
        reachability: &ConcreteCodegenReachability,
    ) -> Result<bool, CodegenPreparationError> {
        let mut pending = BTreeSet::<CodegenInstanceKey>::new();
        let mut visited = BTreeSet::new();

        if let Some(finalization) = &realization.finalization {
            pending.insert(finalization.instance.key().clone());
        }

        if let Some(destroy) = &realization.destroy {
            pending.insert(destroy.key().clone());
        }

        while let Some(key) = pending.pop_first() {
            if !visited.insert(key.clone()) {
                continue;
            }

            let Some(instance) = reachability.graph().instance(&key) else {
                if reachability.graph().is_external(&key) {
                    continue;
                }

                return Err(ProductQueryFailure::missing(
                    ProductQueryContext::Instance(key),
                    ProductDataKind::ConcreteInstance,
                )
                .into());
            };

            if instance.mir().frame_descriptor().is_some_and(|frame| {
                frame.states().iter().any(|state| {
                    state
                        .execution()
                        .lane_requirements()
                        .contains(&ExecutionLaneRequirement::MainThread)
                })
            }) {
                return Ok(true);
            }

            pending.extend(
                instance
                    .dependencies()
                    .iter()
                    .map(|dependency| dependency.instance().clone()),
            );
        }

        Ok(false)
    }

    fn static_lifecycle_providers(
        &self,
        consumer: &ConcreteStaticRealization,
        reachability: &ConcreteCodegenReachability,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<BTreeSet<CodegenStaticInstanceKey>, CodegenPreparationError> {
        let mut pending = BTreeSet::<CodegenInstanceKey>::new();
        let mut visited = BTreeSet::new();
        let mut providers = BTreeSet::new();

        // The worklist owns Arc-backed identities independently of borrowed graph entries.
        pending.insert(consumer.initializer.key().clone());

        if let Some(finalization) = &consumer.finalization {
            pending.insert(finalization.instance.key().clone());
        }

        if let Some(destroy) = &consumer.destroy {
            pending.insert(destroy.key().clone());
        }

        while let Some(key) = pending.pop_first() {
            if visited.contains(&key) {
                continue;
            }

            let Some(instance) = reachability.graph().instance(&key) else {
                if reachability.graph().is_external(&key) {
                    continue;
                }

                return Err(ProductQueryFailure::missing(
                    ProductQueryContext::Instance(key),
                    ProductDataKind::ConcreteInstance,
                )
                .into());
            };

            let owner = reachability.instance(&key).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(key.clone()),
                    ProductDataKind::ReachabilityRealization,
                )
            })?;

            for storage in instance.mir().storages() {
                let Some(reference) =
                    self.codegen_static_reference(storage.kind(), cancellation)?
                else {
                    continue;
                };

                let provider =
                    self.concrete_codegen_static(owner, &reference, target, cancellation)?;

                let StaticReferenceSelection::Closed(provider_instance) = &provider.reference
                else {
                    return Err(ProductQueryFailure::unexpected_kind(
                        ProductQueryContext::StaticReference(provider.reference),
                        ProductValueKind::ClosedStaticReference,
                        ProductValueKind::OpenStaticReference,
                    )
                    .into());
                };

                if consumer
                    .lifecycle_dependencies
                    .contains(&provider_instance.template().declaration())
                {
                    providers.insert(provider.key);
                }
            }

            pending.extend(
                instance
                    .dependencies()
                    .iter()
                    .map(|dependency| dependency.instance().clone()),
            );

            visited.insert(key);
        }

        Ok(providers)
    }

    pub(in crate::compilation::product) fn product_static_host_entries(
        &self,
        reachability: &ConcreteCodegenReachability,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ProductStaticHostEntry>, CodegenPreparationError> {
        // Host graph tables own their Arc-backed static keys independently of reachability.
        let mut realized = BTreeMap::new();

        for instance in reachability.graph().instances() {
            let owner = reachability.instance(instance.key()).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.key().clone()),
                    ProductDataKind::ReachabilityRealization,
                )
            })?;

            for storage in instance.mir().storages() {
                let Some(reference) =
                    self.codegen_static_reference(storage.kind(), cancellation)?
                else {
                    continue;
                };

                let static_instance =
                    self.concrete_codegen_static(owner, &reference, target, cancellation)?;

                realized
                    .entry(static_instance.key.clone())
                    .or_insert(static_instance);
            }
        }

        let mut incoming = realized
            .keys()
            .cloned()
            .map(|key| (key, 0_usize))
            .collect::<BTreeMap<_, _>>();

        let mut outgoing = BTreeMap::<_, Vec<_>>::new();
        let mut dependencies = BTreeMap::<_, Vec<_>>::new();

        for (consumer_key, consumer) in &realized {
            let providers =
                self.static_lifecycle_providers(consumer, reachability, target, cancellation)?;

            for provider in &providers {
                if !realized.contains_key(provider) {
                    return Err(ProductQueryFailure::missing(
                        ProductQueryContext::CodegenStatic(provider.clone()),
                        ProductDataKind::RealizedStatic,
                    )
                    .into());
                }
            }

            for provider in providers {
                outgoing
                    .entry(consumer_key.clone())
                    .or_default()
                    .push(provider.clone());

                dependencies
                    .entry(consumer_key.clone())
                    .or_default()
                    .push(provider.clone());

                let count = incoming.get_mut(&provider).ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::CodegenStatic(provider.clone()),
                        ProductDataKind::StaticDependencyCounter,
                    )
                })?;

                *count = count.checked_add(1).ok_or_else(|| {
                    ProductQueryFailure::StaticDependencyOverflow {
                        static_instance: provider,
                    }
                })?;
            }
        }

        let mut ready = incoming
            .iter()
            .filter_map(|(key, count)| (*count == 0).then_some(key.clone()))
            .collect::<BTreeSet<_>>();

        let mut ordered = Vec::with_capacity(realized.len());

        while let Some(key) = ready.pop_first() {
            let static_instance = realized.get(&key).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CodegenStatic(key.clone()),
                    ProductDataKind::RealizedStatic,
                )
            })?;

            // Returned host entries own their shared identities after graph tables are released.
            ordered.push(ProductStaticHostEntry::new(
                static_instance.key.clone(),
                static_instance.reference.clone(),
                static_instance.ty,
                dependencies.remove(&key).unwrap_or_default(),
                self.static_cleanup_requires_main_thread(static_instance, reachability)?,
            ));

            for provider in outgoing.get(&key).into_iter().flatten() {
                let count = incoming.get_mut(provider).ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::CodegenStatic(provider.clone()),
                        ProductDataKind::StaticDependencyCounter,
                    )
                })?;

                *count = count.checked_sub(1).ok_or_else(|| {
                    ProductQueryFailure::StaticDependencyUnderflow {
                        static_instance: provider.clone(),
                    }
                })?;

                if *count == 0 {
                    ready.insert(provider.clone());
                }
            }
        }

        if ordered.len() != realized.len() {
            let instances = incoming
                .into_iter()
                .filter_map(|(key, count)| (count > 0).then_some(key))
                .collect::<Vec<_>>()
                .into_boxed_slice();

            return Err(ProductQueryFailure::StaticLifecycleCycle { instances }.into());
        }

        Ok(ordered)
    }
}

pub(in crate::compilation::product) struct ProductStaticHostEntry {
    key: CodegenStaticInstanceKey,
    reference: StaticReferenceSelection,
    ty: TypeId,
    dependencies: Vec<CodegenStaticInstanceKey>,
    requires_main_thread_cleanup: bool,
}

impl ProductStaticHostEntry {
    pub(super) fn new(
        key: CodegenStaticInstanceKey,
        reference: StaticReferenceSelection,
        ty: TypeId,
        dependencies: Vec<CodegenStaticInstanceKey>,
        requires_main_thread_cleanup: bool,
    ) -> Self {
        Self {
            key,
            reference,
            ty,
            dependencies,
            requires_main_thread_cleanup,
        }
    }

    pub(in crate::compilation::product) const fn key(&self) -> &CodegenStaticInstanceKey {
        &self.key
    }

    pub(in crate::compilation::product) fn dependencies(&self) -> &[CodegenStaticInstanceKey] {
        &self.dependencies
    }

    pub(in crate::compilation::product) const fn requires_main_thread_cleanup(&self) -> bool {
        self.requires_main_thread_cleanup
    }

    pub(in crate::compilation::product) fn lowering_entry(
        &self,
    ) -> bray_lowering::ExecutableHostStatic {
        // Lowered host MIR owns the Arc-backed static reference after planning returns.
        bray_lowering::ExecutableHostStatic::new(self.reference.clone(), self.ty)
    }
}
