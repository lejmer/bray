// rust-style: allow(module-too-large, reason = "the mapping tables share one recursive realization context and must remain auditable together")

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU16, NonZeroU64};

use bray_base::StableDigestHasher;
use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_codegen::{
    CodegenCallableMapping, CodegenCallableSignature, CodegenConstantMapping,
    CodegenConstantTermMapping, CodegenFieldLayout, CodegenHelperMapping, CodegenInstance,
    CodegenLinkage, CodegenMappings, CodegenOperationMapping, CodegenParameterMapping,
    CodegenResultMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenTerminatorMapping, CodegenTypeKind, CodegenTypeMapping, CodegenUnit,
    TargetAddressSpaceKind, child_constants, demanded_callable_instances,
    demanded_callable_instances_for_mir, demanded_constant_terms, demanded_constants,
    demanded_runtime_references, demanded_types,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirBlockKind, MirCall, MirCallTarget, MirCallableReference,
    MirCleanupEdge, MirEdge, MirFrameInitializer, MirFrameReference, MirHelperReference,
    MirOperand, MirOperationKind, MirPlace, MirProjection, MirProjectionKind,
    MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnit,
    MirUnitBuilder, MirUnitId, MirUnitKey, MirUnitKind,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ProtectedFrameOperation, RuntimeAbiRole,
};
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableAbi, CallableDefinitionId, CallableSignatureFact,
    ConstantTermData, ConstantValueKind, DeclaredLayoutMode, ForeignCallableDirection,
    GenericSubstitutionId, NamedTypeSymbolId, SymbolFactRequest,
    TypeAssociatedLifecycleSlot, TypeData, TypeId,
};
use bray_target::{TargetLayoutContract, TargetScalarKind, TargetValueLayout};

