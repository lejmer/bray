use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use bray_binder::BindingQueryContext;
use bray_codegen::{
    CodegenCallSite, CodegenDebugLocation, CodegenHelperMapping, CodegenInstance,
    CodegenOperationMapping, CodegenSourceFile, CodegenSymbolKey, CodegenSymbolMapping,
    CodegenTarget, CodegenTypeMapping, CodegenUnit, demanded_callable_instance_for_call,
    demanded_debug_sources,
};
use bray_ir::{
    MirAsyncOperation, MirBlockKind, MirCallTarget, MirCleanupEdge, MirEdge, MirFrameInitializer,
    MirHelperReference, MirOperand, MirOperationId, MirOperationKind, MirPlace, MirProjection,
    MirProjectionKind, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnit, MirUnitBuilder,
    MirUnitId, MirUnitKey,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_source::LineIndex;
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableDefinitionId, SymbolKeyData, TypeAssociatedLifecycleSlot,
    TypeData, TypeId,
};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::specialization::{
    ConcreteCodegenCallee, ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use super::support::{
    codegen_source_file, dependency_symbol, direct_helper_symbol, helper_runtime_symbol,
    is_void_result, operation_result_type,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn codegen_debug_locations(
        &self,
        unit: &CodegenUnit,
    ) -> Result<Vec<CodegenDebugLocation>, CodegenPreparationError> {
        let mut sources = BTreeMap::new();

        let generated_file = CodegenSourceFile::try_new("generated.bray")
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut locations = Vec::new();

        for anchor in demanded_debug_sources(unit) {
            let debug_location = match &anchor {
                MirSourceAnchor::Source(origin) => {
                    let source_anchor = origin.source_anchor();
                    let syntax = source_anchor.syntax();

                    if !sources.contains_key(&syntax.source_id()) {
                        let source = self
                            .source(syntax.source_id())
                            .filter(|source| source.version() == source_anchor.source_version())
                            .ok_or(FactQueryError::InfrastructureFailure)?;

                        let index = LineIndex::new(source.text())
                            .map_err(|_| FactQueryError::InfrastructureFailure)?;

                        let file = codegen_source_file(source)?;

                        sources.insert(syntax.source_id(), (index, file));
                    }

                    let (index, file) = sources
                        .get(&syntax.source_id())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let location = index
                        .line_column(syntax.full_range().start())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let line = NonZeroU32::new(location.line())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let column = NonZeroU32::new(location.column())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    // The clone retains the shared immutable normalized path.
                    CodegenDebugLocation::new(anchor, file.clone(), line, column)
                }
                MirSourceAnchor::ImportedExecutable(_)
                | MirSourceAnchor::ExecutableHost(_)
                | MirSourceAnchor::GeneratedLifecycle(_) => {
                    // The clone retains the shared immutable generated path.
                    CodegenDebugLocation::new(
                        anchor,
                        generated_file.clone(),
                        NonZeroU32::MIN,
                        NonZeroU32::MIN,
                    )
                }
            };

            locations.push(debug_location);
        }

        Ok(locations)
    }

    pub(super) fn codegen_operations(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenOperationMapping>, CodegenPreparationError> {
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
                            operation,
                            data.kind(),
                            operation_result_type(instance.mir(), data),
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

    pub(super) fn classify_codegen_symbols(
        &self,
        symbols: Vec<CodegenSymbolMapping>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenPreparationError> {
        let mut classified = Vec::with_capacity(symbols.len());
        let mut pending = BTreeSet::new();

        for symbol in symbols {
            let (key, name, linkage, signature, native_entry) = symbol.into_parts();

            let signature = self.classify_codegen_signature(
                signature,
                target,
                cancellation,
                mappings,
                &mut pending,
            )?;

            let mut symbol = CodegenSymbolMapping::new(key, name, linkage, signature);

            if let Some(native_entry) = native_entry {
                symbol = symbol.with_native_entry(native_entry);
            }

            classified.push(symbol);
        }

        Ok(classified)
    }

    pub(super) fn codegen_helper(
        &self,
        owner: &CodegenInstance,
        operation_id: MirOperationId,
        operation: &MirOperationKind,
        operation_result_type: Option<TypeId>,
        owner_realization: &ConcreteCodegenInstance,
        reference: MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenHelperMapping, CodegenPreparationError> {
        let concrete_reference =
            self.concrete_codegen_helper_reference(owner_realization, &reference, cancellation)?;

        if concrete_reference.lifecycle_type().is_some()
            && self.codegen_lifecycle_is_trivial(&concrete_reference, cancellation)?
        {
            return Ok(CodegenHelperMapping::lowered(reference));
        }

        if let Some(symbol) = direct_helper_symbol(owner, &reference) {
            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        if let Some(dependency) = self.concrete_codegen_helper_dependency(
            owner_realization,
            operation,
            operation_result_type,
            &reference,
            target,
            cancellation,
        )? {
            return dependency_symbol(owner, dependency.key(), &reference)
                .map(|symbol| CodegenHelperMapping::new(reference, symbol));
        }

        if matches!(reference, MirHelperReference::CreateFrame(_)) {
            let symbol = self.frame_creation_symbol(
                owner,
                owner_realization,
                operation_id,
                operation,
                &reference,
                target,
                cancellation,
            )?;

            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        Err(CodegenPreparationError::MissingHelperInstance(reference))
    }

    pub(in crate::compilation::product) fn concrete_codegen_helper_dependency(
        &self,
        owner: &ConcreteCodegenInstance,
        operation: &MirOperationKind,
        operation_result_type: Option<TypeId>,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConcreteCodegenInstance>, CodegenPreparationError> {
        let concrete_reference =
            self.concrete_codegen_helper_reference(owner, reference, cancellation)?;

        let dependency = match &concrete_reference {
            MirHelperReference::AnonymousCallable(unit) => {
                let callable_type =
                    operation_result_type.ok_or(FactQueryError::InfrastructureFailure)?;

                self.concrete_codegen_anonymous_callable(owner, unit, callable_type)?
            }
            MirHelperReference::DeclaredCallable(callable) => self.concrete_codegen_callable_data(
                owner,
                &callable.instance(),
                target,
                cancellation,
            )?,
            MirHelperReference::CallableDefault(provider) => {
                let MirOperationKind::Call(call) = operation else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                let MirCallTarget::Direct(callable) = call.target() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                let callee = self.concrete_codegen_callable_data(
                    owner,
                    &callable.instance(),
                    target,
                    cancellation,
                )?;

                self.concrete_codegen_runtime_default(
                    &callee,
                    (*provider).into(),
                    reference,
                    cancellation,
                )?
            }
            MirHelperReference::ConstructionDefault(provider) => self
                .concrete_codegen_runtime_default(
                    owner,
                    provider.symbol(),
                    reference,
                    cancellation,
                )?,
            MirHelperReference::TypeForm(callable) | MirHelperReference::Conversion(callable) => {
                self.concrete_codegen_callable_data(owner, callable, target, cancellation)?
            }
            MirHelperReference::StandardLibrary(helper) => {
                self.concrete_standard_library_helper(*helper, target, cancellation)?
            }
            MirHelperReference::Finalize(_)
            | MirHelperReference::StaticFinalize(_)
            | MirHelperReference::Destroy(_)
            | MirHelperReference::Cleanup { .. } => {
                if self.codegen_lifecycle_is_trivial(&concrete_reference, cancellation)? {
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

    pub(super) fn concrete_codegen_helper_reference(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<MirHelperReference, CodegenPreparationError> {
        let substitution = owner.substitution();

        Ok(match reference {
            MirHelperReference::Finalize(ty) => MirHelperReference::Finalize(
                self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            ),
            MirHelperReference::StaticFinalize(ty) => MirHelperReference::StaticFinalize(
                self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            ),
            MirHelperReference::Destroy(ty) => MirHelperReference::Destroy(
                self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            ),
            MirHelperReference::Cleanup { phase, ty } => MirHelperReference::Cleanup {
                phase: *phase,
                ty: self.concrete_codegen_type(*ty, substitution, Some(owner), cancellation)?,
            },
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::DeclaredCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::StandardLibrary(_)
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

    pub(super) fn concrete_codegen_runtime_default(
        &self,
        owner: &ConcreteCodegenInstance,
        provider: AnySymbolId,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        if let Some(unit) = self.runtime_default_unit(provider)? {
            return self.concrete_codegen_bound_helper(owner, unit);
        }

        let binding_context = self.binding_context(cancellation)?;

        let key = binding_context
            .symbol_key(provider)
            .map_err(super::super::super::binder::binding_query_error)?
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        if !matches!(key.data(), SymbolKeyData::External(_)) {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        }

        let Some(address) = binding_context
            .imported_semantic_address(provider)
            .map_err(super::super::super::binder::binding_query_error)?
        else {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        };

        let template = self.imported_executable_template_with_cancellation(
            crate::fact::ImportedExecutableTemplateAddress::root(address),
            cancellation,
        )?;

        if template.value().is_none() {
            return Err(CodegenPreparationError::Diagnostics(
                template.diagnostics().clone(),
            ));
        }

        ConcreteCodegenInstance::imported_runtime_default(owner, provider)
            .ok_or_else(|| FactQueryError::InfrastructureFailure.into())
    }

    pub(super) fn runtime_default_unit(
        &self,
        provider: AnySymbolId,
    ) -> Result<Option<bray_bound_tree::BoundUnitKey>, CodegenPreparationError> {
        let symbols = self.symbol_graph()?;

        let Some(provider) = symbols.symbol_key(provider) else {
            return Ok(None);
        };

        Ok(self.declared_unit_keys()?.into_iter().find(|unit| {
            unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault
                && unit.declared_owner() == provider
        }))
    }

    pub(in crate::compilation::product) fn concrete_codegen_callable_defaults(
        &self,
        callable: CallableDefinitionId,
        owner: &ConcreteCodegenInstance,
    ) -> Result<Vec<ConcreteCodegenInstance>, CodegenPreparationError> {
        let symbols = self.symbol_graph()?;

        let (parameters, _) = symbols
            .callable_parameters_and_receiver(callable.callable_symbol())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut defaults = Vec::new();

        for parameter in parameters {
            let Some(provider) = symbols
                .callable_parameter(*parameter)
                .and_then(bray_symbols::CallableParameterSymbol::default_provider)
            else {
                continue;
            };

            let unit = self
                .runtime_default_unit(provider.into())?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            defaults.push(self.concrete_codegen_bound_helper(owner, unit)?);
        }

        Ok(defaults)
    }

    pub(super) fn frame_creation_symbol(
        &self,
        owner: &CodegenInstance,
        owner_realization: &ConcreteCodegenInstance,
        operation_id: MirOperationId,
        operation: &MirOperationKind,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenSymbolKey, CodegenPreparationError> {
        let MirOperationKind::Async(MirAsyncOperation::CreateFrame { initializer, .. }) = operation
        else {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        };

        match initializer {
            MirFrameInitializer::Callable(call) => match call.target() {
                MirCallTarget::Direct(_) => {
                    let demand = demanded_callable_instance_for_call(
                        CodegenCallSite::Operation(operation_id),
                        call,
                    )
                    .ok_or_else(|| {
                        CodegenPreparationError::MissingHelperInstance(reference.clone())
                    })?;

                    let ConcreteCodegenCallee::Instance(dependency) = self
                        .concrete_codegen_callee(
                            owner_realization,
                            &demand,
                            target,
                            cancellation,
                        )?
                    else {
                        return Err(CodegenPreparationError::MissingHelperInstance(
                            reference.clone(),
                        ));
                    };

                    dependency_symbol(owner, dependency.key(), reference)
                }
                MirCallTarget::Indirect { .. } => {
                    Ok(helper_runtime_symbol(owner, RuntimeAbiRole::FrameCreation))
                }
                MirCallTarget::Runtime(_) => Err(CodegenPreparationError::MissingHelperInstance(
                    reference.clone(),
                )),
            },
            MirFrameInitializer::TaskObservation { .. } => Ok(helper_runtime_symbol(
                owner,
                RuntimeAbiRole::TaskObservationCreation,
            )),
        }
    }

    pub(in crate::compilation) fn codegen_generated_lifecycle_mir(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        reference: &MirHelperReference,
        unit: MirUnitId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let MirUnitKey::GeneratedLifecycle(key) = instance.template() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        if Some(key.role()) != bray_ir::MirGeneratedLifecycleRole::from_reference(reference) {
            return Err(FactQueryError::InfrastructureFailure.into());
        }

        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        let values = self.semantic_value_store()?;

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: ty,
            })
            .map_err(FactQueryError::SemanticValueStore)?;

        let source = MirSourceAnchor::generated_lifecycle(reference.clone());

        let mut builder = MirUnitBuilder::for_generated_lifecycle(
            unit,
            instance.template().clone(),
            reference.clone(),
            instance.target().clone(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter(0), pointer)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let place = MirPlace::new(
            storage,
            [MirProjection::new(
                MirProjectionKind::Dereference,
                pointer,
                ty,
            )],
            ty,
        );

        match reference {
            MirHelperReference::Cleanup { phase, .. } => {
                let broadcast = builder
                    .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

                let lifecycle = builder
                    .push_block(source.clone(), MirBlockKind::LifecycleResolution)
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

                builder
                    .set_terminator(
                        entry,
                        source.clone(),
                        MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                            bray_ir::MirCleanupPhase::TaskCancellation,
                            MirEdge::new(broadcast, []),
                        )),
                    )
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

                let (broadcast_end, lifecycle_end) = match phase {
                    bray_ir::MirCleanupPhase::TaskCancellation => (
                        self.push_generated_lifecycle_operations(
                            &mut builder,
                            broadcast,
                            &source,
                            reference,
                            place,
                            instance.target().runtime_abi(),
                            cancellation,
                        )?,
                        lifecycle,
                    ),
                    bray_ir::MirCleanupPhase::LifecycleResolution => (
                        broadcast,
                        self.push_generated_lifecycle_operations(
                            &mut builder,
                            lifecycle,
                            &source,
                            reference,
                            place,
                            instance.target().runtime_abi(),
                            cancellation,
                        )?,
                    ),
                };

                builder
                    .set_terminator(
                        broadcast_end,
                        source.clone(),
                        MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                            bray_ir::MirCleanupPhase::LifecycleResolution,
                            MirEdge::new(lifecycle, []),
                        )),
                    )
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

                builder
                    .set_terminator(lifecycle_end, source, MirTerminatorKind::Return(None))
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;
            }
            MirHelperReference::StaticFinalize(ty) => {
                let return_value = if self.push_compiler_known_lifecycle_operations(
                    &mut builder,
                    entry,
                    &source,
                    reference,
                    &place,
                    instance.target().runtime_abi(),
                    cancellation,
                )? {
                    None
                } else if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Finalizer,
                    cancellation,
                )? {
                    let returns_void = callable.3 == bray_symbols::CallableExecution::Synchronous
                        && is_void_result(self, callable.2)?;

                    let value = self.push_static_finalizer_call(
                        &mut builder,
                        entry,
                        &source,
                        place,
                        callable,
                    )?;

                    (!returns_void).then_some(MirOperand::Value(value))
                } else {
                    None
                };

                builder
                    .set_terminator(entry, source, MirTerminatorKind::Return(return_value))
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;
            }
            MirHelperReference::Finalize(_) | MirHelperReference::Destroy(_) => {
                let end = self.push_generated_lifecycle_operations(
                    &mut builder,
                    entry,
                    &source,
                    reference,
                    place,
                    instance.target().runtime_abi(),
                    cancellation,
                )?;

                builder
                    .set_terminator(end, source, MirTerminatorKind::Return(None))
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;
            }
            MirHelperReference::AnonymousCallable(_)
            | MirHelperReference::DeclaredCallable(_)
            | MirHelperReference::CallableDefault(_)
            | MirHelperReference::ConstructionDefault(_)
            | MirHelperReference::TypeForm(_)
            | MirHelperReference::Conversion(_)
            | MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::StandardLibrary(_)
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => {
                return Err(CodegenPreparationError::MissingHelperInstance(
                    reference.clone(),
                ));
            }
        }

        builder
            .finish(entry)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)
    }
}
