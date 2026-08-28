use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{
    CodegenConstantTermMapping, CodegenLinkage, CodegenMappings, CodegenStaticFinalization,
    CodegenStaticIncidentMemory, CodegenStaticInstanceKey, CodegenStaticRelocation,
    CodegenStaticStorageMapping, CodegenStaticWitness, CodegenTarget, CodegenTerminatorMapping,
    CodegenUnit, demanded_callable_instances_for_mir,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirStorageKind, MirUnit, MirUnitKey};
use bray_runtime_interface::{BinarySymbolName, ExecutableEntryResult, ExecutableHostContract};
use bray_symbols::{
    CallableExecution, GenericArgument, StaticReferenceSelection, TypeAssociatedLifecycleSlot,
    TypeData, TypeId,
};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::specialization::{
    ConcreteCodegenCallee, ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use super::names::generated_symbol_name;
use super::support::{operation_result_type, signature_types};
use crate::fact::{CancellationToken, FactQueryError};

pub(super) struct ConcreteStaticRealization {
    pub(super) key: CodegenStaticInstanceKey,
    symbol: BinarySymbolName,
    pub(super) initializer: ConcreteCodegenInstance,
    initial_value: bray_symbols::ConstantValueId,
    relocations: Vec<CodegenStaticRelocation>,
    pub(super) finalization: Option<ConcreteStaticFinalization>,
    pub(super) destroy: Option<ConcreteCodegenInstance>,
    pub(super) reference: StaticReferenceSelection,
    pub(super) ty: TypeId,
    pub(super) lifecycle_dependencies: Vec<bray_symbols::StaticSymbolId>,
}

pub(super) struct ConcreteStaticFinalization {
    execution: CallableExecution,
    pub(super) instance: ConcreteCodegenInstance,
    pub(super) result: ExecutableEntryResult,
    error_type_identity: Option<[u8; 32]>,
    source: Option<bray_ir::MirSourceAnchor>,
    incident_cleanup: Option<ConcreteCodegenInstance>,
    incident_memory: Option<ConcreteStaticIncidentMemory>,
}

struct ConcreteStaticIncidentMemory {
    allocation: ConcreteCodegenInstance,
    deallocation: ConcreteCodegenInstance,
}

impl Compilation {
    fn static_finalizer_result(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<(ExecutableEntryResult, Option<[u8; 32]>), CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        match super::super::super::foreign::compiler_known_representation(self, *definition) {
            Some(RepresentationRole::Unit) => Ok((ExecutableEntryResult::Unit, None)),
            Some(RepresentationRole::Result) => {
                let representation = self
                    .available_compiler_known_symbols()
                    .result_representation()
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let substitution = values
                    .generic_substitution_data(*substitution)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let [success, error] = substitution.bindings() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                let GenericArgument::Type(success) = success.argument() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                let success = values
                    .type_data(success)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let TypeData::Named { definition, .. } = success.as_ref() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                if super::super::super::foreign::compiler_known_representation(self, *definition)
                    != Some(RepresentationRole::Unit)
                {
                    return Err(CodegenPreparationError::from(
                        FactQueryError::InfrastructureFailure,
                    ));
                }

                let GenericArgument::Type(error) = error.argument() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                let binding_context = self.binding_context(cancellation)?;

                let identity =
                    super::super::structural_type_identity(values, &binding_context, error)?;

                Ok((
                    ExecutableEntryResult::Fallible {
                        ty,
                        error,
                        success_variant: representation.success_variant(),
                    },
                    Some(identity),
                ))
            }
            _ => Err(FactQueryError::InfrastructureFailure.into()),
        }
    }

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
            let Some(reference) = self.codegen_static_reference(storage.kind(), cancellation)?
            else {
                continue;
            };

            let static_realization =
                self.concrete_codegen_static(owner, &reference, target, cancellation)?;

            if static_realization.initializer.key() != owner.key() {
                dependencies.push(static_realization.initializer);
            }

            if let Some(finalization) = static_realization.finalization {
                dependencies.push(finalization.instance);

                if let Some(cleanup) = finalization.incident_cleanup {
                    dependencies.push(cleanup);
                }

                if let Some(memory) = finalization.incident_memory {
                    dependencies.extend([memory.allocation, memory.deallocation]);
                }
            }

            if let Some(destroy) = static_realization.destroy {
                dependencies.push(destroy);
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
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        include_debug_locations: bool,
        cancellation: &CancellationToken,
    ) -> Result<CodegenMappings, CodegenPreparationError> {
        let operations = self.codegen_operations(unit, target, reachability, cancellation)?;

        let static_storages =
            self.codegen_static_storages(unit, target, reachability, cancellation)?;

        let native_storages =
            self.codegen_native_static_storages(unit, reachability, cancellation)?;

        let mut symbols = self.codegen_symbols(
            unit,
            &operations,
            executable_host,
            platform_overrides,
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

        demanded.extend(
            native_storages
                .iter()
                .map(bray_codegen::CodegenNativeStaticMapping::pointee_type),
        );

        for symbol in &symbols {
            demanded.extend(signature_types(symbol.signature()));
        }

        for storage in &static_storages {
            let Some(finalization) = storage.finalization() else {
                continue;
            };

            if let ExecutableEntryResult::Fallible { ty, error, .. } = finalization.result() {
                demanded.extend([ty, error]);
            }
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

        CodegenMappings::try_new_with_storage_mappings(
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
            native_storages,
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
                let Some(reference) = self.codegen_static_reference(data.kind(), cancellation)?
                else {
                    continue;
                };

                let realization =
                    self.concrete_codegen_static(owner, &reference, target, cancellation)?;

                let native_binding = self
                    .optional_native_static_contract(&reference, cancellation)?
                    .filter(|contract| {
                        contract.direction == bray_symbols::ForeignCallableDirection::Export
                    })
                    .map(|contract| contract.symbol.binding());

                let defines_storage = unit
                    .instances()
                    .iter()
                    .any(|candidate| candidate.key() == realization.initializer.key());

                mappings.push(CodegenStaticStorageMapping::new(
                    instance.key().clone(),
                    storage,
                    match data.kind() {
                        MirStorageKind::NativeStatic(_) => realization.ty,
                        _ => data.ty(),
                    },
                    realization.key,
                    realization.symbol,
                    native_binding,
                    defines_storage,
                    realization.initial_value,
                    realization.relocations,
                    realization.finalization.map(|finalization| {
                        CodegenStaticFinalization::new(
                            finalization.execution,
                            finalization.instance.key().clone(),
                            finalization.result,
                            finalization.error_type_identity,
                            finalization.source,
                            finalization
                                .incident_cleanup
                                .map(|cleanup| cleanup.key().clone()),
                            finalization.incident_memory.map(|memory| {
                                CodegenStaticIncidentMemory::new(
                                    memory.allocation.key().clone(),
                                    memory.deallocation.key().clone(),
                                )
                            }),
                        )
                    }),
                    realization.destroy.map(|destroy| destroy.key().clone()),
                ));
            }
        }

        Ok(mappings)
    }

    pub(super) fn concrete_codegen_static(
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

        let symbol = match self.optional_native_static_contract(reference, cancellation)? {
            Some(native) if native.direction == bray_symbols::ForeignCallableDirection::Export => {
                native
                    .symbol
                    .identity()
                    .name()
                    .and_then(BinarySymbolName::try_new)
                    .ok_or(CodegenPreparationError::InvalidSymbolName)?
            }
            _ => generated_symbol_name(target, CodegenLinkage::LinkOnce, "static", &key)?,
        };

        let ty = self.resolve_codegen_type(
            template.value().declared_type(),
            instance.substitution().substitution(),
            cancellation,
        )?;

        let lifecycle_dependencies = template.value().lifecycle_dependencies().to_vec();

        let finalization = (!self.codegen_lifecycle_is_trivial(
            &bray_ir::MirHelperReference::Finalize(ty),
            cancellation,
        )?)
        .then(|| {
            let callable =
                self.lifecycle_callable(ty, TypeAssociatedLifecycleSlot::Finalizer, cancellation)?;

            let source = match callable {
                Some((callable, ..)) => self
                    .callable_body_key(callable.instance().definition())?
                    .map(|key| {
                        bray_ir::MirSourceAnchor::source(bray_bound_tree::BoundNodeOrigin::source(
                            key.source(),
                        ))
                    }),
                None => None,
            };

            let (execution, result, error_type_identity) = callable.map_or_else(
                || {
                    Ok((
                        CallableExecution::Synchronous,
                        ExecutableEntryResult::Unit,
                        None,
                    ))
                },
                |(_, _, result, execution)| {
                    self.static_finalizer_result(result, cancellation)
                        .map(|(result, identity)| (execution, result, identity))
                },
            )?;

            let (incident_cleanup, incident_memory) = match result {
                ExecutableEntryResult::Fallible { error, .. } => {
                    let cleanup = self.concrete_codegen_lifecycle(
                        bray_ir::MirHelperReference::Cleanup {
                            phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                            ty: error,
                        },
                        target,
                    )?;

                    let memory = ConcreteStaticIncidentMemory {
                        allocation: self.concrete_standard_library_helper(
                            bray_ir::MirStandardLibraryHelper::MemoryAllocate,
                            target,
                            cancellation,
                        )?,
                        deallocation: self.concrete_standard_library_helper(
                            bray_ir::MirStandardLibraryHelper::MemoryDeallocate,
                            target,
                            cancellation,
                        )?,
                    };

                    (Some(cleanup), Some(memory))
                }
                ExecutableEntryResult::Unit => (None, None),
                ExecutableEntryResult::I32 => {
                    return Err(CodegenPreparationError::from(
                        FactQueryError::InfrastructureFailure,
                    ));
                }
            };

            Ok(ConcreteStaticFinalization {
                execution,
                instance: self.concrete_codegen_lifecycle(
                    bray_ir::MirHelperReference::StaticFinalize(ty),
                    target,
                )?,
                result,
                error_type_identity,
                source,
                incident_cleanup,
                incident_memory,
            })
        })
        .transpose()?;

        let destroy = (!self.codegen_lifecycle_is_trivial(
            &bray_ir::MirHelperReference::Destroy(ty),
            cancellation,
        )?)
        .then(|| self.concrete_codegen_lifecycle(bray_ir::MirHelperReference::Destroy(ty), target))
        .transpose()?;

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

        let relocations = self.codegen_static_relocations(
            &initializer,
            evaluated.value().value(),
            target,
            cancellation,
        )?;

        Ok(ConcreteStaticRealization {
            key,
            symbol,
            initializer,
            initial_value: evaluated.value().value(),
            relocations,
            finalization,
            destroy,
            reference: StaticReferenceSelection::Closed(instance),
            ty,
            lifecycle_dependencies,
        })
    }

    fn codegen_static_relocations(
        &self,
        owner: &ConcreteCodegenInstance,
        root: bray_symbols::ConstantValueId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenStaticRelocation>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        let mut relocations = BTreeMap::new();

        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }

            let data = values
                .constant_value_data(value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            match data.kind() {
                bray_symbols::ConstantValueKind::StaticAddress(reference) => {
                    let realization =
                        self.concrete_codegen_static(owner, reference, target, cancellation)?;

                    let relocation = CodegenStaticRelocation::new(
                        value,
                        realization.key,
                        realization.symbol,
                        realization.ty,
                    );

                    if relocations.insert(value, relocation).is_some() {
                        return Err(FactQueryError::InfrastructureFailure.into());
                    }
                }
                bray_symbols::ConstantValueKind::NullablePresent(child) => pending.push(*child),
                bray_symbols::ConstantValueKind::Tuple(children)
                | bray_symbols::ConstantValueKind::Array(children) => {
                    pending.extend(children.iter().copied());
                }
                bray_symbols::ConstantValueKind::Product(fields) => {
                    pending.extend(fields.iter().map(|field| *field.value()));
                }
                bray_symbols::ConstantValueKind::Union { fields, .. } => {
                    pending.extend(fields.iter().map(|field| *field.value()));
                }
                bray_symbols::ConstantValueKind::Error
                | bray_symbols::ConstantValueKind::Boolean(_)
                | bray_symbols::ConstantValueKind::Character(_)
                | bray_symbols::ConstantValueKind::Integer(_)
                | bray_symbols::ConstantValueKind::Real(_)
                | bray_symbols::ConstantValueKind::Complex { .. }
                | bray_symbols::ConstantValueKind::String(_)
                | bray_symbols::ConstantValueKind::Unit
                | bray_symbols::ConstantValueKind::NullableAbsent => {}
            }
        }

        Ok(relocations.into_values().collect())
    }
}