use super::super::CodegenFactError;
use super::super::Compilation;
use super::specialization::{
    ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn concrete_codegen_dependencies_for_mir(
        &self,
        owner: &ConcreteCodegenInstance,
        mir: &MirUnit,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ConcreteCodegenInstance>, CodegenFactError> {
        let mut dependencies = demanded_callable_instances_for_mir(mir)
            .into_iter()
            .map(|demand| {
                self.concrete_codegen_callee(
                    owner,
                    &demand,
                    target,
                    cancellation,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        for reference in mir
            .operations()
            .iter()
            .flat_map(|operation| operation.kind().helper_references())
        {
            if let Some(dependency) = self.concrete_codegen_helper_dependency(
                owner,
                &reference,
                target,
                cancellation,
            )? {
                dependencies.push(dependency);
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

    pub(super) fn codegen_mappings_for_product(
        &self,
        unit: &CodegenUnit,
        executable_host: Option<&ExecutableHostContract>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<CodegenMappings, CodegenFactError> {
        let operations =
            self.codegen_operations(unit, target, reachability, cancellation)?;

        let mut symbols = self.codegen_symbols(
            unit,
            &operations,
            executable_host,
            target,
            roots,
            reachability,
            cancellation,
        )?;

        let callables =
            self.codegen_callables(unit, target, reachability, cancellation)?;

        let realization = unit
            .instances()
            .first()
            .and_then(|instance| reachability.instance(instance.key()))
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let constants = self.codegen_constants(unit)?;

        let (constant_terms, terminators) =
            self.codegen_constant_terms(unit, realization)?;

        let mut demanded = demanded_types(unit);

        demanded.extend(constants.iter().map(|constant| constant.data().ty()));

        for symbol in &symbols {
            demanded.extend(signature_types(symbol.signature()));
        }

        let types = self.codegen_types(
            demanded,
            realization.substitution(),
            target,
            cancellation,
        )?;

        symbols.sort_unstable_by(|left, right| left.key().cmp(right.key()));

        CodegenMappings::try_new(
            unit,
            target,
            types,
            symbols,
            constants,
            constant_terms,
            callables,
            operations,
            terminators,
            [],
        )
        .map_err(CodegenFactError::InvalidMappings)
    }

    fn codegen_operations(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenOperationMapping>, CodegenFactError> {
        let mut mappings = Vec::new();

        for instance in unit.instances() {
            let realization = reachability
                .instance(instance.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            for (operation, data) in instance.mir().operations_with_ids() {
                let references = data.kind().helper_references();

                if references.is_empty() {
                    continue;
                }

                let helpers = references
                    .into_iter()
                    .map(|reference| {
                        self.codegen_helper(
                            instance,
                            data.kind(),
                            realization,
                            reference,
                            target,
                            cancellation,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                mappings.push(CodegenOperationMapping::new(
                    instance.key().clone(),
                    operation,
                    helpers,
                ));
            }
        }

        Ok(mappings)
    }

    fn codegen_helper(
        &self,
        owner: &CodegenInstance,
        operation: &MirOperationKind,
        owner_realization: &ConcreteCodegenInstance,
        reference: MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenHelperMapping, CodegenFactError> {
        let concrete_reference =
            self.concrete_codegen_helper_reference(owner_realization, &reference)?;

        if let Some(ty) = concrete_reference.lifecycle_type()
            && self.has_trivial_codegen_lifecycle(ty)?
        {
            return Ok(CodegenHelperMapping::lowered(reference));
        }

        if let Some(symbol) = direct_helper_symbol(owner, &reference) {
            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        if let Some(dependency) = self.concrete_codegen_helper_dependency(
            owner_realization,
            &reference,
            target,
            cancellation,
        )? {
            return dependency_symbol(owner, dependency.key(), &reference)
                .map(|symbol| CodegenHelperMapping::new(reference, symbol));
        }

        if matches!(reference, MirHelperReference::CreateFrame(_)) {
            let symbol =
                self.frame_creation_symbol(
                    owner,
                    owner_realization,
                    operation,
                    &reference,
                    target,
                    cancellation,
                )?;

            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        Err(CodegenFactError::MissingHelperInstance(reference))
    }

    pub(super) fn concrete_codegen_helper_dependency(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConcreteCodegenInstance>, CodegenFactError> {
        let concrete_reference =
            self.concrete_codegen_helper_reference(owner, reference)?;

        let dependency = match &concrete_reference {
            MirHelperReference::AnonymousCallable(unit) => self
                .concrete_codegen_bound_helper(owner, unit.clone())?,
            MirHelperReference::CallableDefault(provider) => {
                self.concrete_codegen_bound_helper(
                    owner,
                    self.runtime_default_unit((*provider).into(), reference)?,
                )?
            }
            MirHelperReference::ConstructionDefault(provider) => {
                self.concrete_codegen_bound_helper(
                    owner,
                    self.runtime_default_unit(provider.symbol(), reference)?,
                )?
            }
            MirHelperReference::TypeForm(callable)
            | MirHelperReference::Conversion(callable) => self
                .concrete_codegen_callable_data(
                    owner,
                    callable,
                    target,
                    cancellation,
                )?,
            MirHelperReference::Finalize(ty)
            | MirHelperReference::Destroy(ty)
            | MirHelperReference::Cleanup { ty, .. } => {
                if self.has_trivial_codegen_lifecycle(*ty)? {
                    return Ok(None);
                }

                self.concrete_codegen_lifecycle(concrete_reference, target)?
            }
            MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => return Ok(None),
        };

        Ok(Some(dependency))
    }

    fn concrete_codegen_helper_reference(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &MirHelperReference,
    ) -> Result<MirHelperReference, FactQueryError> {
        let substitution = owner.substitution();

        Ok(match reference {
            MirHelperReference::Finalize(ty) => {
                MirHelperReference::Finalize(
                    self.substitute_codegen_type(*ty, substitution)?,
                )
            }
            MirHelperReference::Destroy(ty) => {
                MirHelperReference::Destroy(
                    self.substitute_codegen_type(*ty, substitution)?,
                )
            }
            MirHelperReference::Cleanup { phase, ty } => {
                MirHelperReference::Cleanup {
                    phase: *phase,
                    ty: self.substitute_codegen_type(*ty, substitution)?,
                }
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => reference.clone(),
        })
    }

    fn runtime_default_unit(
        &self,
        provider: AnySymbolId,
        reference: &MirHelperReference,
    ) -> Result<bray_bound_tree::BoundUnitKey, CodegenFactError> {
        let symbols = self.symbol_graph()?;

        let provider = symbols
            .symbol_key(provider)
            .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))?;

        self.declared_unit_keys()?
            .into_iter()
            .find(|unit| {
                unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault
                    && unit.declared_owner() == provider
            })
            .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))
    }

    fn frame_creation_symbol(
        &self,
        owner: &CodegenInstance,
        owner_realization: &ConcreteCodegenInstance,
        operation: &MirOperationKind,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenSymbolKey, CodegenFactError> {
        let MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            initializer,
            ..
        }) = operation
        else {
            return Err(CodegenFactError::MissingHelperInstance(reference.clone()));
        };

        match initializer {
            MirFrameInitializer::Callable(call) => match call.target() {
                MirCallTarget::Direct(callable) => {
                    let dependency = self.concrete_codegen_callable_data(
                        owner_realization,
                        &callable.instance(),
                        target,
                        cancellation,
                    )?;

                    dependency_symbol(owner, dependency.key(), reference)
                }
                MirCallTarget::Indirect { .. } => {
                    Ok(helper_runtime_symbol(owner, RuntimeAbiRole::FrameCreation))
                }
            },
            MirFrameInitializer::TaskObservation { .. } => {
                Ok(helper_runtime_symbol(owner, RuntimeAbiRole::JoinRegistration))
            }
        }
    }

    pub(in crate::compilation) fn codegen_generated_lifecycle_mir(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        reference: &MirHelperReference,
        unit: MirUnitId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenFactError> {
        let MirUnitKey::GeneratedLifecycle(key) = instance.template() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        if Some(key.role())
            != bray_ir::MirGeneratedLifecycleRole::from_reference(reference)
        {
            return Err(FactQueryError::InfrastructureFailure.into());
        }

        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))?;

        let values = self.semantic_value_store()?;

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: ty,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let source = MirSourceAnchor::generated_lifecycle(reference.clone());

        let mut builder = MirUnitBuilder::for_generated_lifecycle(
            unit,
            instance.template().clone(),
            reference.clone(),
            instance.target().clone(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter, pointer)
            .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

        let place = MirPlace::new(
            storage,
            [MirProjection::new(
                MirProjectionKind::Dereference,
                pointer,
                ty,
            )],
            ty,
        );

        let operation_block = match reference {
            MirHelperReference::Cleanup { phase, .. } => {
                let kind = match phase {
                    bray_ir::MirCleanupPhase::TaskCancellation => {
                        MirBlockKind::CleanupBroadcast
                    }
                    bray_ir::MirCleanupPhase::LifecycleResolution => {
                        MirBlockKind::LifecycleResolution
                    }
                };

                let block = builder
                    .push_block(source.clone(), kind)
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

                builder
                    .set_terminator(
                        entry,
                        source.clone(),
                        MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                            *phase,
                            MirEdge::new(block, []),
                        )),
                    )
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

                block
            }
            MirHelperReference::Finalize(_) | MirHelperReference::Destroy(_) => entry,
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(CodegenFactError::MissingHelperInstance(reference.clone()));
            }
        };

        self.push_generated_lifecycle_operations(
            &mut builder,
            operation_block,
            &source,
            reference,
            place,
            instance.target().runtime_abi(),
            cancellation,
        )?;

        match reference {
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ..
            } => {
                let lifecycle = builder
                    .push_block(source.clone(), MirBlockKind::LifecycleResolution)
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

                builder
                    .set_terminator(
                        operation_block,
                        source.clone(),
                        MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                            bray_ir::MirCleanupPhase::LifecycleResolution,
                            MirEdge::new(lifecycle, []),
                        )),
                    )
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

                builder
                    .set_terminator(
                        lifecycle,
                        source,
                        MirTerminatorKind::Return(None),
                    )
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;
            }
            MirHelperReference::Finalize(_)
            | MirHelperReference::Destroy(_)
            | MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            } => {
                builder
                    .set_terminator(
                        operation_block,
                        source,
                        MirTerminatorKind::Return(None),
                    )
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(CodegenFactError::MissingHelperInstance(reference.clone()));
            }
        }

        builder
            .finish(entry)
            .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)
    }

    fn push_generated_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<(), CodegenFactError> {
        if self.push_compiler_known_lifecycle_operations(
            builder,
            block,
            source,
            reference,
            &place,
            runtime_abi,
        )? {
            return Ok(());
        }

        match reference {
            MirHelperReference::Finalize(ty) => {
                if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Finalizer,
                    cancellation,
                )? {
                    self.push_lifecycle_call(
                        builder, block, source, place, callable,
                    )?;
                }
            }
            MirHelperReference::Destroy(ty) => {
                if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Destructor,
                    cancellation,
                )? {
                    self.push_lifecycle_call(
                        builder,
                        block,
                        source,
                        place.clone(),
                        callable,
                    )?;
                }

                for child in self
                    .lifecycle_children(place, cancellation)?
                    .into_iter()
                    .rev()
                {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Finalize(child.clone()),
                    )?;

                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Destroy(child),
                    )?;
                }
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ..
            } => {
                for child in self
                    .lifecycle_children(place, cancellation)?
                    .into_iter()
                    .rev()
                {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Cleanup {
                            phase: bray_ir::MirCleanupPhase::TaskCancellation,
                            place: child,
                        },
                    )?;
                }
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            } => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Finalize(place.clone()),
                )?;

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Destroy(place),
                )?;
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(CodegenFactError::MissingHelperInstance(reference.clone()));
            }
        }

        Ok(())
    }

    fn push_compiler_known_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: &MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<bool, CodegenFactError> {
        let Some(ty) = reference.lifecycle_type() else {
            return Ok(false);
        };

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(false);
        };

        let Some(role) =
            super::super::foreign::compiler_known_representation(self, *definition)
        else {
            return Ok(false);
        };

        if role != RepresentationRole::Task {
            return Ok(false);
        }

        match reference {
            MirHelperReference::Finalize(_) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::ResolveTask {
                        task: MirOperand::Move(place.clone()),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::JoinRegistration,
                            runtime_abi,
                        ),
                    }),
                )?;
            }
            MirHelperReference::Destroy(_) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(
                        MirAsyncOperation::DestroyTerminalTask {
                            task: MirOperand::Move(place.clone()),
                        },
                    ),
                )?;
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ..
            } => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(
                        MirAsyncOperation::RequestTaskCancellation {
                            task: MirOperand::Move(place.clone()),
                            runtime: MirRuntimeReference::new(
                                RuntimeAbiRole::TaskCancellationRequest,
                                runtime_abi,
                            ),
                        },
                    ),
                )?;
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ..
            } => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(MirAsyncOperation::ResolveTask {
                        task: MirOperand::Move(place.clone()),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::JoinRegistration,
                            runtime_abi,
                        ),
                    }),
                )?;

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Async(
                        MirAsyncOperation::DestroyTerminalTask {
                            task: MirOperand::Move(place.clone()),
                        },
                    ),
                )?;
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(CodegenFactError::MissingHelperInstance(reference.clone()));
            }
        }

        Ok(true)
    }

    fn push_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<(), CodegenFactError> {
        builder
            .push_operation(block, source.clone(), operation, None)
            .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

        Ok(())
    }

    fn push_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: (MirCallableReference, TypeId, TypeId),
    ) -> Result<(), CodegenFactError> {
        let (callable, receiver, result) = callable;

        let values = self.semantic_value_store()?;

        let receiver_data = values
            .type_data(receiver)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let receiver = match receiver_data.as_ref() {
            TypeData::Borrow { kind, .. } => {
                let value = builder
                    .push_operation(
                        block,
                        source.clone(),
                        MirOperationKind::Borrow {
                            kind: *kind,
                            place,
                        },
                        Some(receiver),
                    )
                    .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?
                    .result()
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                MirOperand::Value(value)
            }
            TypeData::Error
            | TypeData::Named { .. }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Tuple(_)
            | TypeData::Array { .. }
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::TraitView(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => MirOperand::Move(place),
        };

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    bray_bound_tree::BoundCallResult::Immediate(result),
                    [receiver],
                    [],
                )),
                Some(result),
            )
            .map_err(CodegenFactError::InvalidGeneratedLifecycleMir)?;

        Ok(())
    }

    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<Option<(MirCallableReference, TypeId, TypeId)>, CodegenFactError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(None);
        };

        let surface =
            self.type_associated_surface_result_with_cancellation(*definition, cancellation)?;

        let mut members = surface
            .value()
            .lifecycle_members()
            .iter()
            .filter(|member| member.slot() == slot)
            .map(|member| member.id());

        let Some(member) = members.next() else {
            return Ok(None);
        };

        if members.next().is_some() {
            return Err(FactQueryError::InfrastructureFailure.into());
        }

        let callable = super::super::implementation::callable_instance(
            values,
            member,
            [*substitution],
        )?;

        let facts = self.binder_facts(cancellation)?;

        let signature = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(super::super::binder::binder_fact_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [signature.value().callable_type(), signature.value().result()],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            values,
            signature.value(),
            callable.substitution(),
            constants.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

        let receiver = signature
            .receiver()
            .map(|receiver| receiver.ty())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        Ok(Some((
            MirCallableReference::new(callable, callable_type.abi()),
            receiver,
            signature.result(),
        )))
    }

    fn lifecycle_children(
        &self,
        place: MirPlace,
        cancellation: &CancellationToken,
    ) -> Result<Vec<MirPlace>, CodegenFactError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(place.ty())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let children = match data.as_ref() {
            TypeData::Named {
                definition: NamedTypeSymbolId::Struct(structure),
                substitution,
            } => {
                if super::super::foreign::compiler_known_representation(
                    self,
                    NamedTypeSymbolId::Struct(*structure),
                )
                .is_some()
                {
                    return Ok(Vec::new());
                }

                let facts = self.binder_facts(cancellation)?;

                let structure = facts
                    .symbols()
                    .structure(*structure)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                structure
                    .fields()
                    .iter()
                    .map(|field| {
                        let template = facts
                            .symbol_fact(bray_symbols::SymbolFactRequest::<
                                bray_symbols::StructFieldTypeFact,
                            >::new(*field))
                            .map_err(super::super::binder::binder_fact_error)?;

                        let ty = self.resolve_codegen_type(
                            template.value(),
                            *substitution,
                            cancellation,
                        )?;

                        Ok((
                            MirProjectionKind::Field(
                                bray_ir::MirFieldReference::Struct(*field),
                            ),
                            ty,
                        ))
                    })
                    .collect::<Result<Vec<_>, FactQueryError>>()?
            }
            TypeData::Tuple(elements) => elements
                .iter()
                .copied()
                .enumerate()
                .map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| CodegenFactError::LayoutOverflow(place.ty()))?;

                    Ok((MirProjectionKind::TupleField(index), ty))
                })
                .collect::<Result<Vec<_>, CodegenFactError>>()?,
            TypeData::Array { element, length } => {
                let length = closed_array_length(values, *length)?;

                let length = u32::try_from(length)
                    .map_err(|_| CodegenFactError::LayoutOverflow(place.ty()))?;

                (0..length)
                    .map(|index| {
                        (MirProjectionKind::ElementFromStart(index), *element)
                    })
                    .collect()
            }
            TypeData::OwnedIndirection { target, .. } => {
                vec![(MirProjectionKind::Dereference, *target)]
            }
            TypeData::Error
            | TypeData::Named {
                definition: NamedTypeSymbolId::Union(_),
                ..
            }
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::Borrow { .. }
            | TypeData::TraitView(_)
            | TypeData::Callable(_) => Vec::new(),
        };

        Ok(children
            .into_iter()
            .map(|(kind, ty)| {
                let mut projections = place.projections().to_vec();
                projections.push(MirProjection::new(kind, place.ty(), ty));

                MirPlace::new(place.storage(), projections, ty)
            })
            .collect())
    }

    fn has_trivial_codegen_lifecycle(&self, ty: TypeId) -> Result<bool, FactQueryError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let trivial = match data.as_ref() {
            TypeData::Named { definition, .. } => {
                super::super::foreign::compiler_known_representation(self, *definition).is_some_and(
                    |role| {
                        matches!(
                            role,
                            RepresentationRole::Unit
                                | RepresentationRole::Never
                                | RepresentationRole::RawPointer
                        ) || super::super::representation::target_scalar(role).is_some()
                    },
                )
            }
            TypeData::Borrow { .. } | TypeData::Callable(_) => true,
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Tuple(_)
            | TypeData::Array { .. }
            | TypeData::Slice(_)
            | TypeData::OwnedIndirection { .. }
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::TraitView(_) => false,
        };

        Ok(trivial)
    }

    fn codegen_symbols(
        &self,
        unit: &CodegenUnit,
        operations: &[CodegenOperationMapping],
        executable_host: Option<&ExecutableHostContract>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenFactError> {
        let mut symbols = Vec::new();

        for instance in unit.instances() {
            let (name, linkage, signature) = match instance.mir().kind() {
                MirUnitKind::ExecutableHost(host) => (
                    host.native_entry().clone(),
                    CodegenLinkage::Export,
                    void_signature(CallableAbi::Bray),
                ),
                MirUnitKind::GeneratedLifecycle(reference) => (
                    generated_symbol_name(
                        target,
                        CodegenLinkage::Internal,
                        "lifecycle",
                        instance.key(),
                    )?,
                    CodegenLinkage::Internal,
                    self.generated_lifecycle_signature(reference)?,
                ),
                MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                    let realization = reachability
                        .instance(instance.key())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let boundary = self.codegen_native_boundary(instance.key(), cancellation)?;

                    let linkage = boundary
                        .as_ref()
                        .map(|(_, linkage)| *linkage)
                        .unwrap_or_else(|| {
                            if roots.contains(instance.key()) {
                                CodegenLinkage::Export
                            } else {
                                CodegenLinkage::Internal
                            }
                        });

                    (
                        boundary.map(|(name, _)| name).map_or_else(
                            || generated_symbol_name(target, linkage, "instance", instance.key()),
                            Ok,
                        )?,
                        linkage,
                        self.codegen_instance_signature(
                            realization,
                            cancellation,
                        )?,
                    )
                }
            };

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.key().clone()),
                name,
                linkage,
                signature,
            ));
        }

        for instance in unit.external_instances() {
            let realization = reachability
                .instance(instance)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let boundary = self.codegen_native_boundary(instance, cancellation)?;

            let linkage = boundary
                .as_ref()
                .map(|(_, linkage)| *linkage)
                .unwrap_or(CodegenLinkage::Import);

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.clone()),
                boundary.map(|(name, _)| name).map_or_else(
                    || generated_symbol_name(target, linkage, "instance", instance),
                    Ok,
                )?,
                linkage,
                self.codegen_instance_signature(realization, cancellation)?,
            ));
        }

        for reference in codegen_runtime_references(unit, operations) {
            let Some(binding) =
                executable_host.and_then(|host| host.role_binding(reference.role()))
            else {
                return Err(CodegenFactError::MissingRuntimeRole(reference.role()));
            };

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Runtime(reference),
                binding.symbol_name().clone(),
                CodegenLinkage::Import,
                void_signature(CallableAbi::Bray),
            ));
        }

        for instance in unit.instances() {
            let Some(frame) = instance.protected_frame_identity() else {
                continue;
            };

            for operation in ProtectedFrameOperation::ALL {
                symbols.push(CodegenSymbolMapping::new(
                    CodegenSymbolKey::ProtectedFrame { frame, operation },
                    generated_frame_symbol_name(target, frame, operation)?,
                    CodegenLinkage::Internal,
                    void_signature(CallableAbi::Bray),
                ));
            }
        }

        for (frame, operation) in operations.iter().flat_map(|mapping| {
            mapping.helpers().iter().filter_map(|helper| {
                let Some(CodegenSymbolKey::ProtectedFrame { frame, operation }) =
                    helper.symbol()
                else {
                    return None;
                };

                Some((*frame, *operation))
            })
        }) {
            let key = CodegenSymbolKey::ProtectedFrame { frame, operation };

            if symbols.iter().any(|symbol| symbol.key() == &key) {
                continue;
            }

            symbols.push(CodegenSymbolMapping::new(
                key,
                generated_frame_symbol_name(target, frame, operation)?,
                CodegenLinkage::Import,
                void_signature(CallableAbi::Bray),
            ));
        }

        Ok(symbols)
    }

    fn codegen_native_boundary(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<(BinarySymbolName, CodegenLinkage)>, CodegenFactError> {
        if matches!(
            instance.template(),
            MirUnitKey::GeneratedLifecycle(_) | MirUnitKey::ExecutableHost(_)
        ) {
            return Ok(None);
        }

        let definition = self.codegen_callable_definition(instance)?;

        let bray_symbols::CallableSymbolId::Function(function) = definition.callable_symbol()
        else {
            return Ok(None);
        };

        let contract = self.foreign_callable_contract_with_cancellation(function, cancellation)?;

        let Some(contract) = contract.value() else {
            return Ok(None);
        };

        let name = BinarySymbolName::try_new(contract.symbol())
            .ok_or(CodegenFactError::InvalidSymbolName)?;

        let linkage = match contract.direction() {
            ForeignCallableDirection::Import => CodegenLinkage::Import,
            ForeignCallableDirection::Export => CodegenLinkage::Export,
        };

        Ok(Some((name, linkage)))
    }

    fn codegen_callables(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCallableMapping>, CodegenFactError> {
        demanded_callable_instances(unit)
            .into_iter()
            .map(|(owner, demand)| {
                let owner_realization = reachability
                    .instance(&owner)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let instance = self.concrete_codegen_callee(
                    owner_realization,
                    &demand,
                    target,
                    cancellation,
                )?;

                Ok(CodegenCallableMapping::new(
                    owner,
                    demand.reference(),
                    instance.key().clone(),
                ))
            })
            .collect()
    }

    fn codegen_constants(
        &self,
        unit: &CodegenUnit,
    ) -> Result<Vec<CodegenConstantMapping>, CodegenFactError> {
        let values = self.semantic_value_store()?;
        let mut pending: Vec<_> = demanded_constants(unit).values().iter().copied().collect();
        let mut mapped = BTreeMap::new();

        while let Some(value) = pending.pop() {
            if mapped.contains_key(&value) {
                continue;
            }

            let data = values
                .constant_value_data(value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            pending.extend(child_constants(data.kind()));
            mapped.insert(value, data.as_ref().clone());
        }

        Ok(mapped
            .into_iter()
            .map(|(value, data)| CodegenConstantMapping::new(value, data))
            .collect())
    }

    fn codegen_constant_terms(
        &self,
        unit: &CodegenUnit,
        realization: &ConcreteCodegenInstance,
    ) -> Result<
        (
            Vec<CodegenConstantTermMapping>,
            Vec<CodegenTerminatorMapping>,
        ),
        CodegenFactError,
    > {
        let values = self.semantic_value_store()?;
        let mut terms = Vec::new();
        let mut resolved = BTreeMap::new();

        for instance in unit.instances() {
            for template_term in demanded_constant_terms(instance.mir()) {
                let term = self.substitute_codegen_constant_term(
                    template_term,
                    realization.substitution(),
                )?;

                let data = values
                    .constant_term_data(term)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let ConstantTermData::Value(value) = data.as_ref() else {
                    return Err(CodegenFactError::OpenConstantTerm(term));
                };

                resolved.insert((instance.key().clone(), template_term), *value);

                terms.push(CodegenConstantTermMapping::new(
                    instance.key().clone(),
                    template_term,
                    *value,
                ));
            }
        }

        let mut terminators = Vec::new();

        for instance in unit.instances() {
            for (block, data) in instance.mir().blocks_with_ids() {
                let Some(term) = data.terminator().kind().pattern_constant_term() else {
                    continue;
                };

                let Some(value) = resolved.get(&(instance.key().clone(), term)) else {
                    return Err(CodegenFactError::OpenConstantTerm(term));
                };

                terminators.push(CodegenTerminatorMapping::new(
                    instance.key().clone(),
                    block,
                    [*value],
                ));
            }
        }

        Ok((terms, terminators))
    }

    fn codegen_types(
        &self,
        demanded: BTreeSet<TypeId>,
        substitution: Option<GenericSubstitutionId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenTypeMapping>, CodegenFactError> {
        let mut mappings = BTreeMap::new();

        for template in demanded {
            let ty = self.substitute_codegen_type(template, substitution)?;

            self.codegen_type(
                ty,
                target,
                cancellation,
                &mut mappings,
                &mut BTreeSet::new(),
            )?;

            if template != ty {
                let mapping = mappings
                    .get(&ty)
                    .cloned()
                    .ok_or(CodegenFactError::UnsupportedType(ty))?;

                mappings.insert(
                    template,
                    CodegenTypeMapping::new(
                        template,
                        mapping.layout(),
                        mapping.kind().clone(),
                    ),
                );
            }
        }

        Ok(mappings.into_values().collect())
    }

    fn substitute_codegen_type(
        &self,
        ty: TypeId,
        substitution: Option<GenericSubstitutionId>,
    ) -> Result<TypeId, FactQueryError> {
        let Some(substitution) = substitution else {
            return Ok(ty);
        };

        self.semantic_value_store()?
            .substitute_type(ty, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    fn substitute_codegen_constant_term(
        &self,
        term: bray_symbols::ConstantTermId,
        substitution: Option<GenericSubstitutionId>,
    ) -> Result<bray_symbols::ConstantTermId, FactQueryError> {
        let Some(substitution) = substitution else {
            return Ok(term);
        };

        self.semantic_value_store()?
            .substitute_constant_term(term, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    fn codegen_type(
        &self,
        ty: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenFactError> {
        if mappings.contains_key(&ty) {
            return Ok(());
        }

        cancellation.check()?;

        if !pending.insert(ty) {
            return Err(CodegenFactError::RecursiveValueType(ty));
        }

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mapping = match data.as_ref() {
            TypeData::Named {
                definition,
                substitution,
            } => self.codegen_named_type(
                ty,
                *definition,
                *substitution,
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::Tuple(elements) => self.codegen_aggregate_type(
                ty,
                elements.iter().copied().map(|element| (None, element)),
                TargetLayoutContract::Default,
                None,
                None,
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::Array { element, length } => {
                self.codegen_type(*element, target, cancellation, mappings, pending)?;

                let length = closed_array_length(values, *length)?;

                let element_layout = mappings
                    .get(element)
                    .map(CodegenTypeMapping::layout)
                    .ok_or(CodegenFactError::UnsupportedType(*element))?;

                CodegenTypeMapping::new(
                    ty,
                    TargetValueLayout::new(
                        element_layout
                            .size()
                            .checked_mul(length)
                            .ok_or(CodegenFactError::LayoutOverflow(ty))?,
                        element_layout.alignment(),
                        TargetLayoutContract::Default,
                    ),
                    CodegenTypeKind::Array {
                        element: *element,
                        length,
                    },
                )
            }
            TypeData::Borrow {
                target: pointee, ..
            }
            | TypeData::OwnedIndirection {
                target: pointee, ..
            } => pointer_mapping(ty, *pointee, target),
            TypeData::Callable(callable) => {
                let signature = callable_type_signature(self, callable)?;

                CodegenTypeMapping::new(
                    ty,
                    pointer_layout(target),
                    CodegenTypeKind::Callable(signature.into()),
                )
            }
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Slice(_)
            | TypeData::Generator(_)
            | TypeData::Nullable(_)
            | TypeData::TraitView(_) => return Err(CodegenFactError::UnsupportedType(ty)),
        };

        pending.remove(&ty);
        mappings.insert(ty, mapping);

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "recursive type realization keeps the selected target and cycle state explicit"
    )]
    fn codegen_named_type(
        &self,
        ty: TypeId,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenFactError> {
        let role = super::super::foreign::compiler_known_representation(self, definition);

        if let Some(role) = role {
            return self.codegen_compiler_known_type(ty, role, target);
        }

        let facts = self.binder_facts(cancellation)?;

        match definition {
            NamedTypeSymbolId::Struct(structure) => {
                let structure = facts
                    .symbols()
                    .structure(structure)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let fields = structure
                    .fields()
                    .iter()
                    .map(|field| {
                        let template = facts
                            .symbol_fact(bray_symbols::SymbolFactRequest::<
                                bray_symbols::StructFieldTypeFact,
                            >::new(*field))
                            .map_err(super::super::binder::binder_fact_error)?;

                        let field_ty = self.resolve_codegen_type(
                            template.value(),
                            substitution,
                            cancellation,
                        )?;

                        Ok((Some(bray_ir::MirFieldReference::Struct(*field)), field_ty))
                    })
                    .collect::<Result<Vec<_>, FactQueryError>>()?;

                let representation =
                    self.declared_type_representation_with_cancellation(definition, cancellation)?;

                self.codegen_aggregate_type(
                    ty,
                    fields,
                    target_layout_contract(representation.value().layout()),
                    representation.value().alignment(),
                    representation.value().packing(),
                    target,
                    cancellation,
                    mappings,
                    pending,
                )
            }
            NamedTypeSymbolId::Union(_) => Err(CodegenFactError::UnsupportedType(ty)),
        }
    }

    fn codegen_compiler_known_type(
        &self,
        ty: TypeId,
        role: RepresentationRole,
        target: &CodegenTarget,
    ) -> Result<CodegenTypeMapping, CodegenFactError> {
        if matches!(role, RepresentationRole::Unit | RepresentationRole::Never) {
            return Ok(CodegenTypeMapping::new(
                ty,
                TargetValueLayout::new(0, NonZeroU64::MIN, TargetLayoutContract::Default),
                CodegenTypeKind::Unit,
            ));
        }

        if role == RepresentationRole::RawPointer {
            return Ok(pointer_mapping(ty, ty, target));
        }

        let Some(scalar) = super::super::representation::target_scalar(role) else {
            return Err(CodegenFactError::UnsupportedType(ty));
        };

        scalar_mapping(ty, scalar, target)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "recursive aggregate realization keeps layout and cycle state explicit"
    )]
    fn codegen_aggregate_type(
        &self,
        ty: TypeId,
        fields: impl IntoIterator<Item = (Option<bray_ir::MirFieldReference>, TypeId)>,
        contract: TargetLayoutContract,
        requested_alignment: Option<u64>,
        packing: Option<u64>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenFactError> {
        let mut offset = 0_u64;
        let mut alignment = NonZeroU64::MIN;
        let mut layouts = Vec::new();

        for (reference, field) in fields {
            self.codegen_type(field, target, cancellation, mappings, pending)?;

            let field_layout = mappings
                .get(&field)
                .map(CodegenTypeMapping::layout)
                .ok_or(CodegenFactError::UnsupportedType(field))?;

            let field_alignment = packing
                .and_then(NonZeroU64::new)
                .map_or(field_layout.alignment(), |packing| {
                    field_layout.alignment().min(packing)
                });

            alignment = alignment.max(field_alignment);

            offset =
                align_to(offset, field_alignment).ok_or(CodegenFactError::LayoutOverflow(ty))?;

            layouts.push(CodegenFieldLayout::new(reference, field, offset));

            offset = offset
                .checked_add(field_layout.size())
                .ok_or(CodegenFactError::LayoutOverflow(ty))?;
        }

        if let Some(requested) = requested_alignment.and_then(NonZeroU64::new) {
            alignment = alignment.max(requested);
        }

        let size = align_to(offset, alignment).ok_or(CodegenFactError::LayoutOverflow(ty))?;

        Ok(CodegenTypeMapping::new(
            ty,
            TargetValueLayout::new(size, alignment, contract),
            CodegenTypeKind::aggregate(layouts),
        ))
    }

    fn resolve_codegen_type(
        &self,
        template: &bray_symbols::TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, FactQueryError> {
        let constants =
            self.checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

        let ty = bray_checker::resolve_type_expression_template(
            self.semantic_value_store()?,
            template,
            constants.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

        self.semantic_value_store()?
            .substitute_type(ty, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    pub(super) fn codegen_instance_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenFactError> {
        if let MirUnitKey::GeneratedLifecycle(_) = instance.key().template() {
            let reference = instance
                .generated_lifecycle_reference()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            return self.generated_lifecycle_signature(reference);
        }

        let definition = match instance.key().template() {
            MirUnitKey::ExecutableHost(_) => return Ok(void_signature(CallableAbi::Bray)),
            MirUnitKey::Bound(_) | MirUnitKey::ExternalCallable(_) => {
                self.codegen_callable_definition(instance.key())?
            }
            MirUnitKey::GeneratedLifecycle(_) => {
                return Err(FactQueryError::InfrastructureFailure.into());
            }
        };

        let values = self.semantic_value_store()?;

        let callable = instance
            .callable_instance()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if callable.definition() != definition {
            return Err(FactQueryError::InfrastructureFailure.into());
        }

        let substitution = callable.substitution();
        let facts = self.binder_facts(cancellation)?;

        let template = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                definition.callable_symbol(),
            ))
            .map_err(super::super::binder::binder_fact_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [template.value().callable_type(), template.value().result()],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            values,
            template.value(),
            substitution,
            constants.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

        let callable = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable) = callable.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let parameters = signature
            .receiver()
            .map(|receiver| receiver.ty())
            .into_iter()
            .chain(
                signature
                    .parameters()
                    .iter()
                    .map(|parameter| parameter.ty()),
            )
            .map(|ty| CodegenParameterMapping::direct(ty, None, []));

        let result = if is_unit(self, signature.result())? {
            CodegenResultMapping::Void
        } else {
            CodegenResultMapping::direct(signature.result(), None, [])
        };

        Ok(CodegenCallableSignature::new(
            parameters,
            result,
            callable.abi(),
            false,
        ))
    }

    fn codegen_callable_definition(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
    ) -> Result<CallableDefinitionId, FactQueryError> {
        match instance.template() {
            MirUnitKey::Bound(key) => {
                let symbol = self
                    .symbol_graph()?
                    .symbol_for_key(key.declared_owner())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                CallableDefinitionId::try_new(symbol).ok_or(FactQueryError::InfrastructureFailure)
            }
            MirUnitKey::ExternalCallable(definition) => Ok(*definition),
            MirUnitKey::ExecutableHost(_) | MirUnitKey::GeneratedLifecycle(_) => {
                Err(FactQueryError::InfrastructureFailure)
            }
        }
    }

    fn generated_lifecycle_signature(
        &self,
        reference: &MirHelperReference,
    ) -> Result<CodegenCallableSignature, CodegenFactError> {
        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))?;

        let pointer = self
            .semantic_value_store()?
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: ty,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(CodegenCallableSignature::new(
            [CodegenParameterMapping::direct(pointer, None, [])],
            CodegenResultMapping::Void,
            CallableAbi::Bray,
            false,
        ))
    }
}

const fn target_layout_contract(layout: DeclaredLayoutMode) -> TargetLayoutContract {
    match layout {
        DeclaredLayoutMode::Default => TargetLayoutContract::Default,
        DeclaredLayoutMode::Stable => TargetLayoutContract::Stable,
        DeclaredLayoutMode::C => TargetLayoutContract::C,
        DeclaredLayoutMode::Transparent => TargetLayoutContract::Transparent,
    }
}

fn signature_types(signature: &CodegenCallableSignature) -> impl Iterator<Item = TypeId> + '_ {
    signature
        .parameters()
        .iter()
        .filter_map(|parameter| match parameter {
            CodegenParameterMapping::Ignore => None,
            CodegenParameterMapping::Direct { ty, .. } => Some(*ty),
            CodegenParameterMapping::Indirect { pointer, .. } => Some(*pointer),
        })
        .chain(match signature.result() {
            CodegenResultMapping::Void => None,
            CodegenResultMapping::Direct { ty, .. } => Some(*ty),
            CodegenResultMapping::Indirect { pointer, .. } => Some(*pointer),
        })
}

fn closed_array_length(
    values: &bray_symbols::SemanticValueStore,
    term_id: bray_symbols::ConstantTermId,
) -> Result<u64, CodegenFactError> {
    let term = values
        .constant_term_data(term_id)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let ConstantTermData::Value(value) = term.as_ref() else {
        return Err(CodegenFactError::OpenConstantTerm(term_id));
    };

    let data = values
        .constant_value_data(*value)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let ConstantValueKind::Integer(value) = data.kind() else {
        return Err(CodegenFactError::UnsupportedType(data.ty()));
    };

    value
        .to_u64()
        .ok_or(CodegenFactError::LayoutOverflow(data.ty()))
}

fn scalar_mapping(
    ty: TypeId,
    scalar: TargetScalarKind,
    target: &CodegenTarget,
) -> Result<CodegenTypeMapping, CodegenFactError> {
    let pointer_width = target.machine().pointer_width_bits().get();

    let (size, kind) = match scalar {
        TargetScalarKind::Bool => (1, CodegenTypeKind::Boolean),
        TargetScalarKind::Char => (4, CodegenTypeKind::UnsignedInteger(nonzero_width(32))),
        TargetScalarKind::I8 => (1, CodegenTypeKind::SignedInteger(nonzero_width(8))),
        TargetScalarKind::I16 => (2, CodegenTypeKind::SignedInteger(nonzero_width(16))),
        TargetScalarKind::I32 => (4, CodegenTypeKind::SignedInteger(nonzero_width(32))),
        TargetScalarKind::I64 => (8, CodegenTypeKind::SignedInteger(nonzero_width(64))),
        TargetScalarKind::I128 => (16, CodegenTypeKind::SignedInteger(nonzero_width(128))),
        TargetScalarKind::U8 => (1, CodegenTypeKind::UnsignedInteger(nonzero_width(8))),
        TargetScalarKind::U16 => (2, CodegenTypeKind::UnsignedInteger(nonzero_width(16))),
        TargetScalarKind::U32 => (4, CodegenTypeKind::UnsignedInteger(nonzero_width(32))),
        TargetScalarKind::U64 => (8, CodegenTypeKind::UnsignedInteger(nonzero_width(64))),
        TargetScalarKind::U128 => (16, CodegenTypeKind::UnsignedInteger(nonzero_width(128))),
        TargetScalarKind::Isize => (
            u64::from(pointer_width.div_ceil(8)),
            CodegenTypeKind::SignedInteger(nonzero_width(pointer_width)),
        ),
        TargetScalarKind::Usize => (
            u64::from(pointer_width.div_ceil(8)),
            CodegenTypeKind::UnsignedInteger(nonzero_width(pointer_width)),
        ),
        TargetScalarKind::R16 => (2, CodegenTypeKind::Float(nonzero_width(16))),
        TargetScalarKind::R32 => (4, CodegenTypeKind::Float(nonzero_width(32))),
        TargetScalarKind::R64 => (8, CodegenTypeKind::Float(nonzero_width(64))),
        TargetScalarKind::R128 => (16, CodegenTypeKind::Float(nonzero_width(128))),
        TargetScalarKind::C32
        | TargetScalarKind::C64
        | TargetScalarKind::C128
        | TargetScalarKind::C256 => return Err(CodegenFactError::UnsupportedType(ty)),
    };

    Ok(CodegenTypeMapping::new(
        ty,
        TargetValueLayout::new(
            size,
            target.profile().facts().scalars().alignment(scalar),
            TargetLayoutContract::Default,
        ),
        kind,
    ))
}

fn pointer_mapping(ty: TypeId, pointee: TypeId, target: &CodegenTarget) -> CodegenTypeMapping {
    CodegenTypeMapping::new(
        ty,
        pointer_layout(target),
        CodegenTypeKind::Pointer {
            target: pointee,
            address_space: TargetAddressSpaceKind::Default,
        },
    )
}

fn pointer_layout(target: &CodegenTarget) -> TargetValueLayout {
    TargetValueLayout::new(
        u64::from(target.machine().pointer_width_bits().get().div_ceil(8)),
        NonZeroU64::from(target.machine().pointer_alignment_bytes()),
        TargetLayoutContract::Default,
    )
}

fn callable_type_signature(
    compilation: &Compilation,
    callable: &bray_symbols::CallableTypeData,
) -> Result<CodegenCallableSignature, CodegenFactError> {
    let result = if is_unit(compilation, callable.result())? {
        CodegenResultMapping::Void
    } else {
        CodegenResultMapping::direct(callable.result(), None, [])
    };

    Ok(CodegenCallableSignature::new(
        callable
            .parameters()
            .iter()
            .map(|parameter| CodegenParameterMapping::direct(parameter.ty(), None, [])),
        result,
        callable.abi(),
        false,
    ))
}

fn void_signature(abi: CallableAbi) -> CodegenCallableSignature {
    CodegenCallableSignature::new([], CodegenResultMapping::Void, abi, false)
}

fn helper_runtime_symbol(
    owner: &CodegenInstance,
    role: RuntimeAbiRole,
) -> CodegenSymbolKey {
    CodegenSymbolKey::Runtime(MirRuntimeReference::new(
        role,
        owner.key().target().runtime_abi(),
    ))
}

fn direct_helper_symbol(
    owner: &CodegenInstance,
    reference: &MirHelperReference,
) -> Option<CodegenSymbolKey> {
    let symbol = match reference {
        MirHelperReference::BeginGenerator => {
            helper_runtime_symbol(owner, RuntimeAbiRole::GeneratorBegin)
        }
        MirHelperReference::PushGenerator => {
            helper_runtime_symbol(owner, RuntimeAbiRole::GeneratorPush)
        }
        MirHelperReference::FinishGenerator => {
            helper_runtime_symbol(owner, RuntimeAbiRole::GeneratorFinish)
        }
        MirHelperReference::PanicReport => {
            helper_runtime_symbol(owner, RuntimeAbiRole::PanicReportConstruction)
        }
        MirHelperReference::MoveInactiveFrame(frame) => match frame {
            MirFrameReference::Known(frame) => CodegenSymbolKey::ProtectedFrame {
                frame: *frame,
                operation: ProtectedFrameOperation::MoveBeforeStart,
            },
            MirFrameReference::Erased => {
                helper_runtime_symbol(owner, RuntimeAbiRole::InactiveFrameMove)
            }
        },
        MirHelperReference::ComposeAwaitedFrame(_) => {
            helper_runtime_symbol(owner, RuntimeAbiRole::AwaitedFrameComposition)
        }
        MirHelperReference::CommitAwaitedCompletion(frame) => match frame {
            MirFrameReference::Known(frame) => CodegenSymbolKey::ProtectedFrame {
                frame: *frame,
                operation: ProtectedFrameOperation::CompletionMove,
            },
            MirFrameReference::Erased => {
                helper_runtime_symbol(owner, RuntimeAbiRole::FrameCompletionMove)
            }
        },
        MirHelperReference::DestroyTerminalTask => {
            helper_runtime_symbol(owner, RuntimeAbiRole::TaskDestruction)
        }
        MirHelperReference::AnonymousCallable(_)
        | MirHelperReference::CallableDefault(_)
        | MirHelperReference::ConstructionDefault(_)
        | MirHelperReference::TypeForm(_)
        | MirHelperReference::Conversion(_)
        | MirHelperReference::Finalize(_)
        | MirHelperReference::Destroy(_)
        | MirHelperReference::Cleanup { .. }
        | MirHelperReference::CreateFrame(_) => return None,
    };

    Some(symbol)
}

fn codegen_runtime_references(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
) -> BTreeSet<MirRuntimeReference> {
    let mut references = demanded_runtime_references(unit);

    references.extend(operations.iter().flat_map(|operation| {
        operation.helpers().iter().filter_map(|helper| {
            let Some(CodegenSymbolKey::Runtime(reference)) = helper.symbol() else {
                return None;
            };

            Some(*reference)
        })
    }));

    references
}

fn dependency_symbol(
    owner: &CodegenInstance,
    instance: &bray_codegen::CodegenInstanceKey,
    reference: &MirHelperReference,
) -> Result<CodegenSymbolKey, CodegenFactError> {
    owner
        .dependencies()
        .iter()
        .find(|dependency| dependency.instance() == instance)
        .map(|dependency| CodegenSymbolKey::Instance(dependency.instance().clone()))
        .ok_or_else(|| CodegenFactError::MissingHelperInstance(reference.clone()))
}

fn is_unit(compilation: &Compilation, ty: TypeId) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(false);
    };

    Ok(
        super::super::foreign::compiler_known_representation(compilation, *definition)
            == Some(RepresentationRole::Unit),
    )
}

fn align_to(value: u64, alignment: NonZeroU64) -> Option<u64> {
    let mask = alignment.get().checked_sub(1)?;

    value.checked_add(mask).map(|value| value & !mask)
}

fn nonzero_width(width: u16) -> NonZeroU16 {
    NonZeroU16::new(width).unwrap_or(NonZeroU16::MIN)
}

pub(in crate::compilation) fn generated_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    category: &str,
    identity: &impl Hash,
) -> Result<BinarySymbolName, CodegenFactError> {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.codegen-symbol");
    category.hash(&mut hasher);
    identity.hash(&mut hasher);

    binary_symbol_name(target, linkage, category, hasher.finalize())
}

pub(in crate::compilation) fn generated_frame_symbol_name(
    target: &CodegenTarget,
    frame: bray_runtime_interface::ProtectedAsyncFrameId,
    operation: ProtectedFrameOperation,
) -> Result<BinarySymbolName, CodegenFactError> {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.protected-frame-symbol");
    frame.hash(&mut hasher);
    operation.hash(&mut hasher);

    binary_symbol_name(
        target,
        CodegenLinkage::Internal,
        operation.as_str(),
        hasher.finalize(),
    )
}

fn binary_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    category: &str,
    digest: [u8; 32],
) -> Result<BinarySymbolName, CodegenFactError> {
    let prefix = if linkage == CodegenLinkage::Private {
        target.symbols().private_prefix()
    } else {
        target.symbols().global_prefix()
    };

    let mut name = format!("{prefix}bray_{category}_");

    for byte in digest {
        let _ = write!(name, "{byte:02x}");
    }

    BinarySymbolName::try_new(name).ok_or(CodegenFactError::InvalidSymbolName)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_codegen::{
        CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenSymbolKey,
        CodegenTarget,
    };
    use bray_ir::{
        MirCleanupPhase, MirFrameReference, MirHelperReference, MirOperationKind,
        MirProjectionKind, MirRuntimeReference, MirUnitId,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole,
    };
    use bray_symbols::TypeData;
    use bray_testing::{test_mir_unit, test_mir_unit_with_declaration};

    use super::{dependency_symbol, direct_helper_symbol};
    use crate::compilation::CodegenFactError;
    use crate::compilation::product::specialization::ConcreteCodegenInstance;
    use crate::{CancellationToken, Compilation};

    #[test]
    fn generated_helpers_map_to_exact_runtime_and_frame_roles() {
        let owner = CodegenInstance::non_generic(test_mir_unit(1));
        let frame = ProtectedAsyncFrameId::new([7; 32]);

        let runtime = |role| {
            CodegenSymbolKey::Runtime(MirRuntimeReference::new(
                role,
                owner.key().target().runtime_abi(),
            ))
        };

        let cases = [
            (
                MirHelperReference::BeginGenerator,
                runtime(RuntimeAbiRole::GeneratorBegin),
            ),
            (
                MirHelperReference::PushGenerator,
                runtime(RuntimeAbiRole::GeneratorPush),
            ),
            (
                MirHelperReference::FinishGenerator,
                runtime(RuntimeAbiRole::GeneratorFinish),
            ),
            (
                MirHelperReference::PanicReport,
                runtime(RuntimeAbiRole::PanicReportConstruction),
            ),
            (
                MirHelperReference::MoveInactiveFrame(MirFrameReference::Known(frame)),
                CodegenSymbolKey::ProtectedFrame {
                    frame,
                    operation: ProtectedFrameOperation::MoveBeforeStart,
                },
            ),
            (
                MirHelperReference::MoveInactiveFrame(MirFrameReference::Erased),
                runtime(RuntimeAbiRole::InactiveFrameMove),
            ),
            (
                MirHelperReference::ComposeAwaitedFrame(MirFrameReference::Erased),
                runtime(RuntimeAbiRole::AwaitedFrameComposition),
            ),
            (
                MirHelperReference::CommitAwaitedCompletion(
                    MirFrameReference::Known(frame),
                ),
                CodegenSymbolKey::ProtectedFrame {
                    frame,
                    operation: ProtectedFrameOperation::CompletionMove,
                },
            ),
            (
                MirHelperReference::CommitAwaitedCompletion(
                    MirFrameReference::Erased,
                ),
                runtime(RuntimeAbiRole::FrameCompletionMove),
            ),
            (
                MirHelperReference::DestroyTerminalTask,
                runtime(RuntimeAbiRole::TaskDestruction),
            ),
        ];

        for (reference, expected) in cases {
            assert_eq!(direct_helper_symbol(&owner, &reference), Some(expected));
        }
    }

    #[test]
    fn lifecycle_helpers_use_distinct_type_aware_instance_dependencies() {
        let compilation = crate::test_support::compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let leaf = values
            .intern_type(TypeData::tuple([]))
            .expect("leaf type must intern");

        let aggregate = values
            .intern_type(TypeData::tuple([leaf]))
            .expect("aggregate type must intern");

        let references = [
            MirHelperReference::Finalize(aggregate),
            MirHelperReference::Destroy(aggregate),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty: aggregate,
            },
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty: aggregate,
            },
        ];

        let dependencies = references
            .iter()
            .cloned()
            .map(|reference| {
                compilation
                    .concrete_codegen_lifecycle(reference, &target)
                    .expect("lifecycle instance must realize")
            })
            .collect::<Vec<_>>();

        assert_eq!(
            dependencies
                .iter()
                .map(ConcreteCodegenInstance::key)
                .collect::<BTreeSet<_>>()
                .len(),
            references.len()
        );

        let owner_mir = test_mir_unit(2);

        let owner = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&owner_mir),
            owner_mir,
            dependencies
                .iter()
                .map(|dependency| {
                    CodegenInstanceDependency::definition(dependency.key().clone())
                }),
        )
        .expect("generated lifecycle dependencies must validate");

        for (reference, dependency) in references.into_iter().zip(dependencies) {
            assert_eq!(
                dependency_symbol(&owner, dependency.key(), &reference),
                Ok(CodegenSymbolKey::Instance(dependency.key().clone()))
            );
        }
    }

    #[test]
    fn lifecycle_identity_is_stable_and_payload_remains_compilation_local() {
        let first = crate::test_support::compilation("module app; func main() {}");
        let second = crate::test_support::compilation("module app; func main() {}");

        let first_values = first
            .semantic_value_store()
            .expect("first semantic values must resolve");

        let first_leaf = first_values
            .intern_type(TypeData::Error)
            .expect("first leaf type must intern");

        let first_type = first_values
            .intern_type(TypeData::tuple([first_leaf]))
            .expect("first aggregate type must intern");

        let second_values = second
            .semantic_value_store()
            .expect("second semantic values must resolve");

        let _ = second_values
            .intern_type(TypeData::tuple([]))
            .expect("unrelated type must intern");

        let second_leaf = second_values
            .intern_type(TypeData::Error)
            .expect("second leaf type must intern");

        let second_type = second_values
            .intern_type(TypeData::tuple([second_leaf]))
            .expect("second aggregate type must intern");

        let first = first
            .concrete_codegen_lifecycle(
                MirHelperReference::Destroy(first_type),
                &codegen_target(&first),
            )
            .expect("first lifecycle instance must realize");

        let second = second
            .concrete_codegen_lifecycle(
                MirHelperReference::Destroy(second_type),
                &codegen_target(&second),
            )
            .expect("second lifecycle instance must realize");

        assert_eq!(first.key(), second.key());

        assert_eq!(
            first.generated_lifecycle_reference(),
            Some(&MirHelperReference::Destroy(first_type))
        );

        assert_eq!(
            second.generated_lifecycle_reference(),
            Some(&MirHelperReference::Destroy(second_type))
        );
    }

    #[test]
    fn lifecycle_payload_must_match_the_stable_key_role() {
        let compilation = crate::test_support::compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let ty = compilation
            .semantic_value_store()
            .expect("semantic values must resolve")
            .intern_type(TypeData::tuple([]))
            .expect("test type must intern");

        let finalize = compilation
            .concrete_codegen_lifecycle(
                MirHelperReference::Finalize(ty),
                &target,
            )
            .expect("finalization instance must realize");

        assert!(
            ConcreteCodegenInstance::try_generated_lifecycle(
                finalize.key().clone(),
                MirHelperReference::Destroy(ty),
            )
            .is_none()
        );
    }

    #[test]
    fn generated_destruction_composes_parts_in_reverse_order() {
        let compilation = crate::test_support::compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let leaf = values
            .intern_type(TypeData::tuple([]))
            .expect("leaf type must intern");

        let aggregate = values
            .intern_type(TypeData::tuple([leaf, leaf]))
            .expect("aggregate type must intern");

        let instance = compilation
            .concrete_codegen_lifecycle(
                MirHelperReference::Destroy(aggregate),
                &target,
            )
            .expect("destruction instance must realize");

        let reference = instance
            .generated_lifecycle_reference()
            .expect("generated lifecycle payload must be retained");

        let generated = compilation
            .codegen_generated_lifecycle_mir(
                instance.key(),
                reference,
                MirUnitId::new(77),
                &CancellationToken::new(),
            )
            .expect("represented-part destruction must generate");

        let operations = generated
            .operations()
            .iter()
            .map(|operation| operation.kind())
            .collect::<Vec<_>>();

        let expected = [
            ("finalize", 1),
            ("destroy", 1),
            ("finalize", 0),
            ("destroy", 0),
        ];

        assert_eq!(operations.len(), expected.len());

        for (operation, (kind, field)) in operations.into_iter().zip(expected) {
            let place = match operation {
                MirOperationKind::Finalize(place) if kind == "finalize" => place,
                MirOperationKind::Destroy(place) if kind == "destroy" => place,
                other => panic!("unexpected lifecycle operation: {other:?}"),
            };

            assert!(matches!(
                place.projections().last().map(|projection| projection.kind()),
                Some(MirProjectionKind::TupleField(actual)) if *actual == field
            ));
        }
    }

    #[test]
    fn declaration_helpers_require_the_exact_concrete_dependency() {
        let owner_mir = test_mir_unit(3);

        let dependency =
            CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(4, 5));

        let owner = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&owner_mir),
            owner_mir,
            [CodegenInstanceDependency::definition(dependency.clone())],
        )
        .expect("test helper dependency must validate");

        let reference = MirHelperReference::AnonymousCallable(
            match dependency.template() {
                bray_ir::MirUnitKey::Bound(unit) => unit.clone(),
                bray_ir::MirUnitKey::ExecutableHost(_)
                | bray_ir::MirUnitKey::GeneratedLifecycle(_)
                | bray_ir::MirUnitKey::ExternalCallable(_) => {
                    panic!("test dependency must be bound");
                }
            },
        );

        assert_eq!(
            dependency_symbol(&owner, &dependency, &reference),
            Ok(CodegenSymbolKey::Instance(dependency.clone()))
        );

        let missing =
            CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(6, 7));

        assert_eq!(
            dependency_symbol(&owner, &missing, &reference),
            Err(CodegenFactError::MissingHelperInstance(reference))
        );
    }

    fn codegen_target(compilation: &Compilation) -> CodegenTarget {
        compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("test codegen target must validate")
    }
}
