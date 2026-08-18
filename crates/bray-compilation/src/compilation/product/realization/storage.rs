use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{CodegenInstanceKey, CodegenStaticInstanceKey, CodegenTarget};
use bray_ir::MirStorageKind;
use bray_runtime_interface::ExecutableEntryResult;
use bray_symbols::{StaticReferenceSelection, TypeId};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenReachability;
use super::statics::ConcreteStaticRealization;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
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

            let instance = reachability
                .graph()
                .instance(&key)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let owner = reachability
                .instance(&key)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            for storage in instance.mir().storages() {
                let MirStorageKind::Static(reference) = storage.kind() else {
                    continue;
                };

                let provider =
                    self.concrete_codegen_static(owner, &reference, target, cancellation)?;

                let StaticReferenceSelection::Closed(provider_instance) = &provider.reference
                else {
                    return Err(FactQueryError::InfrastructureFailure.into());
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
            let owner = reachability
                .instance(instance.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            for storage in instance.mir().storages() {
                let MirStorageKind::Static(reference) = storage.kind() else {
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
                    return Err(FactQueryError::InfrastructureFailure.into());
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

                let count = incoming
                    .get_mut(&provider)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                *count = count
                    .checked_add(1)
                    .ok_or(FactQueryError::InfrastructureFailure)?;
            }
        }

        let mut ready = incoming
            .iter()
            .filter_map(|(key, count)| (*count == 0).then_some(key.clone()))
            .collect::<BTreeSet<_>>();

        let mut ordered = Vec::with_capacity(realized.len());

        while let Some(key) = ready.pop_first() {
            let static_instance = realized
                .get(&key)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            // Returned host entries own their shared identities after graph tables are released.
            ordered.push(ProductStaticHostEntry::new(
                static_instance.key.clone(),
                static_instance.reference.clone(),
                static_instance.ty,
                dependencies.remove(&key).unwrap_or_default(),
                static_instance
                    .finalization
                    .as_ref()
                    .is_some_and(|finalization| {
                        matches!(finalization.result, ExecutableEntryResult::Fallible { .. })
                    }),
            ));

            for provider in outgoing.get(&key).into_iter().flatten() {
                let count = incoming
                    .get_mut(provider)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                *count = count
                    .checked_sub(1)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                if *count == 0 {
                    ready.insert(provider.clone());
                }
            }
        }

        if ordered.len() != realized.len() {
            return Err(FactQueryError::InfrastructureFailure.into());
        }

        Ok(ordered)
    }
}

pub(in crate::compilation::product) struct ProductStaticHostEntry {
    key: CodegenStaticInstanceKey,
    reference: StaticReferenceSelection,
    ty: TypeId,
    dependencies: Vec<CodegenStaticInstanceKey>,
    transfers_cleanup_incident: bool,
}

impl ProductStaticHostEntry {
    pub(super) fn new(
        key: CodegenStaticInstanceKey,
        reference: StaticReferenceSelection,
        ty: TypeId,
        dependencies: Vec<CodegenStaticInstanceKey>,
        transfers_cleanup_incident: bool,
    ) -> Self {
        Self {
            key,
            reference,
            ty,
            dependencies,
            transfers_cleanup_incident,
        }
    }

    pub(in crate::compilation::product) const fn key(&self) -> &CodegenStaticInstanceKey {
        &self.key
    }

    pub(in crate::compilation::product) fn dependencies(&self) -> &[CodegenStaticInstanceKey] {
        &self.dependencies
    }

    pub(in crate::compilation::product) const fn transfers_cleanup_incident(&self) -> bool {
        self.transfers_cleanup_incident
    }

    pub(in crate::compilation::product) fn lowering_entry(
        &self,
    ) -> bray_lowering::ExecutableHostStatic {
        // Lowered host MIR owns the Arc-backed static reference after planning returns.
        bray_lowering::ExecutableHostStatic::new(self.reference.clone(), self.ty)
    }
}
