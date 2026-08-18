use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{
    CodegenConstantTermMapping, CodegenLinkage, CodegenMappings, CodegenStaticInstanceKey,
    CodegenStaticStorageMapping, CodegenStaticWitness, CodegenTarget, CodegenTerminatorMapping,
    CodegenUnit, demanded_callable_instances_for_mir,
};
use bray_ir::{MirStorageKind, MirUnit, MirUnitKey};
use bray_runtime_interface::{BinarySymbolName, ExecutableHostContract};
use bray_symbols::{StaticReferenceSelection, TypeId};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::specialization::{
    ConcreteCodegenCallee, ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use super::names::generated_symbol_name;
use super::support::{operation_result_type, signature_types};
use crate::fact::{CancellationToken, FactQueryError};

struct ConcreteStaticRealization {
    key: CodegenStaticInstanceKey,
    symbol: BinarySymbolName,
    initializer: ConcreteCodegenInstance,
    initial_value: bray_symbols::ConstantValueId,
    cleanup: Option<ConcreteCodegenInstance>,
    reference: StaticReferenceSelection,
    ty: TypeId,
    lifecycle_dependencies: Vec<bray_symbols::StaticSymbolId>,
}

pub(in crate::compilation::product) struct ProductStaticHostEntry {
    key: CodegenStaticInstanceKey,
    reference: StaticReferenceSelection,
    ty: TypeId,
    dependencies: Vec<CodegenStaticInstanceKey>,
}

impl ProductStaticHostEntry {
    pub(in crate::compilation::product) const fn key(&self) -> &CodegenStaticInstanceKey {
        &self.key
    }

    pub(in crate::compilation::product) fn dependencies(&self) -> &[CodegenStaticInstanceKey] {
        &self.dependencies
    }

    pub(in crate::compilation::product) fn lowering_entry(
        &self,
    ) -> bray_lowering::ExecutableHostStatic {
        // Lowered host MIR owns the Arc-backed static reference after planning returns.
        bray_lowering::ExecutableHostStatic::new(self.reference.clone(), self.ty)
    }
}

impl Compilation {
    pub(in crate::compilation::product) fn concrete_codegen_dependencies_for_mir(
        &self,
        owner: &ConcreteCodegenInstance,
        mir: &MirUnit,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ConcreteCodegenInstance>, CodegenPreparationError> {
        let mut dependencies = Vec::new();

        for demand in demanded_callable_instances_for_mir(mir) {
            let callee = self.concrete_codegen_callee(owner, &demand, target, cancellation)?;

            if let ConcreteCodegenCallee::Instance(dependency) = callee {
                dependencies.push(dependency);
            }
        }

        for operation in mir.operations() {
            let result_type = operation_result_type(mir, operation);

            for reference in operation.kind().helper_references() {
                if let Some(dependency) = self.concrete_codegen_helper_dependency(
                    owner,
                    operation.kind(),
                    result_type,
                    &reference,
                    target,
                    cancellation,
                )? {
                    dependencies.push(dependency);
                }
            }
        }

        for storage in mir.storages() {
            let MirStorageKind::Static(reference) = storage.kind() else {
                continue;
            };

            let static_realization =
                self.concrete_codegen_static(owner, &reference, target, cancellation)?;

            if static_realization.initializer.key() != owner.key() {
                dependencies.push(static_realization.initializer);
            }

            if let Some(cleanup) = static_realization.cleanup {
                dependencies.push(cleanup);
            }
        }

        dependencies.sort_unstable_by(|left, right| left.key().cmp(right.key()));

        for pair in dependencies.windows(2) {
            if pair[0].key() == pair[1].key() && pair[0] != pair[1] {
                return Err(FactQueryError::InfrastructureFailure.into());
            }
        }

        dependencies.dedup_by(|left, right| left.key() == right.key());

        Ok(dependencies)
    }

    pub(in crate::compilation::product) fn codegen_mappings_for_product(
        &self,
        unit: &CodegenUnit,
        executable_host: Option<&ExecutableHostContract>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        include_debug_locations: bool,
        cancellation: &CancellationToken,
    ) -> Result<CodegenMappings, CodegenPreparationError> {
        let operations = self.codegen_operations(unit, target, reachability, cancellation)?;

        let static_storages =
            self.codegen_static_storages(unit, target, reachability, cancellation)?;

        let mut symbols = self.codegen_symbols(
            unit,
            &operations,
            executable_host,
            target,
            roots,
            reachability,
            cancellation,
        )?;

        let callables = self.codegen_callables(unit, target, reachability, cancellation)?;

        let (constant_terms, terminators) = self.codegen_constant_terms(unit, reachability)?;

        let constants = self.codegen_constants(
            unit,
            constant_terms
                .iter()
                .map(CodegenConstantTermMapping::value)
                .chain(
                    terminators
                        .iter()
                        .flat_map(CodegenTerminatorMapping::constants)
                        .copied(),
                )
                .chain(
                    static_storages
                        .iter()
                        .map(CodegenStaticStorageMapping::initial_value),
                ),
        )?;

        let mut type_mappings = BTreeMap::new();
        let mut instance_type_mappings = Vec::new();

        for instance in unit.instances() {
            let realization = reachability
                .instance(instance.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            self.extend_codegen_types(
                instance.mir().referenced_types(),
                realization.substitution(),
                target,
                cancellation,
                &mut type_mappings,
                Some(realization),
                &mut instance_type_mappings,
            )?;
        }

        let mut demanded = BTreeSet::new();

        demanded.extend(constants.iter().map(|constant| constant.data().ty()));

        for symbol in &symbols {
            demanded.extend(signature_types(symbol.signature()));
        }

        demanded.retain(|ty| !type_mappings.contains_key(ty));

        self.extend_codegen_types(
            demanded,
            None,
            target,
            cancellation,
            &mut type_mappings,
            None,
            &mut instance_type_mappings,
        )?;

        symbols =
            self.classify_codegen_symbols(symbols, target, cancellation, &mut type_mappings)?;

        let types: Vec<_> = type_mappings.into_values().collect();

        symbols.sort_unstable_by(|left, right| left.key().cmp(right.key()));

        let debug_locations = if include_debug_locations {
            self.codegen_debug_locations(unit)?
        } else {
            Vec::new()
        };

        CodegenMappings::try_new_with_static_storages(
            unit,
            target,
            types,
            instance_type_mappings,
            symbols,
            constants,
            constant_terms,
            callables,
            operations,
            static_storages,
            terminators,
            debug_locations,
        )
        .map_err(CodegenPreparationError::InvalidMappings)
    }

    pub(super) fn codegen_static_storages(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenStaticStorageMapping>, CodegenPreparationError> {
        let mut mappings = Vec::new();

        for instance in unit.instances() {
            let owner = reachability
                .instance(instance.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            for (storage, data) in instance.mir().storages_with_ids() {
                let MirStorageKind::Static(reference) = data.kind() else {
                    continue;
                };

                let realization =
                    self.concrete_codegen_static(owner, &reference, target, cancellation)?;

                mappings.push(CodegenStaticStorageMapping::new(
                    instance.key().clone(),
                    storage,
                    data.ty(),
                    realization.key,
                    realization.symbol,
                    realization.initial_value,
                    realization.cleanup.map(|cleanup| cleanup.key().clone()),
                ));
            }
        }

        Ok(mappings)
    }

    fn concrete_codegen_static(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &StaticReferenceSelection,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteStaticRealization, CodegenPreparationError> {
        let (instance, specialization, witnesses) =
            self.concrete_codegen_static_selection(owner, reference, cancellation)?;

        let declaration = instance.template().declaration();
        let template = self.static_instance_template(declaration)?;

        if template.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                template.diagnostics().clone(),
            ));
        }

        let initializer_template = match self.static_initializer_key(declaration)? {
            Some(source) => MirUnitKey::Bound(source),
            None => {
                let binding_context = self.binding_context(cancellation)?;

                binding_context
                    .imported_semantic_address(declaration.into())
                    .map_err(super::super::super::binder::binding_query_error)?
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                MirUnitKey::ImportedExecutable(bray_ir::MirImportedExecutableKey::new(
                    declaration.into(),
                    bray_ir::MirExecutableTemplateId::ROOT,
                ))
            }
        };

        // Initializer and storage identities independently own shared specialization data.
        let initializer = ConcreteCodegenInstance::static_initializer(
            initializer_template,
            declaration,
            instance.substitution().substitution(),
            specialization.clone(),
            &witnesses,
            owner.key().target().clone(),
        );

        let binding_context = self.binding_context(cancellation)?;
        let declaration = self.portable_codegen_symbol_key(&binding_context, declaration.into())?;

        let key = CodegenStaticInstanceKey::new(
            declaration,
            specialization,
            template
                .value()
                .witness_requirements()
                .iter()
                .cloned()
                .zip(witnesses.iter().map(|(identity, _)| identity.clone()))
                .map(|(requirement, implementation)| {
                    CodegenStaticWitness::new(requirement, implementation)
                }),
            owner.key().target().clone(),
            template.value().duration(),
        );

        let symbol = generated_symbol_name(target, CodegenLinkage::LinkOnce, "static", &key)?;

        let ty = self.resolve_codegen_type(
            template.value().declared_type(),
            instance.substitution().substitution(),
            cancellation,
        )?;

        let lifecycle_dependencies = template.value().lifecycle_dependencies().to_vec();

        let cleanup_is_trivial = self.codegen_cleanup_is_trivial(ty, cancellation)?;

        let cleanup = if cleanup_is_trivial {
            None
        } else {
            let cleanup_reference = bray_ir::MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ty,
            };

            Some(self.concrete_codegen_lifecycle(cleanup_reference, target)?)
        };

        let evaluated = self.evaluate_static_initializer(
            &instance,
            template.value().duration(),
            ty,
            bray_checker::ConstantEvaluationLimits::default(),
            cancellation,
        )?;

        if evaluated.diagnostics().has_errors() {
            // The preparation error owns diagnostics after the evaluation result is released.
            return Err(CodegenPreparationError::Diagnostics(
                evaluated.diagnostics().clone(),
            ));
        }

        Ok(ConcreteStaticRealization {
            key,
            symbol,
            initializer,
            initial_value: evaluated.value().value(),
            cleanup,
            reference: StaticReferenceSelection::Closed(instance),
            ty,
            lifecycle_dependencies,
        })
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
            let initializer = reachability
                .graph()
                .instances()
                .iter()
                .find(|instance| instance.key() == consumer.initializer.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let initializer_owner = reachability
                .instance(initializer.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mut providers = BTreeSet::new();

            for storage in initializer.mir().storages() {
                let MirStorageKind::Static(reference) = storage.kind() else {
                    continue;
                };

                let provider = self.concrete_codegen_static(
                    initializer_owner,
                    &reference,
                    target,
                    cancellation,
                )?;

                let StaticReferenceSelection::Closed(provider_instance) = &provider.reference
                else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                if !consumer
                    .lifecycle_dependencies
                    .contains(&provider_instance.template().declaration())
                {
                    continue;
                }

                if !realized.contains_key(&provider.key) {
                    return Err(FactQueryError::InfrastructureFailure.into());
                }

                providers.insert(provider.key);
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
            ordered.push(ProductStaticHostEntry {
                key: static_instance.key.clone(),
                reference: static_instance.reference.clone(),
                ty: static_instance.ty,
                dependencies: dependencies.remove(&key).unwrap_or_default(),
            });

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
