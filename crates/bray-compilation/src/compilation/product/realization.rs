// rust-style: allow(module-too-large, reason = "the mapping tables share one recursive realization context and must remain auditable together")

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use bray_base::StableDigestHasher;
use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_codegen::{
    CodegenCallableMapping, CodegenCallableSignature, CodegenConstantMapping,
    CodegenConstantTermMapping, CodegenDebugLocation, CodegenDefinitionVisibility,
    CodegenFieldLayout, CodegenHelperMapping, CodegenIndirectParameterKind, CodegenInstance,
    CodegenInstanceTypeMapping, CodegenLinkage, CodegenMappings, CodegenOperationMapping,
    CodegenParameterMapping, CodegenPartitionCompatibility, CodegenResultMapping,
    CodegenSourceFile, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenTerminatorMapping, CodegenTypeKind, CodegenTypeMapping, CodegenUnionVariantLayout,
    CodegenUnit, CodegenValueAttribute, TargetAddressSpaceKind, child_constants,
    demanded_callable_instances, demanded_callable_instances_for_mir, demanded_constant_terms,
    demanded_constants, demanded_debug_sources, mapped_runtime_references,
};
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_diagnostics::DiagnosticBag;
use bray_ir::{
    MirAsyncOperation, MirBlockKind, MirCall, MirCallTarget, MirCallableReference, MirCleanupEdge,
    MirEdge, MirFrameInitializer, MirFrameReference, MirGeneratorOperation, MirHelperReference,
    MirMemoryOperation, MirOperand, MirOperation, MirOperationKind, MirPlace, MirProjection,
    MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind,
    MirTerminatorKind, MirUnit, MirUnitBuilder, MirUnitId, MirUnitKey, MirUnitKind,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ProtectedFrameOperation, RuntimeAbiRole,
};
use bray_source::{LineIndex, SourceSnapshot};
use bray_symbols::{
    AnySymbolId, BorrowKind, CallableAbi, CallableDefinitionId, CallableExecution,
    CallableParameterDefaultQuery, CallableParameterDefaultValue, CallableParameterSignature,
    CallableSignature, CallableSignatureQuery, ConstantTermData, ConstantValueKind,
    DeclaredLayoutMode, ForeignCallableDirection, GenericArgument, GenericSubstitutionId,
    ImplementationCoherenceQuery, ImplementationSymbolId, NamedTypeSymbolId, PackageIdentity,
    ReceiverMode, ReceiverParameterSignature, RuntimeDefaultProviderInput, SelfTypeContext,
    SemanticValueStore, StructFieldDefaultQuery, StructFieldDefaultValue, StructSymbolId,
    SymbolKey, SymbolKeyData, SymbolQueryRequest, TypeAssociatedLifecycleSlot, TypeData, TypeId,
    UnionPayloadDefaultValue, UnionPayloadFieldDefaultQuery, UnionPayloadFieldTypeQuery,
};
use bray_target::{
    TargetAtomicRepresentation, TargetLayoutContract, TargetScalarKind, TargetValueLayout,
};

use super::super::CodegenPreparationError;
use super::super::Compilation;
use super::super::binder::CompilationBindingContext;
use super::super::checker::CompilationCheckerContext;
use super::super::substitution::{named_type, substitution_for_owner};
use super::specialization::{
    ConcreteCodegenCallee, ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn concrete_codegen_dependencies_for_mir(
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
        include_debug_locations: bool,
        cancellation: &CancellationToken,
    ) -> Result<CodegenMappings, CodegenPreparationError> {
        let operations = self.codegen_operations(unit, target, reachability, cancellation)?;

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

        CodegenMappings::try_new(
            unit,
            target,
            types,
            instance_type_mappings,
            symbols,
            constants,
            constant_terms,
            callables,
            operations,
            terminators,
            debug_locations,
        )
        .map_err(CodegenPreparationError::InvalidMappings)
    }

    fn codegen_debug_locations(
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

    fn codegen_operations(
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

    fn classify_codegen_symbols(
        &self,
        symbols: Vec<CodegenSymbolMapping>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenPreparationError> {
        let mut classified = Vec::with_capacity(symbols.len());
        let mut pending = BTreeSet::new();

        for symbol in symbols {
            let (key, name, linkage, signature) = symbol.into_parts();

            let signature = self.classify_codegen_signature(
                signature,
                target,
                cancellation,
                mappings,
                &mut pending,
            )?;

            classified.push(CodegenSymbolMapping::new(key, name, linkage, signature));
        }

        Ok(classified)
    }

    fn codegen_helper(
        &self,
        owner: &CodegenInstance,
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
                operation,
                &reference,
                target,
                cancellation,
            )?;

            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        Err(CodegenPreparationError::MissingHelperInstance(reference))
    }

    pub(super) fn concrete_codegen_helper_dependency(
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
            MirHelperReference::Finalize(_)
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

    fn concrete_codegen_helper_reference(
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

    fn concrete_codegen_runtime_default(
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
            .map_err(super::super::binder::binding_query_error)?
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        if !matches!(key.data(), SymbolKeyData::External(_)) {
            return Err(CodegenPreparationError::MissingHelperInstance(
                reference.clone(),
            ));
        }

        let Some(address) = binding_context
            .imported_semantic_address(provider)
            .map_err(super::super::binder::binding_query_error)?
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

        Ok(ConcreteCodegenInstance::imported_runtime_default(
            owner, provider,
        ))
    }

    fn runtime_default_unit(
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

    pub(super) fn concrete_codegen_callable_defaults(
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

    fn frame_creation_symbol(
        &self,
        owner: &CodegenInstance,
        owner_realization: &ConcreteCodegenInstance,
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
            MirFrameInitializer::TaskObservation { .. } => Ok(helper_runtime_symbol(
                owner,
                RuntimeAbiRole::JoinRegistration,
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

    fn push_generated_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        if self.push_compiler_known_lifecycle_operations(
            builder,
            block,
            source,
            reference,
            &place,
            runtime_abi,
            cancellation,
        )? {
            return Ok(block);
        }

        match reference {
            MirHelperReference::Finalize(ty) => {
                if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Finalizer,
                    cancellation,
                )? {
                    self.push_lifecycle_call(builder, block, source, place, callable)?;
                }
            }
            MirHelperReference::Destroy(ty) => {
                if let Some(callable) = self.lifecycle_callable(
                    *ty,
                    TypeAssociatedLifecycleSlot::Destructor,
                    cancellation,
                )? {
                    self.push_lifecycle_call(builder, block, source, place.clone(), callable)?;
                }

                return self.push_represented_lifecycle_operations(
                    builder,
                    block,
                    source,
                    bray_ir::MirGeneratedLifecycleRole::Destroy,
                    place,
                    runtime_abi,
                    cancellation,
                );
            }
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                ..
            } => {
                return self.push_represented_lifecycle_operations(
                    builder,
                    block,
                    source,
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                    ),
                    place,
                    runtime_abi,
                    cancellation,
                );
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
            | MirHelperReference::DeclaredCallable(_)
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
                return Err(CodegenPreparationError::MissingHelperInstance(
                    reference.clone(),
                ));
            }
        }

        Ok(block)
    }

    fn push_represented_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(place.ty())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                substitution,
            } => self.push_union_lifecycle_operations(
                builder,
                block,
                source,
                role,
                place,
                *union,
                *substitution,
                cancellation,
            ),
            TypeData::Nullable(target) => self
                .push_nullable_lifecycle_operations(builder, block, source, role, place, *target),
            TypeData::Generator(element) => {
                let operation = match role {
                    bray_ir::MirGeneratedLifecycleRole::Destroy => MirGeneratorOperation::Destroy {
                        destination: place,
                        element: *element,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorDestruction,
                            runtime_abi,
                        ),
                    },
                    bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                    ) => MirGeneratorOperation::CleanupBroadcast {
                        destination: place,
                        element: *element,
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::GeneratorCleanupBroadcast,
                            runtime_abi,
                        ),
                    },
                    bray_ir::MirGeneratedLifecycleRole::Finalize
                    | bray_ir::MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::LifecycleResolution,
                    ) => return Err(FactQueryError::InfrastructureFailure.into()),
                };

                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Generator(operation),
                )?;

                Ok(block)
            }
            TypeData::OwnedIndirection { storage, target } => self
                .push_owned_indirection_lifecycle_operations(
                    builder,
                    block,
                    source,
                    role,
                    place,
                    *storage,
                    *target,
                    cancellation,
                ),
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Slice(_)
            | TypeData::TraitView(_) => Err(CodegenPreparationError::UnsupportedType(place.ty())),
            TypeData::Named { .. } | TypeData::Tuple(_) | TypeData::Array { .. } => {
                let children = self.lifecycle_children(place, cancellation)?;

                self.push_child_lifecycle_operations(builder, block, source, role, children)?;

                Ok(block)
            }
            TypeData::Borrow { .. } | TypeData::Callable(_) => {
                Err(FactQueryError::InfrastructureFailure.into())
            }
        }
    }

    fn push_nullable_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        target: TypeId,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let kind = lifecycle_operation_block_kind(role)?;

        let present = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let absent = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::PatternBranch {
                    subject: MirOperand::Copy(place.clone()),
                    predicate: bray_ir::MirPatternPredicate::NullablePresent,
                    matched: MirEdge::new(present, []),
                    unmatched: MirEdge::new(absent, []),
                },
            )
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let child = projected_lifecycle_place(&place, MirProjectionKind::NullableValue, target);

        self.push_child_lifecycle_operations(builder, present, source, role, [child])?;

        for branch in [present, absent] {
            builder
                .set_terminator(
                    branch,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(merge, [])),
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;
        }

        Ok(merge)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "union lifecycle dispatch keeps its checked type and MIR context explicit"
    )]
    fn push_union_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        union: bray_symbols::UnionSymbolId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let kind = lifecycle_operation_block_kind(role)?;

        let merge = builder
            .push_block(source.clone(), kind)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let binding_context = self.binding_context(cancellation)?;

        let union = binding_context
            .union(union)
            .map_err(super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let mut current = block;

        for variant in union.variants() {
            let variant_record = binding_context
                .union_variant(*variant)
                .map_err(super::super::binder::binding_query_error)?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let matched = builder
                .push_block(source.clone(), kind)
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            let unmatched = builder
                .push_block(source.clone(), kind)
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            builder
                .set_terminator(
                    current,
                    source.clone(),
                    MirTerminatorKind::PatternBranch {
                        subject: MirOperand::Copy(place.clone()),
                        predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(*variant),
                        matched: MirEdge::new(matched, []),
                        unmatched: MirEdge::new(unmatched, []),
                    },
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            let children = variant_record
                .payload_fields()
                .iter()
                .map(|field| {
                    let template = binding_context
                        .resolve_symbol_query(
                            SymbolQueryRequest::<UnionPayloadFieldTypeQuery>::new(*field),
                        )
                        .map_err(super::super::binder::binding_query_error)?;

                    let ty =
                        self.resolve_codegen_type(template.value(), substitution, cancellation)?;

                    Ok(projected_lifecycle_place(
                        &place,
                        MirProjectionKind::ActiveUnionPayloadField {
                            variant: *variant,
                            field: *field,
                        },
                        ty,
                    ))
                })
                .collect::<Result<Vec<_>, FactQueryError>>()?;

            self.push_child_lifecycle_operations(builder, matched, source, role, children)?;

            builder
                .set_terminator(
                    matched,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(merge, [])),
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

            current = unmatched;
        }

        let unmatched = if kind == MirBlockKind::CleanupBroadcast {
            MirTerminatorKind::Goto(MirEdge::new(merge, []))
        } else {
            MirTerminatorKind::Unreachable
        };

        builder
            .set_terminator(current, source.clone(), unmatched)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        Ok(merge)
    }

    fn push_child_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        children: impl IntoIterator<Item = MirPlace>,
    ) -> Result<(), CodegenPreparationError> {
        for child in children.into_iter().collect::<Vec<_>>().into_iter().rev() {
            match role {
                bray_ir::MirGeneratedLifecycleRole::Destroy => {
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
                bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => {
                    self.push_lifecycle_operation(
                        builder,
                        block,
                        source,
                        MirOperationKind::Cleanup {
                            phase,
                            place: child,
                        },
                    )?;
                }
                bray_ir::MirGeneratedLifecycleRole::Finalize => {
                    return Err(FactQueryError::InfrastructureFailure.into());
                }
            }
        }

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "owned lifecycle realization keeps the storage policy and target explicit"
    )]
    fn push_owned_indirection_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
        storage: TypeId,
        target: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirBlockId, CodegenPreparationError> {
        let storage_place =
            projected_lifecycle_place(&place, MirProjectionKind::OwnedStorage, storage);

        let target_place = self.storage_target_place(
            builder,
            block,
            source,
            storage_place.clone(),
            storage,
            target,
            cancellation,
        )?;

        match role {
            bray_ir::MirGeneratedLifecycleRole::Destroy => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Finalize(target_place),
                )?;

                self.push_storage_lifecycle_call(
                    builder,
                    block,
                    source,
                    storage_place.clone(),
                    storage,
                    target,
                    "StorageDestroy",
                    Some(BorrowKind::Mutable),
                    cancellation,
                )?;

                self.push_storage_lifecycle_call(
                    builder,
                    block,
                    source,
                    storage_place,
                    storage,
                    target,
                    "StorageRelease",
                    None,
                    cancellation,
                )?;
            }
            bray_ir::MirGeneratedLifecycleRole::Cleanup(phase) => {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Cleanup {
                        phase,
                        place: target_place,
                    },
                )?;
            }
            bray_ir::MirGeneratedLifecycleRole::Finalize => {
                return Err(FactQueryError::InfrastructureFailure.into());
            }
        }

        Ok(block)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "storage projection retains the selected policy and target type"
    )]
    fn storage_target_place(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<MirPlace, CodegenPreparationError> {
        let borrowed = self.push_storage_lifecycle_call(
            builder,
            block,
            source,
            storage_place,
            storage,
            target,
            "StorageBorrowMut",
            Some(BorrowKind::Mutable),
            cancellation,
        )?;

        let values = self.semantic_value_store()?;

        let pointer = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let temporary = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, pointer)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        let temporary_place = MirPlace::new(temporary, [], pointer);

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: temporary_place.clone(),
                value: MirOperand::Value(borrowed),
            },
        )?;

        Ok(projected_lifecycle_place(
            &temporary_place,
            MirProjectionKind::Dereference,
            target,
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "storage protocol calls retain exact policy, target, and MIR placement"
    )]
    fn push_storage_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        storage_place: MirPlace,
        storage: TypeId,
        target: TypeId,
        member: &str,
        borrow: Option<BorrowKind>,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirValueId, CodegenPreparationError> {
        let member = CompilerKnownDeclarationKey::try_new(member)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (callable, signature) =
            self.storage_lifecycle_callable(storage, target, &member, cancellation)?;

        let [parameter] = signature.parameters() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let argument = if let Some(kind) = borrow {
            let value = builder
                .push_operation(
                    block,
                    source.clone(),
                    MirOperationKind::Borrow {
                        kind,
                        place: storage_place,
                    },
                    Some(parameter.ty()),
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
                .result()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            MirOperand::Value(value)
        } else {
            MirOperand::Move(storage_place)
        };

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(callable),
                    bray_bound_tree::BoundCallResult::Immediate(signature.result()),
                    [argument],
                    [],
                )),
                Some(signature.result()),
            )
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
            .result()
            .ok_or(FactQueryError::InfrastructureFailure.into())
    }

    fn storage_lifecycle_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
        cancellation: &CancellationToken,
    ) -> Result<(MirCallableReference, CallableSignature), CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let selected = super::super::operation::selected_storage_callable(
            self,
            &binding_context,
            storage,
            target,
            member,
            cancellation,
        )?;

        if selected.diagnostics().has_errors() {
            return Err(CodegenPreparationError::UnsupportedType(storage));
        }

        let Some((_, _, callable, signature)) = selected.value() else {
            return Err(CodegenPreparationError::UnsupportedType(storage));
        };

        let values = self.semantic_value_store()?;

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        // The generated MIR owns this Arc-backed signature after releasing the query result.
        Ok((
            MirCallableReference::new(*callable, callable_type.abi()),
            signature.clone(),
        ))
    }

    fn push_compiler_known_lifecycle_operations(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        reference: &MirHelperReference,
        place: &MirPlace,
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        let Some(ty) = reference.lifecycle_type() else {
            return Ok(false);
        };

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok(false);
        };

        if let MirHelperReference::Destroy(_) = reference
            && let Some(element) =
                self.imported_raw_buffer_element(*definition, *substitution, cancellation)?
        {
            let borrowed = values
                .intern_type(TypeData::Borrow {
                    kind: BorrowKind::Mutable,
                    target: ty,
                })
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let buffer = builder
                .push_operation(
                    block,
                    source.clone(),
                    MirOperationKind::Borrow {
                        kind: BorrowKind::Mutable,
                        place: place.clone(),
                    },
                    Some(borrowed),
                )
                .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
                .result()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            self.push_lifecycle_operation(
                builder,
                block,
                source,
                MirOperationKind::Memory(MirMemoryOperation::new(
                    bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { element },
                    [MirOperand::Value(buffer)],
                    [borrowed],
                    None,
                )),
            )?;

            return Ok(true);
        }

        let Some(role) = super::super::foreign::compiler_known_representation(self, *definition)
        else {
            return Ok(false);
        };

        if role == RepresentationRole::String {
            if matches!(
                reference,
                MirHelperReference::Destroy(_)
                    | MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
            ) {
                self.push_lifecycle_operation(
                    builder,
                    block,
                    source,
                    MirOperationKind::Text(bray_ir::MirTextOperation::new(
                        bray_ir::MirTextOperationKind::Release,
                        [MirOperand::Move(place.clone())],
                        [ty],
                        None,
                    )),
                )?;
            }

            return Ok(true);
        }

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
                    MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                        task: MirOperand::Move(place.clone()),
                    }),
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
                    MirOperationKind::Async(MirAsyncOperation::RequestTaskCancellation {
                        task: MirOperand::Move(place.clone()),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::TaskCancellationRequest,
                            runtime_abi,
                        ),
                    }),
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
                    MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                        task: MirOperand::Move(place.clone()),
                    }),
                )?;
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

        Ok(true)
    }

    fn push_lifecycle_operation(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<(), CodegenPreparationError> {
        builder
            .push_operation(block, source.clone(), operation, None)
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        Ok(())
    }

    fn push_lifecycle_call(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: (MirCallableReference, TypeId, TypeId),
    ) -> Result<(), CodegenPreparationError> {
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
                        MirOperationKind::Borrow { kind: *kind, place },
                        Some(receiver),
                    )
                    .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?
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
            .map_err(CodegenPreparationError::InvalidGeneratedLifecycleMir)?;

        Ok(())
    }

    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        cancellation: &CancellationToken,
    ) -> Result<Option<(MirCallableReference, TypeId, TypeId)>, CodegenPreparationError> {
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

        let callable =
            super::super::implementation::callable_instance(values, member, [*substitution])?;

        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(super::super::binder::binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature.value().callable_type(),
                signature.value().result(),
            ],
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
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let receiver_ty =
            self.concrete_codegen_type(receiver.ty(), Some(*substitution), None, cancellation)?;

        let receiver = receiver_codegen_type(values, receiver_ty, receiver.mode())?;

        let result = self.concrete_codegen_type(
            signature.result(),
            Some(*substitution),
            None,
            cancellation,
        )?;

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        Ok(Some((
            MirCallableReference::new(callable, callable_type.abi()),
            receiver,
            result,
        )))
    }

    fn lifecycle_children(
        &self,
        place: MirPlace,
        cancellation: &CancellationToken,
    ) -> Result<Vec<MirPlace>, CodegenPreparationError> {
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
                    return Err(CodegenPreparationError::UnsupportedType(place.ty()));
                }

                let binding_context = self.binding_context(cancellation)?;

                let structure = binding_context
                    .structure(*structure)
                    .map_err(super::super::binder::binding_query_error)?
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                structure
                    .fields()
                    .iter()
                    .map(|field| {
                        let template = binding_context
                            .resolve_symbol_query(SymbolQueryRequest::<
                                bray_symbols::StructFieldTypeQuery,
                            >::new(*field))
                            .map_err(super::super::binder::binding_query_error)?;

                        let ty = self.resolve_codegen_type(
                            template.value(),
                            *substitution,
                            cancellation,
                        )?;

                        Ok((
                            MirProjectionKind::Field(bray_ir::MirFieldReference::Struct(*field)),
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
                        .map_err(|_| CodegenPreparationError::LayoutOverflow(place.ty()))?;

                    Ok((MirProjectionKind::TupleField(index), ty))
                })
                .collect::<Result<Vec<_>, CodegenPreparationError>>()?,
            TypeData::Array { element, length } => {
                let length = closed_array_length(values, *length)?;

                let length = u32::try_from(length)
                    .map_err(|_| CodegenPreparationError::LayoutOverflow(place.ty()))?;

                (0..length)
                    .map(|index| (MirProjectionKind::ElementFromStart(index), *element))
                    .collect()
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
            | TypeData::OwnedIndirection { .. }
            | TypeData::Callable(_) => {
                return Err(CodegenPreparationError::UnsupportedType(place.ty()));
            }
        };

        Ok(children
            .into_iter()
            .map(|(kind, ty)| projected_lifecycle_place(&place, kind, ty))
            .collect())
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
    ) -> Result<Vec<CodegenSymbolMapping>, CodegenPreparationError> {
        let mut symbols = Vec::new();

        for instance in unit.instances() {
            let (name, linkage, signature) = match instance.mir().kind() {
                MirUnitKind::ExecutableHost(host) => (
                    host.native_entry().clone(),
                    CodegenLinkage::Export,
                    void_signature(CallableAbi::Bray),
                ),
                MirUnitKind::GeneratedLifecycle(reference) => {
                    let name = generated_instance_symbol_name(
                        target,
                        CodegenLinkage::Internal,
                        instance.key(),
                    )?;

                    let signature = self.generated_lifecycle_signature(reference)?;

                    (name, CodegenLinkage::Internal, signature)
                }
                MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                    let realization = reachability
                        .instance(instance.key())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let (boundary, linkage) =
                        self.codegen_instance_boundary(instance, roots, cancellation)?;

                    let name = match boundary {
                        Some(name) => name,
                        None => self.generated_callable_symbol_name(
                            target,
                            linkage,
                            realization,
                            cancellation,
                        )?,
                    };

                    let signature = self.codegen_instance_signature(realization, cancellation)?;

                    (name, linkage, signature)
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

            let name = match boundary {
                Some((name, _)) => name,
                None => {
                    self.generated_callable_symbol_name(target, linkage, realization, cancellation)?
                }
            };

            let signature = self.codegen_instance_signature(realization, cancellation)?;

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.clone()),
                name,
                linkage,
                signature,
            ));
        }

        for reference in codegen_runtime_references(unit, operations, &symbols) {
            let symbol_name = executable_host
                .and_then(|host| host.role_binding(reference.role()))
                .map(|binding| binding.symbol_name().clone())
                .or_else(|| {
                    bray_runtime_interface::native_runtime_role_symbol(reference.role())
                        .and_then(BinarySymbolName::try_new)
                })
                .ok_or(CodegenPreparationError::MissingRuntimeRole(
                    reference.role(),
                ))?;

            let signature = self.codegen_runtime_signature(reference.role())?;

            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Runtime(reference),
                symbol_name,
                CodegenLinkage::Import,
                signature,
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
                let Some(CodegenSymbolKey::ProtectedFrame { frame, operation }) = helper.symbol()
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

    pub(super) fn codegen_partition_compatibility(
        &self,
        instance: &CodegenInstance,
        product_package: &PackageIdentity,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<CodegenPartitionCompatibility, CodegenPreparationError> {
        let package =
            self.codegen_instance_package(instance.key(), product_package, cancellation)?;

        let (_, linkage) = self.codegen_instance_boundary(instance, roots, cancellation)?;

        let visibility = match linkage {
            CodegenLinkage::Private => CodegenDefinitionVisibility::Unit,
            CodegenLinkage::Internal | CodegenLinkage::LinkOnce | CodegenLinkage::Common => {
                CodegenDefinitionVisibility::Product
            }
            CodegenLinkage::External
            | CodegenLinkage::Weak
            | CodegenLinkage::Import
            | CodegenLinkage::Export => CodegenDefinitionVisibility::Public,
        };

        Ok(CodegenPartitionCompatibility::new(
            package, linkage, visibility,
        ))
    }

    fn codegen_instance_boundary(
        &self,
        instance: &CodegenInstance,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<(Option<BinarySymbolName>, CodegenLinkage), CodegenPreparationError> {
        match instance.mir().kind() {
            MirUnitKind::ExecutableHost(_) => Ok((None, CodegenLinkage::Export)),
            MirUnitKind::GeneratedLifecycle(_) => Ok((None, CodegenLinkage::Internal)),
            MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
                let boundary = self.codegen_native_boundary(instance.key(), cancellation)?;

                if let Some((name, linkage)) = boundary {
                    return Ok((Some(name), linkage));
                }

                let linkage = if roots.contains(instance.key()) {
                    CodegenLinkage::Export
                } else if matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_)) {
                    CodegenLinkage::LinkOnce
                } else {
                    CodegenLinkage::Internal
                };

                Ok((None, linkage))
            }
        }
    }

    fn codegen_instance_package(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        product_package: &PackageIdentity,
        cancellation: &CancellationToken,
    ) -> Result<PackageIdentity, CodegenPreparationError> {
        let symbol = match instance.template() {
            // Compatibility metadata owns package identity beyond the semantic-key borrow.
            MirUnitKey::Bound(key) => {
                return Ok(key
                    .declared_owner()
                    .package_identity()
                    .unwrap_or(product_package)
                    .clone());
            }
            // Compatibility metadata owns package identity beyond the product-key borrow.
            MirUnitKey::ExecutableHost(product) => return Ok(product.package().clone()),
            // Generated lifecycle definitions belong to the selected product package.
            MirUnitKey::GeneratedLifecycle(_) => return Ok(product_package.clone()),
            MirUnitKey::ExternalCallable(definition) => definition.symbol(),
            MirUnitKey::ImportedExecutable(key) => key.owner(),
            MirUnitKey::ExternalRuntimeDefault(symbol) => *symbol,
        };

        let binding_context = self.binding_context(cancellation)?;

        let key = binding_context
            .symbol_key(symbol)
            .map_err(super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Compatibility metadata owns package identity beyond the binder-binding_context borrow.
        Ok(key.package_identity().unwrap_or(product_package).clone())
    }

    fn codegen_native_boundary(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<(BinarySymbolName, CodegenLinkage)>, CodegenPreparationError> {
        if matches!(
            instance.template(),
            MirUnitKey::GeneratedLifecycle(_)
                | MirUnitKey::ExecutableHost(_)
                | MirUnitKey::ExternalRuntimeDefault(_)
        ) {
            return Ok(None);
        }

        if self
            .codegen_runtime_default_provider(instance, cancellation)?
            .is_some()
        {
            return Ok(None);
        }

        let definition = self.codegen_callable_definition(instance)?;

        let bray_symbols::CallableSymbolId::Function(function) = definition.callable_symbol()
        else {
            return Ok(None);
        };

        let contract = self.foreign_callable_contract_with_cancellation(function, cancellation)?;

        if let Some(contract) = contract.value() {
            return native_boundary_mapping(contract.symbol(), contract.direction()).map(Some);
        }

        let boundary =
            self.imported_native_boundary_with_cancellation(function.into(), cancellation)?;

        boundary
            .map(|boundary| {
                native_boundary_mapping(boundary.symbol().as_str(), boundary.direction())
            })
            .transpose()
    }

    fn generated_callable_symbol_name(
        &self,
        target: &CodegenTarget,
        linkage: CodegenLinkage,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<BinarySymbolName, CodegenPreparationError> {
        if let Some(provider) =
            self.codegen_runtime_default_provider(realization.key(), cancellation)?
        {
            let binding_context = self.binding_context(cancellation)?;
            let provider = self.portable_codegen_symbol_key(&binding_context, provider)?;
            let mut hasher = StableDigestHasher::new();

            hasher.write(b"bray.codegen-runtime-default-symbol");
            provider.hash(&mut hasher);
            realization.key().specialization().hash(&mut hasher);
            realization.key().witnesses().hash(&mut hasher);
            realization.key().target().hash(&mut hasher);

            return binary_symbol_name(target, linkage, "default", hasher.finalize());
        }

        let Some(callable) = realization.callable_instance() else {
            return generated_instance_symbol_name(target, linkage, realization.key());
        };

        let binding_context = self.binding_context(cancellation)?;

        let definition =
            self.portable_codegen_symbol_key(&binding_context, callable.definition().symbol())?;

        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.codegen-callable-symbol");
        definition.hash(&mut hasher);
        realization.key().specialization().hash(&mut hasher);
        realization.key().witnesses().hash(&mut hasher);
        realization.key().target().hash(&mut hasher);

        binary_symbol_name(target, linkage, "instance", hasher.finalize())
    }

    fn codegen_runtime_default_provider(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<AnySymbolId>, CodegenPreparationError> {
        match instance.template() {
            MirUnitKey::Bound(key)
                if key.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault =>
            {
                self.symbol_graph()?
                    .symbol_for_key(key.declared_owner())
                    .map(Some)
                    .ok_or_else(|| FactQueryError::InfrastructureFailure.into())
            }
            MirUnitKey::ImportedExecutable(provider) => {
                let binding_context = self.binding_context(cancellation)?;
                let provider = provider.owner();

                Ok(binding_context
                    .runtime_default_subject(provider)
                    .map(|subject| subject.map(|_| provider))
                    .map_err(super::super::binder::binding_query_error)?)
            }
            MirUnitKey::ExternalRuntimeDefault(provider) => Ok(Some(*provider)),
            MirUnitKey::Bound(_)
            | MirUnitKey::ExecutableHost(_)
            | MirUnitKey::GeneratedLifecycle(_)
            | MirUnitKey::ExternalCallable(_) => Ok(None),
        }
    }

    pub(super) fn portable_codegen_symbol_key(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        symbol: AnySymbolId,
    ) -> Result<SymbolKey, CodegenPreparationError> {
        let definition = binding_context
            .symbol_key(symbol)
            .map_err(super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        match definition.data() {
            SymbolKeyData::SourceDeclaration { .. } => Ok(SymbolKey::external(
                super::super::export::external_symbol_key(
                    binding_context.symbols(),
                    self.package_identity(),
                    symbol,
                )
                .map_err(|_| FactQueryError::InfrastructureFailure)?,
            )),
            SymbolKeyData::Synthesized(synthesized)
                if source_backed_symbol_key(synthesized.subject()) =>
            {
                Ok(SymbolKey::external(
                    super::super::export::external_symbol_key(
                        binding_context.symbols(),
                        self.package_identity(),
                        symbol,
                    )
                    .map_err(|_| FactQueryError::InfrastructureFailure)?,
                ))
            }
            // Symbol keys are Arc-backed and the generated mapping owns its identity input.
            _ => Ok(definition.clone()),
        }
    }

    fn codegen_callables(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCallableMapping>, CodegenPreparationError> {
        let mut mappings = Vec::new();

        for (owner, demand) in demanded_callable_instances(unit) {
            let owner_realization = reachability
                .instance(&owner)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mapping = match self.concrete_codegen_callee(
                owner_realization,
                &demand,
                target,
                cancellation,
            )? {
                ConcreteCodegenCallee::Instance(instance) => CodegenCallableMapping::new(
                    owner.clone(),
                    demand.site(),
                    demand.reference(),
                    instance.key().clone(),
                ),
                ConcreteCodegenCallee::Intrinsic(intrinsic) => CodegenCallableMapping::intrinsic(
                    owner.clone(),
                    demand.site(),
                    demand.reference(),
                    intrinsic,
                ),
            };

            mappings.push(mapping);
        }

        Ok(mappings)
    }

    fn codegen_constants(
        &self,
        unit: &CodegenUnit,
        additional: impl IntoIterator<Item = bray_symbols::ConstantValueId>,
    ) -> Result<Vec<CodegenConstantMapping>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let demands = demanded_constants(unit);
        let mut pending = Vec::new();
        let mut mapped = BTreeMap::new();

        for value in demands.values() {
            match demands.types().get(value) {
                Some(types) if !types.is_empty() => {
                    pending.extend(types.iter().map(|ty| (*value, Some(*ty))));
                }
                Some(_) | None => pending.push((*value, None)),
            }
        }

        pending.extend(additional.into_iter().map(|value| (value, None)));

        while let Some((value, representation)) = pending.pop() {
            let data = values
                .constant_value_data(value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let representation = representation.unwrap_or_else(|| data.ty());
            let key = (value, representation);

            if mapped.contains_key(&key) {
                continue;
            }

            pending.extend(child_constants(data.kind()).map(|child| (child, None)));

            // Code generation mappings outlive this shared semantic-store read and therefore
            // take independent ownership of the immutable payload at this boundary.
            mapped.insert(
                key,
                CodegenConstantMapping::with_representation(
                    value,
                    data.as_ref().clone(),
                    representation,
                ),
            );
        }

        Ok(mapped.into_values().collect())
    }

    fn codegen_constant_terms(
        &self,
        unit: &CodegenUnit,
        reachability: &ConcreteCodegenReachability,
    ) -> Result<
        (
            Vec<CodegenConstantTermMapping>,
            Vec<CodegenTerminatorMapping>,
        ),
        CodegenPreparationError,
    > {
        let values = self.semantic_value_store()?;
        let mut terms = Vec::new();
        let mut resolved = BTreeMap::new();

        for instance in unit.instances() {
            let realization = reachability
                .instance(instance.key())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            for template_term in demanded_constant_terms(instance.mir()) {
                let term = self
                    .substitute_codegen_constant_term(template_term, realization.substitution())?;

                let data = values
                    .constant_term_data(term)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let ConstantTermData::Value(value) = data.as_ref() else {
                    return Err(CodegenPreparationError::OpenConstantTerm(term));
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
                if let Some(value) = data.terminator().kind().pattern_literal_value() {
                    terminators.push(CodegenTerminatorMapping::new(
                        instance.key().clone(),
                        block,
                        [value],
                    ));

                    continue;
                }

                let Some(term) = data.terminator().kind().pattern_constant_term() else {
                    continue;
                };

                let Some(value) = resolved.get(&(instance.key().clone(), term)) else {
                    return Err(CodegenPreparationError::OpenConstantTerm(term));
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

    #[cfg(test)]
    fn codegen_types(
        &self,
        demanded: BTreeSet<TypeId>,
        substitution: Option<GenericSubstitutionId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenTypeMapping>, CodegenPreparationError> {
        let mut mappings = BTreeMap::new();
        let mut instance_mappings = Vec::new();

        self.extend_codegen_types(
            demanded,
            substitution,
            target,
            cancellation,
            &mut mappings,
            None,
            &mut instance_mappings,
        )?;

        Ok(mappings.into_values().collect())
    }

    fn extend_codegen_types(
        &self,
        demanded: BTreeSet<TypeId>,
        substitution: Option<GenericSubstitutionId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        instance: Option<&ConcreteCodegenInstance>,
        instance_mappings: &mut Vec<CodegenInstanceTypeMapping>,
    ) -> Result<(), CodegenPreparationError> {
        let mut pending = BTreeSet::new();

        for template in demanded {
            let ty = self.concrete_codegen_type(template, substitution, instance, cancellation)?;

            self.codegen_type(ty, target, cancellation, mappings, &mut pending)?;

            if template != ty {
                let mapping = mappings
                    .get(&ty)
                    .cloned()
                    .ok_or(CodegenPreparationError::UnsupportedType(ty))?;

                if let Some(instance) = instance {
                    // Published mappings own instance identities independently of realization.
                    instance_mappings.push(CodegenInstanceTypeMapping::new(
                        instance.key().clone(),
                        template,
                        ty,
                    ));
                } else {
                    let mapping = match mapping.layout() {
                        Some(layout) => {
                            CodegenTypeMapping::new(template, layout, mapping.kind().clone())
                        }
                        None => CodegenTypeMapping::new_unsized(template, mapping.kind().clone()),
                    }
                    .with_backend_type(ty)
                    .with_behavior(mapping.behavior());

                    mappings.insert(template, mapping);
                }
            }
        }

        self.classify_codegen_callable_types(target, cancellation, mappings, &mut pending)?;

        Ok(())
    }

    fn concrete_codegen_type(
        &self,
        ty: TypeId,
        substitution: Option<GenericSubstitutionId>,
        instance: Option<&ConcreteCodegenInstance>,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, CodegenPreparationError> {
        let ty = self.substitute_codegen_type(ty, substitution, cancellation)?;
        let binding_context = self.binding_context(cancellation)?;

        let checker = CompilationCheckerContext::new(binding_context)
            .with_implementation_witnesses(
                instance
                    .into_iter()
                    .flat_map(ConcreteCodegenInstance::implementation_witnesses)
                    .copied(),
            );

        let mut diagnostics = DiagnosticBag::new();

        let ty = bray_checker::normalize_type_valued_members(&checker, ty, &mut diagnostics)
            .map_err(codegen_checker_error)?;

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        Ok(ty)
    }

    fn substitute_codegen_type(
        &self,
        ty: TypeId,
        substitution: Option<GenericSubstitutionId>,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, FactQueryError> {
        let ty = self.resolve_codegen_contextual_self(ty, cancellation)?;

        let Some(substitution) = substitution else {
            return Ok(ty);
        };

        let values = self.semantic_value_store()?;

        let ty = values
            .substitute_type(ty, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let substitution =
                    substitution_for_owner(values, definition.into_any(), [substitution])?;

                values
                    .intern_type(TypeData::Named {
                        definition: *definition,
                        substitution,
                    })
                    .map_err(|_| FactQueryError::InfrastructureFailure)
            }
            TypeData::Borrow { kind, target } => {
                let target =
                    self.substitute_codegen_type(*target, Some(substitution), cancellation)?;

                values
                    .intern_type(TypeData::Borrow {
                        kind: *kind,
                        target,
                    })
                    .map_err(|_| FactQueryError::InfrastructureFailure)
            }
            _ => Ok(ty),
        }
    }

    fn resolve_codegen_contextual_self(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, FactQueryError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::ContextualSelf(SelfTypeContext::Implementation(implementation)) =
            data.as_ref()
        else {
            return Ok(ty);
        };

        let binding_context = self.binding_context(cancellation)?;

        implementation_subject(&binding_context, *implementation)
    }

    fn substitute_codegen_constant_term(
        &self,
        term: bray_symbols::ConstantTermId,
        substitution: Option<GenericSubstitutionId>,
    ) -> Result<bray_symbols::ConstantTermId, CodegenPreparationError> {
        let term = if let Some(substitution) = substitution {
            self.semantic_value_store()?
                .substitute_constant_term(term, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?
        } else {
            term
        };

        self.realize_codegen_constant_argument(term)
    }

    fn classify_codegen_callable_types(
        &self,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenPreparationError> {
        let mut classified = BTreeSet::new();

        loop {
            let callable =
                mappings.values().find_map(|mapping| {
                    if classified.contains(&mapping.ty()) {
                        return None;
                    }

                    let CodegenTypeKind::Callable(signature) = mapping.kind() else {
                        return None;
                    };

                    if signature.parameters().iter().any(|parameter| {
                        !matches!(parameter, CodegenParameterMapping::Direct { .. })
                    }) || matches!(signature.result(), CodegenResultMapping::Indirect { .. })
                    {
                        return None;
                    }

                    // Signatures use shared slices. This clone releases the mapping borrow before recursion.
                    Some((mapping.ty(), signature.as_ref().clone()))
                });

            let Some((ty, signature)) = callable else {
                break;
            };

            classified.insert(ty);

            self.codegen_signature_types(&signature, target, cancellation, mappings, pending)?;

            let signature = self.classify_codegen_signature(
                signature,
                target,
                cancellation,
                mappings,
                pending,
            )?;

            mappings.insert(
                ty,
                CodegenTypeMapping::new(
                    ty,
                    pointer_layout(target),
                    CodegenTypeKind::Callable(signature.into()),
                ),
            );
        }

        Ok(())
    }

    fn codegen_type(
        &self,
        ty: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenPreparationError> {
        if mappings.contains_key(&ty) {
            return Ok(());
        }

        cancellation.check()?;

        if !pending.insert(ty) {
            return Err(CodegenPreparationError::RecursiveValueType(ty));
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
                    .and_then(CodegenTypeMapping::layout)
                    .ok_or(CodegenPreparationError::UnsizedTypeByValue(*element))?;

                CodegenTypeMapping::new(
                    ty,
                    TargetValueLayout::new(
                        element_layout
                            .size()
                            .checked_mul(length)
                            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?,
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
                kind,
                target: pointee,
            } => self.codegen_indirection_type(
                ty,
                *pointee,
                Some(*kind),
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::OwnedIndirection {
                target: pointee, ..
            } => self.codegen_indirection_type(
                ty,
                *pointee,
                None,
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::Callable(callable) => {
                let signature = callable_type_signature(self, callable)?;

                CodegenTypeMapping::new(
                    ty,
                    pointer_layout(target),
                    CodegenTypeKind::Callable(signature.into()),
                )
            }
            TypeData::Slice(element) => {
                self.codegen_type(*element, target, cancellation, mappings, pending)?;

                CodegenTypeMapping::new_unsized(
                    ty,
                    CodegenTypeKind::UnsizedSlice { element: *element },
                )
            }
            TypeData::Generator(element) => {
                self.codegen_generator_type(ty, *element, target, cancellation, mappings, pending)?
            }
            TypeData::Nullable(element) => {
                self.codegen_nullable_type(ty, *element, target, cancellation, mappings, pending)?
            }
            TypeData::TraitView(_) => {
                CodegenTypeMapping::new_unsized(ty, CodegenTypeKind::UnsizedTraitView)
            }
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let mut candidates = mappings.values().filter(|mapping| {
                    values.type_data(mapping.ty()).is_ok_and(|data| {
                        matches!(
                            data.as_ref(),
                            TypeData::Named {
                                definition: candidate,
                                ..
                            } if candidate == definition
                        )
                    })
                });

                let mapping = candidates
                    .next()
                    .cloned()
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                if candidates.any(|candidate| {
                    candidate.layout() != mapping.layout() || candidate.kind() != mapping.kind()
                }) {
                    return Err(CodegenPreparationError::UnresolvedType(ty));
                }

                match mapping.layout() {
                    Some(layout) => CodegenTypeMapping::new(ty, layout, mapping.kind().clone()),
                    None => CodegenTypeMapping::new_unsized(ty, mapping.kind().clone()),
                }
            }
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(
                SelfTypeContext::Trait(_) | SelfTypeContext::Implementation(_),
            )
            | TypeData::TypeValuedMemberProjection { .. } => {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            }
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
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let role = super::super::foreign::compiler_known_representation(self, definition);

        if let Some(role) = role
            && let Some(mapping) = self.codegen_compiler_known_type(
                ty,
                role,
                substitution,
                target,
                cancellation,
                mappings,
                pending,
            )?
        {
            return Ok(mapping);
        }

        let heap_key = CompilerKnownDeclarationKey::try_new("Heap")
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if self
            .available_compiler_known_symbols()
            .declaration_symbol::<StructSymbolId>(&heap_key)
            .is_some_and(|heap| definition == NamedTypeSymbolId::Struct(heap))
        {
            return Ok(pointer_mapping(
                ty,
                ty,
                target,
                TargetAddressSpaceKind::Default,
            ));
        }

        match definition {
            NamedTypeSymbolId::Struct(structure) => {
                let representation = self.declared_type_representation_with_cancellation(
                    NamedTypeSymbolId::Struct(structure),
                    cancellation,
                )?;

                let bray_symbols::DeclaredStorageShape::Structure(storage) =
                    representation.value().storage()
                else {
                    return Err(CodegenPreparationError::UnresolvedType(ty));
                };

                let fields = storage
                    .iter()
                    .map(|field| {
                        let field_ty =
                            self.resolve_codegen_type(field.ty(), substitution, cancellation)?;

                        Ok((
                            field.field().map(bray_ir::MirFieldReference::Struct),
                            field_ty,
                        ))
                    })
                    .collect::<Result<Vec<_>, FactQueryError>>()?;

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
            NamedTypeSymbolId::Union(union) => self.codegen_union_type(
                ty,
                union,
                substitution,
                target,
                cancellation,
                mappings,
                pending,
            ),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "compiler-known type realization shares recursive mapping state with structural types"
    )]
    fn codegen_compiler_known_type(
        &self,
        ty: TypeId,
        role: RepresentationRole,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<Option<CodegenTypeMapping>, CodegenPreparationError> {
        if matches!(role, RepresentationRole::Unit | RepresentationRole::Never) {
            return Ok(Some(CodegenTypeMapping::new(
                ty,
                TargetValueLayout::new(0, NonZeroU64::MIN, TargetLayoutContract::Default),
                CodegenTypeKind::Unit,
            )));
        }

        if matches!(
            role,
            RepresentationRole::RawPointer | RepresentationRole::DevicePointer
        ) {
            let substitution = self
                .semantic_value_store()?
                .generic_substitution_data(substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let [binding] = substitution.bindings() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            let GenericArgument::Type(pointee) = binding.argument() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            let address_space = match role {
                RepresentationRole::RawPointer => TargetAddressSpaceKind::Default,
                RepresentationRole::DevicePointer => TargetAddressSpaceKind::Device,
                _ => return Err(CodegenPreparationError::UnresolvedType(ty)),
            };

            self.codegen_type(pointee, target, cancellation, mappings, pending)?;

            return Ok(Some(pointer_mapping(ty, pointee, target, address_space)));
        }

        if role == RepresentationRole::Atomic {
            let values = self.semantic_value_store()?;

            let substitution = values
                .generic_substitution_data(substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let [binding] = substitution.bindings() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            let GenericArgument::Type(value) = binding.argument() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            self.codegen_type(value, target, cancellation, mappings, pending)?;

            let representation = atomic_representation_for_type(self, value, target, cancellation)?
                .ok_or(CodegenPreparationError::UnsupportedType(value))?;

            let storage_type = match atomic_storage_role(representation) {
                Some(role) => self.codegen_representation_type(role)?,
                None => value,
            };

            self.codegen_type(storage_type, target, cancellation, mappings, pending)?;

            let storage_mapping = mappings
                .get(&storage_type)
                .ok_or(CodegenPreparationError::UnresolvedType(storage_type))?;

            let storage_layout = storage_mapping
                .layout()
                .ok_or(CodegenPreparationError::UnsizedTypeByValue(storage_type))?;

            let binding_context = target
                .profile()
                .properties()
                .atomics()
                .representation(representation);

            if !binding_context.operations().any() {
                return Err(CodegenPreparationError::UnsupportedType(value));
            }

            return Ok(Some(
                CodegenTypeMapping::new(
                    ty,
                    TargetValueLayout::new(
                        storage_layout.size(),
                        binding_context.required_alignment(),
                        storage_layout.contract(),
                    ),
                    // The wrapper and backing storage deliberately share one backend kind.
                    storage_mapping.kind().clone(),
                )
                .with_backend_type(storage_type),
            ));
        }

        if let Some(scalar) = super::super::representation::target_scalar(role) {
            return scalar_mapping(
                self,
                ty,
                role,
                scalar,
                target,
                cancellation,
                mappings,
                pending,
            )
            .map(Some);
        }

        match role {
            RepresentationRole::Uninit => {
                let values = self.semantic_value_store()?;

                let element = self
                    .available_compiler_known_symbols()
                    .unary_representation_argument(values, role, ty)
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                self.codegen_type(element, target, cancellation, mappings, pending)?;

                let element_mapping = mappings
                    .get(&element)
                    .ok_or(CodegenPreparationError::UnresolvedType(element))?;

                // The wrapper shares immutable physical kind metadata but intentionally omits the element lifecycle behavior.
                let kind = element_mapping.kind().clone();

                let mapping = match element_mapping.layout() {
                    Some(layout) => CodegenTypeMapping::new(ty, layout, kind),
                    None => CodegenTypeMapping::new_unsized(ty, kind),
                }
                .with_backend_type(element_mapping.backend_type());

                Ok(Some(mapping))
            }
            RepresentationRole::String => self
                .codegen_string_type(ty, target, cancellation, mappings, pending)
                .map(Some),
            RepresentationRole::PanicReport => scalar_mapping(
                self,
                ty,
                RepresentationRole::ScalarUsize,
                TargetScalarKind::Usize,
                target,
                cancellation,
                mappings,
                pending,
            )
            .map(Some),
            RepresentationRole::Future => {
                let pointer = self.codegen_opaque_pointer_type()?;

                self.codegen_aggregate_type(
                    ty,
                    [(None, pointer), (None, pointer)],
                    TargetLayoutContract::C,
                    None,
                    None,
                    target,
                    cancellation,
                    mappings,
                    pending,
                )
                .map(Some)
            }
            RepresentationRole::Task => Ok(Some(pointer_mapping(
                ty,
                ty,
                target,
                TargetAddressSpaceKind::Default,
            ))),
            RepresentationRole::Result
            | RepresentationRole::RunResult
            | RepresentationRole::ConversionError => Ok(None),
            RepresentationRole::BooleanTrue
            | RepresentationRole::BooleanFalse
            | RepresentationRole::UnitValue
            | RepresentationRole::NoneValue => Err(CodegenPreparationError::UnresolvedType(ty)),
            RepresentationRole::Unit
            | RepresentationRole::Never
            | RepresentationRole::Atomic
            | RepresentationRole::RawPointer
            | RepresentationRole::DevicePointer
            | RepresentationRole::ScalarBool
            | RepresentationRole::ScalarChar
            | RepresentationRole::ScalarI8
            | RepresentationRole::ScalarI16
            | RepresentationRole::ScalarI32
            | RepresentationRole::ScalarI64
            | RepresentationRole::ScalarI128
            | RepresentationRole::ScalarU8
            | RepresentationRole::ScalarU16
            | RepresentationRole::ScalarU32
            | RepresentationRole::ScalarU64
            | RepresentationRole::ScalarU128
            | RepresentationRole::ScalarIsize
            | RepresentationRole::ScalarUsize
            | RepresentationRole::ScalarR16
            | RepresentationRole::ScalarR32
            | RepresentationRole::ScalarR64
            | RepresentationRole::ScalarR128
            | RepresentationRole::ScalarC32
            | RepresentationRole::ScalarC64
            | RepresentationRole::ScalarC128
            | RepresentationRole::ScalarC256 => Err(CodegenPreparationError::UnresolvedType(ty)),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "union realization keeps checked representation and recursive mapping state explicit"
    )]
    fn codegen_union_type(
        &self,
        ty: TypeId,
        union: bray_symbols::UnionSymbolId,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let representation = self.declared_type_representation_with_cancellation(
            NamedTypeSymbolId::Union(union),
            cancellation,
        )?;

        let bray_symbols::DeclaredStorageShape::Union(storage) = representation.value().storage()
        else {
            return Err(CodegenPreparationError::UnresolvedType(ty));
        };

        let tag = representation
            .value()
            .union_tag_type()
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        self.codegen_type(tag, target, cancellation, mappings, pending)?;

        let tag_layout = sized_layout(mappings, tag)?;
        let packing = representation.value().packing().and_then(NonZeroU64::new);

        let tag_alignment = packed_alignment(tag_layout.alignment(), packing);

        let mut payload_alignment = NonZeroU64::MIN;
        let mut payload_size = 0_u64;
        let mut variants = Vec::with_capacity(storage.len());

        for variant in storage.iter() {
            let mut offset = 0_u64;
            let mut alignment = NonZeroU64::MIN;
            let mut fields = Vec::with_capacity(variant.members().len());

            for field in variant.members() {
                let field_ty = self.resolve_codegen_type(field.ty(), substitution, cancellation)?;

                self.codegen_type(field_ty, target, cancellation, mappings, pending)?;

                let field_layout = sized_layout(mappings, field_ty)?;

                let field_alignment = packed_alignment(field_layout.alignment(), packing);

                alignment = alignment.max(field_alignment);

                offset = align_to(offset, field_alignment)
                    .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

                fields.push(CodegenFieldLayout::new(
                    field.field().map(bray_ir::MirFieldReference::UnionPayload),
                    field_ty,
                    offset,
                ));

                offset = offset
                    .checked_add(field_layout.size())
                    .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;
            }

            let size =
                align_to(offset, alignment).ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

            payload_alignment = payload_alignment.max(alignment);
            payload_size = payload_size.max(size);
            variants.push((variant.variant(), fields));
        }

        let payload_offset = align_to(tag_layout.size(), payload_alignment)
            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        let variants = variants
            .into_iter()
            .map(|(variant, fields)| {
                let tag = representation
                    .value()
                    .union_tags()
                    .iter()
                    .find(|tag| tag.variant() == variant)
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                let fields = fields
                    .into_iter()
                    .map(|field| {
                        let offset = payload_offset
                            .checked_add(field.offset_bytes())
                            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

                        Ok(CodegenFieldLayout::new(
                            field.reference(),
                            field.ty(),
                            offset,
                        ))
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

                // Representation binding_context are shared. Codegen mappings own exact tag magnitudes.
                Ok(CodegenUnionVariantLayout::new(
                    variant,
                    tag.value().clone(),
                    fields,
                ))
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        let mut alignment = tag_alignment.max(payload_alignment);

        if let Some(requested) = representation.value().alignment().and_then(NonZeroU64::new) {
            alignment = alignment.max(requested);
        }

        ensure_target_alignment(ty, alignment, target)?;

        let size = payload_offset
            .checked_add(payload_size)
            .and_then(|size| align_to(size, alignment))
            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        Ok(CodegenTypeMapping::new(
            ty,
            TargetValueLayout::new(
                size,
                alignment,
                target_layout_contract(representation.value().layout()),
            ),
            CodegenTypeKind::union(tag, variants),
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "indirection realization keeps boundary metadata and recursive mapping state explicit"
    )]
    fn codegen_indirection_type(
        &self,
        ty: TypeId,
        pointee: TypeId,
        borrow: Option<BorrowKind>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let pointee_data = values
            .type_data(pointee)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let fields = match pointee_data.as_ref() {
            TypeData::Named { definition, .. }
                if super::super::foreign::compiler_known_representation(self, *definition)
                    == Some(RepresentationRole::String) =>
            {
                self.codegen_type(pointee, target, cancellation, mappings, pending)?;

                return Ok(pointer_mapping(
                    ty,
                    pointee,
                    target,
                    TargetAddressSpaceKind::Default,
                ));
            }
            TypeData::Slice(element) => {
                self.codegen_type(pointee, target, cancellation, mappings, pending)?;

                let data = self.indirection_metadata_pointer(
                    *element,
                    borrow.unwrap_or(BorrowKind::Mutable),
                )?;

                let length = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

                vec![data, length]
            }
            TypeData::TraitView(_) => {
                self.codegen_type(pointee, target, cancellation, mappings, pending)?;

                let metadata = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

                let pointer = self.indirection_metadata_pointer(metadata, BorrowKind::Shared)?;

                vec![pointer, pointer]
            }
            _ => {
                return Ok(pointer_mapping(
                    ty,
                    pointee,
                    target,
                    TargetAddressSpaceKind::Default,
                ));
            }
        };

        self.codegen_aggregate_type(
            ty,
            fields.into_iter().map(|field| (None, field)),
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "generator realization shares recursive mapping state with element and metadata types"
    )]
    fn codegen_generator_type(
        &self,
        ty: TypeId,
        element: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        self.codegen_type(element, target, cancellation, mappings, pending)?;

        if mappings
            .get(&element)
            .is_none_or(|mapping| mapping.layout().is_none())
        {
            return Err(CodegenPreparationError::UnsizedTypeByValue(element));
        }

        let data = self.indirection_metadata_pointer(element, BorrowKind::Mutable)?;
        let length = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

        self.codegen_aggregate_type(
            ty,
            [data, length, length]
                .into_iter()
                .map(|field| (None, field)),
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "nullable realization shares recursive mapping state with tag and payload types"
    )]
    fn codegen_nullable_type(
        &self,
        ty: TypeId,
        element: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let present = self.compiler_known_type(RepresentationRole::ScalarBool)?;

        self.codegen_aggregate_type(
            ty,
            [(None, present), (None, element)],
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "string realization shares recursive mapping state with data and length types"
    )]
    fn codegen_string_type(
        &self,
        ty: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let byte = self.compiler_known_type(RepresentationRole::ScalarU8)?;
        let data = self.indirection_metadata_pointer(byte, BorrowKind::Shared)?;
        let owner = data;
        let length = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

        self.codegen_aggregate_type(
            ty,
            [(None, data), (None, owner), (None, length)],
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
        .map(|mapping| mapping.with_behavior(Some(bray_codegen::CodegenTypeBehavior::String)))
    }

    fn compiler_known_type(&self, role: RepresentationRole) -> Result<TypeId, FactQueryError> {
        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        named_type(
            self.semantic_value_store()?,
            NamedTypeSymbolId::Struct(definition),
        )
    }

    fn indirection_metadata_pointer(
        &self,
        target: TypeId,
        kind: BorrowKind,
    ) -> Result<TypeId, FactQueryError> {
        self.semantic_value_store()?
            .intern_type(TypeData::Borrow { kind, target })
            .map_err(|_| FactQueryError::InfrastructureFailure)
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
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let mut offset = 0_u64;
        let mut alignment = NonZeroU64::MIN;
        let mut layouts = Vec::new();
        let packing = packing.and_then(NonZeroU64::new);

        for (reference, field) in fields {
            self.codegen_type(field, target, cancellation, mappings, pending)?;

            let field_layout = mappings
                .get(&field)
                .and_then(CodegenTypeMapping::layout)
                .ok_or(CodegenPreparationError::UnsizedTypeByValue(field))?;

            let field_alignment = packed_alignment(field_layout.alignment(), packing);

            alignment = alignment.max(field_alignment);

            offset = align_to(offset, field_alignment)
                .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

            layouts.push(CodegenFieldLayout::new(reference, field, offset));

            offset = offset
                .checked_add(field_layout.size())
                .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;
        }

        if let Some(requested) = requested_alignment.and_then(NonZeroU64::new) {
            alignment = alignment.max(requested);
        }

        ensure_target_alignment(ty, alignment, target)?;

        let size =
            align_to(offset, alignment).ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        Ok(CodegenTypeMapping::new(
            ty,
            TargetValueLayout::new(size, alignment, contract),
            CodegenTypeKind::aggregate(layouts),
        ))
    }

    pub(super) fn resolve_codegen_type(
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

    pub(in crate::compilation) fn plain_storage_atomic_representation(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Option<TargetAtomicRepresentation>, FactQueryError> {
        let Ok(target) = self.selected_target().target().codegen_target() else {
            return Ok(None);
        };

        let mut mappings = BTreeMap::new();
        let mut pending = BTreeSet::new();

        match self.codegen_type(ty, &target, cancellation, &mut mappings, &mut pending) {
            Ok(()) => {}
            Err(CodegenPreparationError::Query(error)) => return Err(error),
            Err(_) => return Ok(None),
        }

        let size = mappings
            .get(&ty)
            .and_then(CodegenTypeMapping::layout)
            .filter(|_| atomic_storage_is_padding_free(ty, &mappings))
            .map(TargetValueLayout::size);

        Ok(size.and_then(TargetAtomicRepresentation::for_storage_size))
    }

    pub(in crate::compilation) fn atomic_representation_for_type(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Option<TargetAtomicRepresentation>, FactQueryError> {
        let Ok(target) = self.selected_target().target().codegen_target() else {
            return Ok(None);
        };

        match atomic_representation_for_type(self, ty, &target, cancellation) {
            Ok(Some(representation))
                if target
                    .profile()
                    .properties()
                    .atomics()
                    .representation(representation)
                    .operations()
                    .any() =>
            {
                Ok(Some(representation))
            }
            Ok(_) => Ok(None),
            Err(CodegenPreparationError::Query(error)) => Err(error),
            Err(_) => Ok(None),
        }
    }

    fn codegen_signature_types(
        &self,
        signature: &CodegenCallableSignature,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenPreparationError> {
        for ty in signature_types(signature) {
            self.codegen_type(ty, target, cancellation, mappings, pending)?;
        }

        Ok(())
    }

    fn classify_codegen_signature(
        &self,
        signature: CodegenCallableSignature,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let mut parameters = Vec::with_capacity(signature.parameters().len());

        for parameter in signature.parameters() {
            let ty = match parameter {
                CodegenParameterMapping::Direct { ty, .. } => *ty,
                CodegenParameterMapping::Ignore | CodegenParameterMapping::Indirect { .. } => {
                    return Err(CodegenPreparationError::InvalidAbiMapping);
                }
            };

            parameters.push(self.classify_codegen_parameter(
                ty,
                signature.abi(),
                target,
                cancellation,
                mappings,
                pending,
            )?);
        }

        let result = match signature.result() {
            CodegenResultMapping::Void => CodegenResultMapping::Void,
            CodegenResultMapping::Direct { ty, .. } => self.classify_codegen_result(
                *ty,
                signature.abi(),
                target,
                cancellation,
                mappings,
                pending,
            )?,
            CodegenResultMapping::Indirect { .. } => {
                return Err(CodegenPreparationError::InvalidAbiMapping);
            }
        };

        Ok(CodegenCallableSignature::new(
            parameters,
            result,
            signature.abi(),
            signature.is_variadic(),
        ))
    }

    fn classify_codegen_parameter(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenParameterMapping, CodegenPreparationError> {
        let mapping = mappings
            .get(&ty)
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        let layout = mapping
            .layout()
            .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))?;

        if layout.size() == 0 {
            return Ok(CodegenParameterMapping::Ignore);
        }

        if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(CodegenParameterMapping::direct(ty, None, []));
        }

        let pointer = self.indirection_metadata_pointer(ty, BorrowKind::Shared)?;

        self.codegen_type(pointer, target, cancellation, mappings, pending)?;

        Ok(CodegenParameterMapping::indirect(
            pointer,
            ty,
            indirect_parameter_kind(abi, target),
            layout.alignment(),
            [CodegenValueAttribute::NonNull],
        ))
    }

    fn classify_codegen_result(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenResultMapping, CodegenPreparationError> {
        let mapping = mappings
            .get(&ty)
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        let layout = mapping
            .layout()
            .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))?;

        if layout.size() == 0 {
            return Ok(CodegenResultMapping::Void);
        }

        if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(CodegenResultMapping::direct(ty, None, []));
        }

        let pointer = self.indirection_metadata_pointer(ty, BorrowKind::Mutable)?;

        self.codegen_type(pointer, target, cancellation, mappings, pending)?;

        Ok(CodegenResultMapping::indirect(
            pointer,
            ty,
            layout.alignment(),
            [
                CodegenValueAttribute::NoAlias,
                CodegenValueAttribute::NonNull,
            ],
        ))
    }

    pub(super) fn codegen_instance_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        if let MirUnitKey::GeneratedLifecycle(_) = instance.key().template() {
            let reference = instance
                .generated_lifecycle_reference()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            return self.generated_lifecycle_signature(reference);
        }

        if let Some(callable_type) = instance.anonymous_callable_type() {
            let callable_type = self.concrete_codegen_type(
                callable_type,
                instance.substitution(),
                Some(instance),
                cancellation,
            )?;

            let callable = self
                .semantic_value_store()?
                .type_data(callable_type)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let TypeData::Callable(callable) = callable.as_ref() else {
                return Err(FactQueryError::InfrastructureFailure.into());
            };

            return callable_type_signature(self, callable);
        }

        if let Some(provider) =
            self.codegen_runtime_default_provider(instance.key(), cancellation)?
        {
            return self.codegen_runtime_default_signature(instance, provider, cancellation);
        }

        let definition = match instance.key().template() {
            MirUnitKey::ExecutableHost(_) => return Ok(void_signature(CallableAbi::Bray)),
            MirUnitKey::Bound(_)
            | MirUnitKey::ImportedExecutable(_)
            | MirUnitKey::ExternalCallable(_) => {
                self.codegen_callable_definition(instance.key())?
            }
            MirUnitKey::GeneratedLifecycle(_) | MirUnitKey::ExternalRuntimeDefault(_) => {
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
        let binding_context = self.binding_context(cancellation)?;

        let contextual_self = self
            .symbol_graph()?
            .containing_symbol(definition.symbol())
            .and_then(ImplementationSymbolId::try_from_any)
            .map(|implementation| {
                let subject = implementation_subject(&binding_context, implementation)?;

                Ok::<_, FactQueryError>((SelfTypeContext::Implementation(implementation), subject))
            })
            .transpose()?;

        let template = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(super::super::binder::binding_query_error)?;

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

        let callable_type =
            substitute_contextual_self(values, signature.callable_type(), contextual_self)?;

        let receiver = signature
            .receiver()
            .map(|receiver| {
                substitute_contextual_self(values, receiver.ty(), contextual_self).map(|ty| {
                    ReceiverParameterSignature::new(receiver.parameter(), ty, receiver.mode())
                })
            })
            .transpose()?;

        let parameters = signature
            .parameters()
            .iter()
            .copied()
            .map(|parameter| {
                substitute_contextual_self(values, parameter.ty(), contextual_self)
                    .map(|ty| CallableParameterSignature::new(parameter.parameter(), ty))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result = substitute_contextual_self(values, signature.result(), contextual_self)?;

        let checker = CompilationCheckerContext::new(binding_context)
            .with_implementation_witnesses(instance.implementation_witnesses().iter().copied());

        let mut diagnostics = DiagnosticBag::new();

        let callable_type =
            bray_checker::normalize_type_valued_members(&checker, callable_type, &mut diagnostics)
                .map_err(codegen_checker_error)?;

        let receiver = receiver
            .as_ref()
            .map(|receiver| {
                bray_checker::normalize_type_valued_members(
                    &checker,
                    receiver.ty(),
                    &mut diagnostics,
                )
                .map(|ty| {
                    ReceiverParameterSignature::new(receiver.parameter(), ty, receiver.mode())
                })
                .map_err(codegen_checker_error)
            })
            .transpose()?;

        let parameters = parameters
            .iter()
            .copied()
            .map(|parameter| {
                bray_checker::normalize_type_valued_members(
                    &checker,
                    parameter.ty(),
                    &mut diagnostics,
                )
                .map(|ty| CallableParameterSignature::new(parameter.parameter(), ty))
                .map_err(codegen_checker_error)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result =
            bray_checker::normalize_type_valued_members(&checker, result, &mut diagnostics)
                .map_err(codegen_checker_error)?;

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        let signature = CallableSignature::new(callable_type, receiver, parameters, result);

        let callable = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable) = callable.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let receiver = signature
            .receiver()
            .map(|receiver| {
                let receiver_ty = self.concrete_codegen_type(
                    receiver.ty(),
                    Some(substitution),
                    Some(instance),
                    cancellation,
                )?;

                receiver_codegen_type(values, receiver_ty, receiver.mode())
                    .map_err(CodegenPreparationError::from)
            })
            .transpose()?;

        let parameter_types = signature
            .parameters()
            .iter()
            .map(|parameter| {
                self.concrete_codegen_type(
                    parameter.ty(),
                    Some(substitution),
                    Some(instance),
                    cancellation,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let parameters = receiver
            .into_iter()
            .chain(parameter_types)
            .map(|ty| CodegenParameterMapping::direct(ty, None, []));

        let result_type = self.concrete_codegen_type(
            signature.result(),
            Some(substitution),
            Some(instance),
            cancellation,
        )?;

        let result = if callable.execution() == CallableExecution::Asynchronous {
            let future = self
                .available_compiler_known_symbols()
                .unary_representation_type(values, RepresentationRole::Future, result_type)
                .ok_or(FactQueryError::InfrastructureFailure)?;

            CodegenResultMapping::direct(future, None, [])
        } else if is_void_result(self, result_type)? {
            CodegenResultMapping::Void
        } else {
            CodegenResultMapping::direct(result_type, None, [])
        };

        Ok(CodegenCallableSignature::new(
            parameters,
            result,
            callable.abi(),
            false,
        ))
    }

    fn codegen_runtime_default_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        provider: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let owner = binding_context
            .runtime_default_subject(provider)
            .map_err(super::super::binder::binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let (inputs, result) = match owner {
            AnySymbolId::CallableParameter(owner) => {
                let checked = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableParameterDefaultQuery>::new(
                        owner,
                    ))
                    .map_err(super::super::binder::binding_query_error)?;

                let CallableParameterDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                (surface.inputs().to_vec(), surface.result())
            }
            AnySymbolId::StructField(owner) => {
                let checked = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<StructFieldDefaultQuery>::new(owner))
                    .map_err(super::super::binder::binding_query_error)?;

                let StructFieldDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                (surface.inputs().to_vec(), surface.result())
            }
            AnySymbolId::UnionPayloadField(owner) => {
                let checked = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<UnionPayloadFieldDefaultQuery>::new(
                        owner,
                    ))
                    .map_err(super::super::binder::binding_query_error)?;

                let UnionPayloadDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(FactQueryError::InfrastructureFailure.into());
                };

                (surface.inputs().to_vec(), surface.result())
            }
            _ => return Err(FactQueryError::InfrastructureFailure.into()),
        };

        let substitution = instance.substitution();

        let parameters = inputs
            .iter()
            .copied()
            .map(|input| {
                self.codegen_runtime_default_input_type(input, instance, cancellation)
                    .map(|ty| CodegenParameterMapping::direct(ty, None, []))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result =
            self.concrete_codegen_type(result, substitution, Some(instance), cancellation)?;

        let is_void = is_void_result(self, result)?;

        let result = if is_void {
            CodegenResultMapping::Void
        } else {
            CodegenResultMapping::direct(result, None, [])
        };

        Ok(CodegenCallableSignature::new(
            parameters,
            result,
            CallableAbi::Bray,
            false,
        ))
    }

    fn codegen_runtime_default_input_type(
        &self,
        input: RuntimeDefaultProviderInput,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let (owner, parameter) = match input {
            RuntimeDefaultProviderInput::Receiver(parameter) => {
                let owner = binding_context
                    .receiver_parameter(parameter)
                    .map_err(super::super::binder::binding_query_error)?
                    .map(bray_symbols::ReceiverParameterSymbol::owner)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                (owner, AnySymbolId::ReceiverParameter(parameter))
            }
            RuntimeDefaultProviderInput::EarlierParameter(parameter) => {
                let owner = binding_context
                    .callable_parameter(parameter)
                    .map_err(super::super::binder::binding_query_error)?
                    .map(bray_symbols::CallableParameterSymbol::owner)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                (owner, AnySymbolId::CallableParameter(parameter))
            }
        };

        let template = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(owner))
            .map_err(super::super::binder::binding_query_error)?;

        let substitution = instance
            .substitution()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [template.value().callable_type(), template.value().result()],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            self.semantic_value_store()?,
            template.value(),
            substitution,
            constants.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

        let values = self.semantic_value_store()?;

        let receiver = signature
            .receiver()
            .filter(|receiver| AnySymbolId::ReceiverParameter(receiver.parameter()) == parameter)
            .map(|receiver| {
                let ty = self.concrete_codegen_type(
                    receiver.ty(),
                    Some(substitution),
                    Some(instance),
                    cancellation,
                )?;

                receiver_codegen_type(values, ty, receiver.mode())
                    .map_err(CodegenPreparationError::from)
            })
            .transpose()?;

        let ty = match receiver {
            Some(receiver) => receiver,
            None => signature
                .parameters()
                .iter()
                .find_map(|candidate| {
                    (AnySymbolId::CallableParameter(candidate.parameter()) == parameter)
                        .then_some(candidate.ty())
                })
                .ok_or(FactQueryError::InfrastructureFailure)?,
        };

        self.concrete_codegen_type(ty, Some(substitution), Some(instance), cancellation)
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
            MirUnitKey::ImportedExecutable(definition) => {
                CallableDefinitionId::try_new(definition.owner())
                    .ok_or(FactQueryError::InfrastructureFailure)
            }
            MirUnitKey::ExternalCallable(definition) => Ok(*definition),
            MirUnitKey::ExecutableHost(_)
            | MirUnitKey::GeneratedLifecycle(_)
            | MirUnitKey::ExternalRuntimeDefault(_) => Err(FactQueryError::InfrastructureFailure),
        }
    }

    fn generated_lifecycle_signature(
        &self,
        reference: &MirHelperReference,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

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

    fn codegen_runtime_signature(
        &self,
        role: RuntimeAbiRole,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        if role == RuntimeAbiRole::CurrentRunCancellationObservation {
            let boolean = self.codegen_representation_type(RepresentationRole::ScalarBool)?;

            return Ok(CodegenCallableSignature::new(
                [],
                CodegenResultMapping::direct(boolean, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if role == RuntimeAbiRole::PanicPropagation {
            let report = self.codegen_representation_type(RepresentationRole::PanicReport)?;

            return Ok(CodegenCallableSignature::new(
                [CodegenParameterMapping::direct(report, None, [])],
                CodegenResultMapping::Void,
                CallableAbi::Bray,
                false,
            ));
        }

        if !matches!(
            role,
            RuntimeAbiRole::GeneratorBegin
                | RuntimeAbiRole::GeneratorPush
                | RuntimeAbiRole::GeneratorFinish
                | RuntimeAbiRole::GeneratorCleanupBroadcast
                | RuntimeAbiRole::GeneratorDestruction
        ) {
            return Ok(void_signature(CallableAbi::Bray));
        }

        let pointer = self.codegen_opaque_pointer_type()?;
        let usize = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.codegen_representation_type(RepresentationRole::ScalarBool)?;
        let parameter = |ty| CodegenParameterMapping::direct(ty, None, []);

        let (parameters, result) = match role {
            RuntimeAbiRole::GeneratorBegin => (
                vec![
                    parameter(pointer),
                    parameter(usize),
                    parameter(usize),
                    parameter(usize),
                    parameter(usize),
                    parameter(boolean),
                ],
                CodegenResultMapping::Void,
            ),
            RuntimeAbiRole::GeneratorPush | RuntimeAbiRole::GeneratorCleanupBroadcast => (
                vec![parameter(pointer), parameter(pointer)],
                CodegenResultMapping::Void,
            ),
            RuntimeAbiRole::GeneratorFinish => {
                (vec![parameter(pointer)], CodegenResultMapping::Void)
            }
            RuntimeAbiRole::GeneratorDestruction => (
                vec![parameter(pointer), parameter(pointer), parameter(pointer)],
                CodegenResultMapping::Void,
            ),
            _ => return Err(FactQueryError::InfrastructureFailure.into()),
        };

        Ok(CodegenCallableSignature::new(
            parameters,
            result,
            CallableAbi::Bray,
            false,
        ))
    }

    fn codegen_representation_type(
        &self,
        role: RepresentationRole,
    ) -> Result<TypeId, FactQueryError> {
        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        named_type(
            self.semantic_value_store()?,
            NamedTypeSymbolId::Struct(definition),
        )
    }

    fn codegen_opaque_pointer_type(&self) -> Result<TypeId, FactQueryError> {
        let element = self.codegen_representation_type(RepresentationRole::ScalarU8)?;

        self.available_compiler_known_symbols()
            .unary_representation_type(
                self.semantic_value_store()?,
                RepresentationRole::RawPointer,
                element,
            )
            .ok_or(FactQueryError::InfrastructureFailure)
    }
}

fn atomic_storage_is_padding_free(
    ty: TypeId,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    let Some(mapping) = mappings.get(&ty) else {
        return false;
    };

    let Some(layout) = mapping.layout() else {
        return false;
    };

    match mapping.kind() {
        CodegenTypeKind::Aggregate(fields) => {
            let mut end = 0_u64;

            for field in fields.iter() {
                let Some(field_layout) = mappings
                    .get(&field.ty())
                    .and_then(CodegenTypeMapping::layout)
                else {
                    return false;
                };

                if field.offset_bytes() != end
                    || !atomic_storage_is_padding_free(field.ty(), mappings)
                {
                    return false;
                }

                let Some(field_end) = end.checked_add(field_layout.size()) else {
                    return false;
                };

                end = field_end;
            }

            end == layout.size()
        }
        CodegenTypeKind::Array { element, length } => {
            let Some(element_layout) = mappings.get(element).and_then(CodegenTypeMapping::layout)
            else {
                return false;
            };

            atomic_storage_is_padding_free(*element, mappings)
                && element_layout
                    .size()
                    .checked_mul(*length)
                    .is_some_and(|size| size == layout.size())
        }
        CodegenTypeKind::Union { .. }
        | CodegenTypeKind::UnsizedSlice { .. }
        | CodegenTypeKind::UnsizedTraitView => false,
        CodegenTypeKind::Unit
        | CodegenTypeKind::Boolean
        | CodegenTypeKind::SignedInteger(_)
        | CodegenTypeKind::UnsignedInteger(_)
        | CodegenTypeKind::Float(_)
        | CodegenTypeKind::Pointer { .. }
        | CodegenTypeKind::Callable(_) => true,
    }
}

fn atomic_representation_for_type(
    compilation: &Compilation,
    ty: TypeId,
    target: &CodegenTarget,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAtomicRepresentation>, CodegenPreparationError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(None);
    };

    let role = super::super::foreign::compiler_known_representation(compilation, *definition);

    if let Some(representation) = role.and_then(|role| {
        bray_checker::atomic_target_representation(
            role,
            target.profile().machine().pointer_width_bits().get(),
        )
    }) {
        return Ok(Some(representation));
    }

    let representation =
        compilation.declared_type_representation_with_cancellation(*definition, cancellation)?;

    let representation = representation.value();

    if representation.is_recovered()
        || !representation.has_finite_size()
        || (!representation.is_plain_storage()
            && representation.layout() != DeclaredLayoutMode::Transparent)
    {
        return Ok(None);
    }

    compilation
        .plain_storage_atomic_representation(ty, cancellation)
        .map_err(CodegenPreparationError::from)
}

const fn atomic_storage_role(
    representation: TargetAtomicRepresentation,
) -> Option<RepresentationRole> {
    match representation {
        TargetAtomicRepresentation::U8 => Some(RepresentationRole::ScalarU8),
        TargetAtomicRepresentation::U16 => Some(RepresentationRole::ScalarU16),
        TargetAtomicRepresentation::U32 => Some(RepresentationRole::ScalarU32),
        TargetAtomicRepresentation::U64 => Some(RepresentationRole::ScalarU64),
        TargetAtomicRepresentation::U128 => Some(RepresentationRole::ScalarU128),
        TargetAtomicRepresentation::Pointer => None,
    }
}

fn source_backed_symbol_key(key: &SymbolKey) -> bool {
    match key.data() {
        SymbolKeyData::SourceDeclaration { .. } => true,
        SymbolKeyData::Synthesized(synthesized) => source_backed_symbol_key(synthesized.subject()),
        SymbolKeyData::Root(_)
        | SymbolKeyData::Module { .. }
        | SymbolKeyData::CompilerKnownDeclaration { .. }
        | SymbolKeyData::External(_) => false,
    }
}

fn codegen_source_file(
    source: &SourceSnapshot,
) -> Result<CodegenSourceFile, CodegenPreparationError> {
    let origin = source.origin();

    let path = origin
        .file_path()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .or_else(|| origin.virtual_name().map(str::to_owned))
        .or_else(|| origin.generated_name().map(str::to_owned))
        .or_else(|| origin.lsp_uri().map(str::to_owned))
        .or_else(|| origin.test_fixture_name().map(str::to_owned))
        .unwrap_or_else(|| format!("source-{}.bray", source.identity().raw()));

    CodegenSourceFile::try_new(path)
        .ok_or(FactQueryError::InfrastructureFailure)
        .map_err(CodegenPreparationError::from)
}

const fn target_layout_contract(layout: DeclaredLayoutMode) -> TargetLayoutContract {
    match layout {
        DeclaredLayoutMode::Default => TargetLayoutContract::Default,
        DeclaredLayoutMode::Stable => TargetLayoutContract::Stable,
        DeclaredLayoutMode::C => TargetLayoutContract::C,
        DeclaredLayoutMode::Transparent => TargetLayoutContract::Transparent,
    }
}

fn substitute_contextual_self(
    values: &SemanticValueStore,
    ty: TypeId,
    substitution: Option<(SelfTypeContext, TypeId)>,
) -> Result<TypeId, FactQueryError> {
    let Some((context, replacement)) = substitution else {
        return Ok(ty);
    };

    values
        .substitute_contextual_self(ty, context, replacement)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

fn implementation_subject(
    binding_context: &CompilationBindingContext<'_>,
    implementation: ImplementationSymbolId,
) -> Result<TypeId, FactQueryError> {
    binding_context
        .resolve_symbol_query(SymbolQueryRequest::<ImplementationCoherenceQuery>::new(
            implementation,
        ))
        .map(|coherence| coherence.value().subject())
        .map_err(super::super::binder::binding_query_error)
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

pub(super) fn closed_array_length(
    values: &bray_symbols::SemanticValueStore,
    term_id: bray_symbols::ConstantTermId,
) -> Result<u64, CodegenPreparationError> {
    let term = values
        .constant_term_data(term_id)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    match term.as_ref() {
        ConstantTermData::Value(value) => {
            let data = values
                .constant_value_data(*value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let ConstantValueKind::Integer(value) = data.kind() else {
                return Err(CodegenPreparationError::InvalidArrayLength(term_id));
            };

            value
                .to_u64()
                .ok_or(CodegenPreparationError::InvalidArrayLength(term_id))
        }
        ConstantTermData::IntegerLiteral { value, .. } => value
            .to_u64()
            .ok_or(CodegenPreparationError::InvalidArrayLength(term_id)),
        _ => Err(CodegenPreparationError::OpenConstantTerm(term_id)),
    }
}

fn native_boundary_mapping(
    symbol: &str,
    direction: ForeignCallableDirection,
) -> Result<(BinarySymbolName, CodegenLinkage), CodegenPreparationError> {
    let name =
        BinarySymbolName::try_new(symbol).ok_or(CodegenPreparationError::InvalidSymbolName)?;

    let linkage = match direction {
        ForeignCallableDirection::Import => CodegenLinkage::Import,
        ForeignCallableDirection::Export => CodegenLinkage::Export,
    };

    Ok((name, linkage))
}

fn scalar_mapping(
    compilation: &Compilation,
    ty: TypeId,
    role: RepresentationRole,
    scalar: TargetScalarKind,
    target: &CodegenTarget,
    cancellation: &CancellationToken,
    mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
    pending: &mut BTreeSet<TypeId>,
) -> Result<CodegenTypeMapping, CodegenPreparationError> {
    if !target.profile().properties().scalars().supports(scalar) {
        return Err(CodegenPreparationError::UnsupportedType(ty));
    }

    if let Some(component) = role.complex_component() {
        let component = compilation.compiler_known_type(component)?;

        return compilation.codegen_aggregate_type(
            ty,
            [(None, component), (None, component)],
            TargetLayoutContract::Default,
            Some(
                target
                    .profile()
                    .properties()
                    .scalars()
                    .alignment(scalar)
                    .get(),
            ),
            None,
            target,
            cancellation,
            mappings,
            pending,
        );
    }

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
        | TargetScalarKind::C256 => return Err(CodegenPreparationError::UnresolvedType(ty)),
    };

    Ok(CodegenTypeMapping::new(
        ty,
        TargetValueLayout::new(
            size,
            target.profile().properties().scalars().alignment(scalar),
            TargetLayoutContract::Default,
        ),
        kind,
    ))
}

fn pointer_mapping(
    ty: TypeId,
    pointee: TypeId,
    target: &CodegenTarget,
    address_space: TargetAddressSpaceKind,
) -> CodegenTypeMapping {
    CodegenTypeMapping::new(
        ty,
        pointer_layout(target),
        CodegenTypeKind::Pointer {
            target: pointee,
            address_space,
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
) -> Result<CodegenCallableSignature, CodegenPreparationError> {
    let result_type = if callable.execution() == CallableExecution::Asynchronous {
        compilation
            .available_compiler_known_symbols()
            .unary_representation_type(
                compilation.semantic_value_store()?,
                RepresentationRole::Future,
                callable.result(),
            )
            .ok_or(FactQueryError::InfrastructureFailure)?
    } else {
        callable.result()
    };

    let result = if is_void_result(compilation, result_type)? {
        CodegenResultMapping::Void
    } else {
        CodegenResultMapping::direct(result_type, None, [])
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

fn operation_result_type(mir: &MirUnit, operation: &MirOperation) -> Option<TypeId> {
    operation
        .result()
        .and_then(|result| mir.value(result))
        .map(bray_ir::MirValue::ty)
}

fn void_signature(abi: CallableAbi) -> CodegenCallableSignature {
    CodegenCallableSignature::new([], CodegenResultMapping::Void, abi, false)
}

fn lifecycle_operation_block_kind(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> Result<MirBlockKind, FactQueryError> {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Destroy => Ok(MirBlockKind::Ordinary),
        bray_ir::MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::TaskCancellation) => {
            Ok(MirBlockKind::CleanupBroadcast)
        }
        bray_ir::MirGeneratedLifecycleRole::Finalize
        | bray_ir::MirGeneratedLifecycleRole::Cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
        ) => Err(FactQueryError::InfrastructureFailure),
    }
}

fn receiver_codegen_type(
    values: &bray_symbols::SemanticValueStore,
    ty: TypeId,
    mode: ReceiverMode,
) -> Result<TypeId, FactQueryError> {
    let data = match mode {
        ReceiverMode::Shared => Some(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: ty,
        }),
        ReceiverMode::Mutable => Some(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: ty,
        }),
        ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => None,
    };

    match data {
        Some(data) => values
            .intern_type(data)
            .map_err(|_| FactQueryError::InfrastructureFailure),
        None => Ok(ty),
    }
}

fn projected_lifecycle_place(parent: &MirPlace, kind: MirProjectionKind, ty: TypeId) -> MirPlace {
    let mut projections = parent.projections().to_vec();
    projections.push(MirProjection::new(kind, parent.ty(), ty));

    MirPlace::new(parent.storage(), projections, ty)
}

fn helper_runtime_symbol(owner: &CodegenInstance, role: RuntimeAbiRole) -> CodegenSymbolKey {
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
        | MirHelperReference::DeclaredCallable(_)
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
    symbols: &[CodegenSymbolMapping],
) -> BTreeSet<MirRuntimeReference> {
    mapped_runtime_references(unit, operations, symbols)
}

fn dependency_symbol(
    owner: &CodegenInstance,
    instance: &bray_codegen::CodegenInstanceKey,
    reference: &MirHelperReference,
) -> Result<CodegenSymbolKey, CodegenPreparationError> {
    owner
        .dependencies()
        .iter()
        .find(|dependency| dependency.instance() == instance)
        .map(|dependency| CodegenSymbolKey::Instance(dependency.instance().clone()))
        .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))
}

fn is_void_result(compilation: &Compilation, ty: TypeId) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(false);
    };

    Ok(matches!(
        super::super::foreign::compiler_known_representation(compilation, *definition),
        Some(RepresentationRole::Unit | RepresentationRole::Never)
    ))
}

fn sized_layout(
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
    ty: TypeId,
) -> Result<TargetValueLayout, CodegenPreparationError> {
    mappings
        .get(&ty)
        .and_then(CodegenTypeMapping::layout)
        .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))
}

fn ensure_target_alignment(
    ty: TypeId,
    alignment: NonZeroU64,
    target: &CodegenTarget,
) -> Result<(), CodegenPreparationError> {
    if alignment > target.profile().properties().alignments().max_storage() {
        return Err(CodegenPreparationError::UnsupportedType(ty));
    }

    Ok(())
}

fn indirect_abi_value(
    abi: CallableAbi,
    kind: &CodegenTypeKind,
    layout: TargetValueLayout,
    target: &CodegenTarget,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    let is_composite = matches!(
        kind,
        CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. }
    );

    if !is_composite {
        return false;
    }

    if abi == CallableAbi::Bray {
        let register_pair_bytes = pointer_layout(target).size().saturating_mul(2);

        return layout.size() > register_pair_bytes;
    }

    if target.profile().machine().architecture() == bray_target::TargetArchitecture::Aarch64
        && is_homogeneous_float_aggregate(kind, mappings)
    {
        return false;
    }

    match bray_target::NativeTarget::for_profile(target.profile()) {
        Some(bray_target::NativeTarget::X86_64WindowsMsvc) => {
            !matches!(layout.size(), 1 | 2 | 4 | 8)
        }
        _ => layout.size() > pointer_layout(target).size().saturating_mul(2),
    }
}

fn indirect_parameter_kind(
    abi: CallableAbi,
    target: &CodegenTarget,
) -> CodegenIndirectParameterKind {
    if abi != CallableAbi::Bray
        && (target.profile().machine().architecture() == bray_target::TargetArchitecture::Aarch64
            || matches!(
                bray_target::NativeTarget::for_profile(target.profile()),
                Some(bray_target::NativeTarget::X86_64WindowsMsvc)
            ))
    {
        return CodegenIndirectParameterKind::Reference;
    }

    CodegenIndirectParameterKind::ByValue
}

fn is_homogeneous_float_aggregate(
    root: &CodegenTypeKind,
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
) -> bool {
    let mut pending = vec![(root, 1_u64)];
    let mut element_width = None;
    let mut element_count = 0_u64;

    while let Some((kind, multiplicity)) = pending.pop() {
        match kind {
            CodegenTypeKind::Float(width) => {
                if element_width.is_some_and(|element_width| element_width != *width) {
                    return false;
                }

                element_width = Some(*width);
                element_count = element_count.saturating_add(multiplicity);

                if element_count > 4 {
                    return false;
                }
            }
            CodegenTypeKind::Aggregate(fields) => {
                for field in fields.iter().rev() {
                    let Some(mapping) = mappings.get(&field.ty()) else {
                        return false;
                    };

                    pending.push((mapping.kind(), multiplicity));
                }
            }
            CodegenTypeKind::Array { element, length } => {
                let Some(mapping) = mappings.get(element) else {
                    return false;
                };

                let Some(multiplicity) = multiplicity.checked_mul(*length) else {
                    return false;
                };

                if multiplicity == 0 || multiplicity > 4 {
                    return false;
                }

                pending.push((mapping.kind(), multiplicity));
            }
            _ => return false,
        }
    }

    element_width.is_some() && element_count > 0
}

fn align_to(value: u64, alignment: NonZeroU64) -> Option<u64> {
    let mask = alignment.get().checked_sub(1)?;

    value.checked_add(mask).map(|value| value & !mask)
}

fn packed_alignment(alignment: NonZeroU64, packing: Option<NonZeroU64>) -> NonZeroU64 {
    packing.map_or(alignment, |packing| alignment.min(packing))
}

fn nonzero_width(width: u16) -> NonZeroU16 {
    NonZeroU16::new(width).unwrap_or(NonZeroU16::MIN)
}

pub(in crate::compilation) fn generated_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    category: &str,
    identity: &impl Hash,
) -> Result<BinarySymbolName, CodegenPreparationError> {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.codegen-symbol");
    category.hash(&mut hasher);
    identity.hash(&mut hasher);

    binary_symbol_name(target, linkage, category, hasher.finalize())
}

fn generated_instance_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    instance: &bray_codegen::CodegenInstanceKey,
) -> Result<BinarySymbolName, CodegenPreparationError> {
    let category = if matches!(instance.template(), MirUnitKey::GeneratedLifecycle(_)) {
        "lifecycle"
    } else {
        "instance"
    };

    generated_symbol_name(target, linkage, category, instance)
}

pub(in crate::compilation) fn generated_frame_symbol_name(
    target: &CodegenTarget,
    frame: bray_runtime_interface::ProtectedAsyncFrameId,
    operation: ProtectedFrameOperation,
) -> Result<BinarySymbolName, CodegenPreparationError> {
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
) -> Result<BinarySymbolName, CodegenPreparationError> {
    let prefix = if linkage == CodegenLinkage::Private {
        target.symbols().private_prefix()
    } else {
        target.symbols().global_prefix()
    };

    let mut name = format!("{prefix}bray_{category}_");

    for byte in digest {
        let _ = write!(name, "{byte:02x}");
    }

    BinarySymbolName::try_new(name).ok_or(CodegenPreparationError::InvalidSymbolName)
}

fn codegen_checker_error(error: bray_checker::CheckerQueryError) -> CodegenPreparationError {
    match error {
        bray_checker::CheckerQueryError::Cancelled => FactQueryError::Cancelled.into(),
        bray_checker::CheckerQueryError::Infrastructure(error) => {
            FactQueryError::CheckerInfrastructure(error).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::{NonZeroU16, NonZeroU64};

    use bray_codegen::{
        CodegenCallableSignature, CodegenFieldLayout, CodegenIndirectParameterKind,
        CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenParameterMapping,
        CodegenResultMapping, CodegenSymbolKey, CodegenTarget, CodegenTypeKind, CodegenTypeMapping,
        TargetAddressSpaceKind,
    };
    use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
    use bray_ir::{
        MirBlockKind, MirCleanupPhase, MirFrameReference, MirGeneratorOperation,
        MirHelperReference, MirOperationKind, MirProjectionKind, MirRuntimeReference,
        MirTerminatorKind, MirUnit, MirUnitId,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole, RuntimeRoleContractEffect,
    };
    use bray_symbols::testing::intern_type;
    use bray_symbols::{
        BorrowKind, CallableAbi, ImplementationSymbolId, InherentImplementationSymbolId,
        NamedTypeSymbolId, ReceiverMode, SelfTypeContext, SemanticValueStore, SymbolId,
        SymbolOrigin, TraitApplicationData, TypeData, TypeId,
    };
    use bray_target::{NativeTarget, TargetLayoutContract, TargetValueLayout};
    use bray_testing::{test_mir_unit, test_mir_unit_for_target, test_mir_unit_with_declaration};

    use super::{
        dependency_symbol, direct_helper_symbol, indirect_abi_value, indirect_parameter_kind,
        is_void_result, named_type, pointer_layout, receiver_codegen_type,
        substitute_contextual_self,
    };
    use crate::compilation::CodegenPreparationError;
    use crate::compilation::product::specialization::ConcreteCodegenInstance;
    use crate::compilation::substitution::empty_substitution;
    use crate::test_support::compilation;
    use crate::{CancellationToken, Compilation, SelectedTarget};

    #[test]
    fn codegen_contextual_self_substitution_reaches_nested_type_forms() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let implementation = ImplementationSymbolId::Inherent(
            InherentImplementationSymbolId::from_symbol_id(SymbolId::new(1)),
        );

        let context = SelfTypeContext::Implementation(implementation);

        let contextual = values
            .intern_type(TypeData::ContextualSelf(context))
            .unwrap_or_else(|error| panic!("test contextual type must intern: {error:?}"));

        let nested = values
            .intern_type(TypeData::Nullable(contextual))
            .unwrap_or_else(|error| panic!("test nested type must intern: {error:?}"));

        let replacement = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test replacement type must intern: {error:?}"));

        let substituted = substitute_contextual_self(&values, nested, Some((context, replacement)))
            .unwrap_or_else(|error| panic!("test contextual type must substitute: {error:?}"));

        let data = values
            .type_data(substituted)
            .unwrap_or_else(|error| panic!("test substituted type must resolve: {error:?}"));

        assert_eq!(data.as_ref(), &TypeData::Nullable(replacement));
    }

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
                MirHelperReference::CommitAwaitedCompletion(MirFrameReference::Known(frame)),
                CodegenSymbolKey::ProtectedFrame {
                    frame,
                    operation: ProtectedFrameOperation::CompletionMove,
                },
            ),
            (
                MirHelperReference::CommitAwaitedCompletion(MirFrameReference::Erased),
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
        let compilation = compilation("module app; func main() {}");
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

        let Some(first_dependency) = dependencies.first() else {
            panic!("lifecycle helpers must produce dependencies");
        };

        let owner_mir = test_mir_unit_for_target(2, first_dependency.key().target().clone());

        let owner = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&owner_mir),
            owner_mir,
            dependencies
                .iter()
                .map(|dependency| CodegenInstanceDependency::definition(dependency.key().clone())),
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
    fn codegen_receiver_types_preserve_receiver_authority() {
        let compilation = compilation("module app; func main() {}");

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let receiver = values
            .intern_type(TypeData::tuple([]))
            .expect("receiver type must intern");

        let shared = receiver_codegen_type(values, receiver, ReceiverMode::Shared)
            .expect("shared receiver must resolve");

        let mutable = receiver_codegen_type(values, receiver, ReceiverMode::Mutable)
            .expect("mutable receiver must resolve");

        assert_eq!(
            values
                .type_data(shared)
                .expect("shared type must resolve")
                .as_ref(),
            &TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: receiver,
            }
        );

        assert_eq!(
            values
                .type_data(mutable)
                .expect("mutable type must resolve")
                .as_ref(),
            &TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: receiver,
            }
        );

        assert_eq!(
            receiver_codegen_type(values, receiver, ReceiverMode::Consuming),
            Ok(receiver)
        );

        assert_eq!(
            receiver_codegen_type(values, receiver, ReceiverMode::ConsumingMutable),
            Ok(receiver)
        );
    }

    #[test]
    fn lifecycle_identity_is_stable_and_payload_remains_compilation_local() {
        let first = compilation("module app; func main() {}");
        let second = compilation("module app; func main() {}");

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
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let ty = compilation
            .semantic_value_store()
            .expect("semantic values must resolve")
            .intern_type(TypeData::tuple([]))
            .expect("test type must intern");

        let finalize = compilation
            .concrete_codegen_lifecycle(MirHelperReference::Finalize(ty), &target)
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
        let compilation = compilation("module app; func main() {}");
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
            .concrete_codegen_lifecycle(MirHelperReference::Destroy(aggregate), &target)
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
    fn nullable_lifecycle_resolves_only_the_present_payload() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let payload = values
            .intern_type(TypeData::tuple([]))
            .expect("payload type must intern");

        let nullable = values
            .intern_type(TypeData::Nullable(payload))
            .expect("nullable type must intern");

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Destroy(nullable),
            78,
        );

        let branch = generated
            .blocks()
            .iter()
            .find_map(|block| match block.terminator().kind() {
                MirTerminatorKind::PatternBranch {
                    predicate: bray_ir::MirPatternPredicate::NullablePresent,
                    matched,
                    unmatched,
                    ..
                } => Some((matched.target(), unmatched.target())),
                _ => None,
            })
            .expect("nullable destruction must branch on presence");

        let present = generated.block(branch.0).expect("present block must exist");

        let absent = generated.block(branch.1).expect("absent block must exist");

        assert_eq!(present.operations().len(), 2);
        assert!(absent.operations().is_empty());

        for (operation, expected) in present.operations().iter().zip(["finalize", "destroy"]) {
            let operation = generated
                .operation(*operation)
                .expect("present lifecycle operation must exist");

            let place = match (expected, operation.kind()) {
                ("finalize", MirOperationKind::Finalize(place))
                | ("destroy", MirOperationKind::Destroy(place)) => place,
                other => panic!("unexpected nullable lifecycle operation: {other:?}"),
            };

            assert!(matches!(
                place
                    .projections()
                    .last()
                    .map(|projection| projection.kind()),
                Some(MirProjectionKind::NullableValue)
            ));
        }
    }

    #[test]
    fn nullable_cancellation_branches_within_the_cleanup_phase() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let payload = values
            .intern_type(TypeData::tuple([]))
            .expect("payload type must intern");

        let nullable = values
            .intern_type(TypeData::Nullable(payload))
            .expect("nullable type must intern");

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty: nullable,
            },
            82,
        );

        let branch = generated
            .blocks()
            .iter()
            .find_map(|block| match block.terminator().kind() {
                MirTerminatorKind::PatternBranch {
                    predicate: bray_ir::MirPatternPredicate::NullablePresent,
                    matched,
                    unmatched,
                    ..
                } => Some((matched.target(), unmatched.target())),
                _ => None,
            })
            .expect("nullable cleanup must branch on presence");

        let present = generated
            .block(branch.0)
            .expect("present cleanup block must exist");

        let absent = generated
            .block(branch.1)
            .expect("absent cleanup block must exist");

        assert_eq!(present.kind(), MirBlockKind::CleanupBroadcast);
        assert_eq!(present.operations().len(), 1);
        assert!(absent.operations().is_empty());

        let operation = generated
            .operation(present.operations()[0])
            .expect("present cleanup operation must exist");

        assert!(matches!(
            operation.kind(),
            MirOperationKind::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                place,
            } if matches!(
                place.projections().last().map(|projection| projection.kind()),
                Some(MirProjectionKind::NullableValue)
            )
        ));
    }

    #[test]
    fn union_lifecycle_resolves_only_the_active_variant_payload() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    Value(value: i32);\n",
            "    Empty;\n",
            "}\n",
            "func main() {}\n",
        ));

        let target = codegen_target(&compilation);
        let union = source_union_type(&compilation);

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Destroy(union),
            79,
        );

        let mut branches = generated
            .blocks()
            .iter()
            .filter_map(|block| match block.terminator().kind() {
                MirTerminatorKind::PatternBranch {
                    predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(variant),
                    matched,
                    ..
                } => Some((*variant, matched.target())),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(branches.len(), 2);

        branches.sort_by_key(|(variant, _)| *variant);

        let payload_blocks = branches
            .into_iter()
            .map(|(_, block)| {
                generated
                    .block(block)
                    .expect("variant lifecycle block must exist")
            })
            .collect::<Vec<_>>();

        assert_eq!(
            payload_blocks
                .iter()
                .map(|block| block.operations().len())
                .collect::<Vec<_>>(),
            [2, 0]
        );

        for (operation, expected) in payload_blocks[0]
            .operations()
            .iter()
            .zip(["finalize", "destroy"])
        {
            let operation = generated
                .operation(*operation)
                .expect("active payload operation must exist");

            let place = match (expected, operation.kind()) {
                ("finalize", MirOperationKind::Finalize(place))
                | ("destroy", MirOperationKind::Destroy(place)) => place,
                other => panic!("unexpected union lifecycle operation: {other:?}"),
            };

            assert!(matches!(
                place
                    .projections()
                    .last()
                    .map(|projection| projection.kind()),
                Some(MirProjectionKind::ActiveUnionPayloadField { .. })
            ));
        }

        assert!(
            generated.blocks().iter().any(|block| {
                matches!(block.terminator().kind(), MirTerminatorKind::Unreachable)
            })
        );
    }

    #[test]
    fn nullable_and_union_codegen_use_checked_payload_layouts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "@layout(stable, tag = u8)\n",
            "union Choice\n",
            "{\n",
            "    @tag(3)\n",
            "    Value(value: i32);\n",
            "    @tag(7)\n",
            "    Empty;\n",
            "}\n",
            "func main() {}\n",
        ));

        let target = codegen_target(&compilation);
        let union = source_union_type(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let nullable = values
            .intern_type(TypeData::Nullable(union))
            .expect("nullable union type must intern");

        let mut mappings = BTreeMap::new();
        let mut pending = BTreeSet::new();

        compilation
            .codegen_type(
                nullable,
                &target,
                &CancellationToken::new(),
                &mut mappings,
                &mut pending,
            )
            .expect("nullable union must realize");

        let nullable = mappings
            .get(&nullable)
            .expect("nullable mapping must be present");

        assert!(matches!(
            nullable.kind(),
            CodegenTypeKind::Aggregate(fields) if fields.len() == 2
        ));

        let union = mappings.get(&union).expect("union mapping must be present");

        assert!(matches!(
            union.kind(),
            CodegenTypeKind::Union { variants, .. }
                if variants
                    .iter()
                    .map(|variant| variant.tag().to_u64())
                    .collect::<Vec<_>>()
                    == [Some(3), Some(7)]
        ));
    }

    #[test]
    fn generator_destruction_reaches_required_element_lifecycle_and_releases_storage() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let leaf = values
            .intern_type(TypeData::tuple([]))
            .expect("generator leaf type must intern");

        let element = values
            .intern_type(TypeData::Generator(leaf))
            .expect("nontrivial generator element type must intern");

        let generator = values
            .intern_type(TypeData::Generator(element))
            .expect("outer generator type must intern");

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Destroy(generator),
            80,
        );

        let [operation] = generated.operations() else {
            panic!("generator destruction must contain one represented operation");
        };

        let MirOperationKind::Generator(MirGeneratorOperation::Destroy {
            element: operation_element,
            runtime,
            ..
        }) = operation.kind()
        else {
            panic!("generator destruction must use the generator destruction ABI");
        };

        assert_eq!(*operation_element, element);
        assert_eq!(runtime.role(), RuntimeAbiRole::GeneratorDestruction);

        assert_eq!(
            runtime.role().contract().effects(),
            &[RuntimeRoleContractEffect::DestroyGenerator]
        );

        assert_eq!(
            operation.kind().helper_references(),
            [
                MirHelperReference::Finalize(element),
                MirHelperReference::Destroy(element),
            ]
        );

        let owner = compilation
            .concrete_codegen_lifecycle(MirHelperReference::Destroy(generator), &target)
            .expect("outer generator lifecycle instance must realize");

        let dependencies = compilation
            .concrete_codegen_dependencies_for_mir(
                &owner,
                &generated,
                &target,
                &CancellationToken::new(),
            )
            .expect("element lifecycle dependencies must realize");

        let [dependency] = dependencies.as_slice() else {
            panic!("only nontrivial element lifecycle dependencies must remain");
        };

        assert_eq!(
            dependency.generated_lifecycle_reference(),
            Some(&MirHelperReference::Destroy(element))
        );
    }

    #[test]
    fn generator_cleanup_broadcast_reaches_exact_element_cleanup() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let leaf = values
            .intern_type(TypeData::tuple([]))
            .expect("generator leaf type must intern");

        let element = values
            .intern_type(TypeData::Generator(leaf))
            .expect("nontrivial generator element type must intern");

        let generator = values
            .intern_type(TypeData::Generator(element))
            .expect("outer generator type must intern");

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty: generator,
            },
            83,
        );

        let operation = generated
            .operations()
            .iter()
            .find(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Generator(MirGeneratorOperation::CleanupBroadcast { .. })
                )
            })
            .expect("generator cleanup must contain one broadcast operation");

        let MirOperationKind::Generator(MirGeneratorOperation::CleanupBroadcast {
            element: operation_element,
            runtime,
            ..
        }) = operation.kind()
        else {
            unreachable!("the operation was selected by its exact variant");
        };

        assert_eq!(*operation_element, element);
        assert_eq!(runtime.role(), RuntimeAbiRole::GeneratorCleanupBroadcast);

        assert_eq!(
            operation.kind().helper_references(),
            [MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty: element,
            }]
        );
    }

    #[test]
    fn generator_runtime_helpers_use_stable_erased_abis() {
        let compilation = compilation("module app; func main() {}");

        let begin = compilation
            .codegen_runtime_signature(RuntimeAbiRole::GeneratorBegin)
            .expect("generator begin signature must realize");

        let destruction = compilation
            .codegen_runtime_signature(RuntimeAbiRole::GeneratorDestruction)
            .expect("generator destruction signature must realize");

        assert_eq!(begin.parameters().len(), 6);
        assert_eq!(destruction.parameters().len(), 3);
        assert_eq!(begin.result(), &CodegenResultMapping::Void);
        assert_eq!(destruction.result(), &CodegenResultMapping::Void);
    }

    #[test]
    fn compiler_known_pointers_retain_their_semantic_pointee_and_address_space() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let element = compilation
            .compiler_known_type(RepresentationRole::ScalarU8)
            .expect("byte representation must resolve");

        let pointer = |role| {
            compilation
                .available_compiler_known_symbols()
                .unary_representation_type(values, role, element)
                .unwrap_or_else(|| panic!("{role:?} pointer type must resolve"))
        };

        let raw = pointer(RepresentationRole::RawPointer);
        let device = pointer(RepresentationRole::DevicePointer);
        let mappings = realized_types(&compilation, &target, [raw, device]);

        assert!(matches!(
            mappings[&raw].kind(),
            CodegenTypeKind::Pointer {
                target,
                address_space: TargetAddressSpaceKind::Default,
            } if *target == element
        ));

        assert!(matches!(
            mappings[&device].kind(),
            CodegenTypeKind::Pointer {
                target,
                address_space: TargetAddressSpaceKind::Device,
            } if *target == element
        ));
    }

    #[test]
    fn never_returning_callables_use_void_codegen_results() {
        let compilation = compilation("module app; func main() {}");

        let never = compilation
            .compiler_known_type(RepresentationRole::Never)
            .unwrap_or_else(|error| panic!("never representation must resolve: {error:?}"));

        assert!(
            is_void_result(&compilation, never).unwrap_or_else(|error| panic!(
                "void result classification must resolve: {error:?}"
            ))
        );
    }

    #[test]
    fn cancellation_observation_runtime_helper_returns_boolean() {
        let compilation = compilation("module app; func main() {}");

        let signature = compilation
            .codegen_runtime_signature(RuntimeAbiRole::CurrentRunCancellationObservation)
            .expect("cancellation observation signature must realize");

        let boolean = compilation
            .codegen_representation_type(RepresentationRole::ScalarBool)
            .expect("boolean representation must realize");

        assert!(signature.parameters().is_empty());

        assert_eq!(
            signature.result(),
            &CodegenResultMapping::direct(boolean, None, [])
        );
    }

    #[test]
    fn generator_runtime_signatures_demand_concrete_opaque_pointers() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let signature = compilation
            .codegen_runtime_signature(RuntimeAbiRole::GeneratorBegin)
            .expect("generator begin signature must realize");

        let Some(CodegenParameterMapping::Direct { ty: pointer, .. }) =
            signature.parameters().first()
        else {
            panic!("generator begin must receive one direct state pointer");
        };

        let element = compilation
            .compiler_known_type(RepresentationRole::ScalarU8)
            .expect("byte representation must resolve");

        let mappings = realized_types(&compilation, &target, [*pointer]);

        assert!(matches!(
            mappings[pointer].kind(),
            CodegenTypeKind::Pointer {
                target,
                address_space: TargetAddressSpaceKind::Default,
            } if *target == element
        ));
    }

    #[test]
    fn owned_indirection_uses_storage_policy_teardown() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let payload = values
            .intern_type(TypeData::tuple([]))
            .expect("owned payload type must intern");

        let heap_key = CompilerKnownDeclarationKey::try_new("Heap")
            .expect("compiler-known Heap key must validate");

        let heap = compilation
            .available_compiler_known_symbols()
            .declaration_symbol::<bray_symbols::StructSymbolId>(&heap_key)
            .expect("compiler-known Heap must be available");

        let storage = named_type(values, NamedTypeSymbolId::Struct(heap))
            .expect("Heap storage type must intern");

        let owned = values
            .intern_type(TypeData::OwnedIndirection {
                storage,
                target: payload,
            })
            .expect("owned indirection type must intern");

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Destroy(owned),
            81,
        );

        assert_eq!(
            generated
                .operations()
                .iter()
                .filter(|operation| matches!(operation.kind(), MirOperationKind::Call(_)))
                .count(),
            3
        );

        assert!(generated.operations().iter().any(|operation| {
            match operation.kind() {
                MirOperationKind::Borrow { place, .. } | MirOperationKind::Finalize(place) => place
                    .projections()
                    .iter()
                    .any(|projection| projection.kind() == &MirProjectionKind::OwnedStorage),
                _ => false,
            }
        }));
    }

    #[test]
    fn declaration_helpers_require_the_exact_concrete_dependency() {
        let owner_mir = test_mir_unit(3);

        let dependency = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(4, 5));

        let owner = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&owner_mir),
            owner_mir,
            [CodegenInstanceDependency::definition(dependency.clone())],
        )
        .expect("test helper dependency must validate");

        let reference = MirHelperReference::AnonymousCallable(match dependency.template() {
            bray_ir::MirUnitKey::Bound(unit) => {
                bray_ir::MirAnonymousCallableReference::bound(unit.clone())
            }
            bray_ir::MirUnitKey::ExecutableHost(_)
            | bray_ir::MirUnitKey::GeneratedLifecycle(_)
            | bray_ir::MirUnitKey::ImportedExecutable(_)
            | bray_ir::MirUnitKey::ExternalCallable(_)
            | bray_ir::MirUnitKey::ExternalRuntimeDefault(_) => {
                panic!("test dependency must be bound");
            }
        });

        assert_eq!(
            dependency_symbol(&owner, &dependency, &reference),
            Ok(CodegenSymbolKey::Instance(dependency.clone()))
        );

        let missing = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(6, 7));

        assert_eq!(
            dependency_symbol(&owner, &missing, &reference),
            Err(CodegenPreparationError::MissingHelperInstance(reference))
        );
    }

    fn codegen_target(compilation: &Compilation) -> CodegenTarget {
        compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("test codegen target must validate")
    }

    fn generated_lifecycle(
        compilation: &Compilation,
        target: &CodegenTarget,
        reference: MirHelperReference,
        unit: u32,
    ) -> MirUnit {
        let instance = compilation
            .concrete_codegen_lifecycle(reference, target)
            .expect("lifecycle instance must realize");

        let reference = instance
            .generated_lifecycle_reference()
            .expect("generated lifecycle payload must be retained");

        compilation
            .codegen_generated_lifecycle_mir(
                instance.key(),
                reference,
                MirUnitId::new(unit),
                &CancellationToken::new(),
            )
            .expect("generated lifecycle MIR must realize")
    }

    fn source_union_type(compilation: &Compilation) -> TypeId {
        let symbols = compilation
            .symbol_graph()
            .expect("test symbol graph must build");

        let union = symbols
            .unions()
            .iter()
            .find(|union| union.origin() == SymbolOrigin::Source)
            .expect("test source must declare one union");

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let substitution =
            empty_substitution(values, union.id().into()).expect("union substitution must intern");

        values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Union(union.id()),
                substitution,
            })
            .expect("union type must intern")
    }

    #[test]
    fn unsized_subjects_receive_layout_only_at_indirection_boundaries() {
        let compilation = compilation("module app;\ntrait Marker {}\n");
        let target = baseline_codegen_target();

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let scalar = compilation
            .compiler_known_type(RepresentationRole::ScalarI32)
            .unwrap_or_else(|error| panic!("i32 must be available: {error:?}"));

        let slice = intern_type(values, TypeData::Slice(scalar));

        let borrowed_slice = intern_type(
            values,
            TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: slice,
            },
        );

        let owned_slice = intern_type(
            values,
            TypeData::OwnedIndirection {
                storage: scalar,
                target: slice,
            },
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let marker = symbols
            .traits()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("fixture must declare Marker"));

        let substitution = empty_substitution(values, marker.id().into())
            .unwrap_or_else(|error| panic!("trait substitution must be available: {error:?}"));

        let application = values
            .intern_trait_application(TraitApplicationData::new(marker.id(), substitution))
            .unwrap_or_else(|error| panic!("trait application must be valid: {error:?}"));

        let view = intern_type(values, TypeData::TraitView(application));

        let borrowed_view = intern_type(
            values,
            TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: view,
            },
        );

        let owned_view = intern_type(
            values,
            TypeData::OwnedIndirection {
                storage: scalar,
                target: view,
            },
        );

        let mappings = realized_types(
            &compilation,
            &target,
            [borrowed_slice, owned_slice, borrowed_view, owned_view],
        );

        assert!(mappings[&slice].layout().is_none());

        assert!(matches!(
            mappings[&slice].kind(),
            CodegenTypeKind::UnsizedSlice { element } if *element == scalar
        ));

        assert!(mappings[&view].layout().is_none());

        assert!(matches!(
            mappings[&view].kind(),
            CodegenTypeKind::UnsizedTraitView
        ));

        for boundary in [borrowed_slice, owned_slice, borrowed_view, owned_view] {
            let mapping = &mappings[&boundary];

            assert_eq!(
                mapping.layout().map(TargetValueLayout::size),
                Some(pointer_layout(&target).size() * 2)
            );

            assert!(matches!(
                mapping.kind(),
                CodegenTypeKind::Aggregate(fields) if fields.len() == 2
            ));
        }
    }

    #[test]
    fn special_values_use_component_and_metadata_layouts() {
        let compilation = compilation("module app;\n");
        let target = baseline_codegen_target();

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let scalar = compilation
            .compiler_known_type(RepresentationRole::ScalarI32)
            .unwrap_or_else(|error| panic!("i32 must be available: {error:?}"));

        let real = compilation
            .compiler_known_type(RepresentationRole::ScalarR64)
            .unwrap_or_else(|error| panic!("r64 must be available: {error:?}"));

        let complex = compilation
            .compiler_known_type(RepresentationRole::ScalarC128)
            .unwrap_or_else(|error| panic!("c128 must be available: {error:?}"));

        let nullable = intern_type(values, TypeData::Nullable(scalar));
        let generator = intern_type(values, TypeData::Generator(scalar));

        let mappings = realized_types(&compilation, &target, [complex, nullable, generator]);

        let CodegenTypeKind::Aggregate(complex_fields) = mappings[&complex].kind() else {
            panic!("complex values must map to their two real components");
        };

        assert_eq!(
            complex_fields
                .iter()
                .map(|field| (field.ty(), field.offset_bytes()))
                .collect::<Vec<_>>(),
            [(real, 0), (real, 8)]
        );

        assert_eq!(
            mappings[&complex].layout().map(TargetValueLayout::size),
            Some(16)
        );

        let CodegenTypeKind::Aggregate(nullable_fields) = mappings[&nullable].kind() else {
            panic!("nullable values must map to state and payload");
        };

        assert_eq!(
            nullable_fields
                .iter()
                .map(CodegenFieldLayout::offset_bytes)
                .collect::<Vec<_>>(),
            [0, 4]
        );

        assert_eq!(
            mappings[&nullable].layout().map(TargetValueLayout::size),
            Some(8)
        );

        assert_eq!(
            mappings[&generator].layout().map(TargetValueLayout::size),
            Some(pointer_layout(&target).size() * 3)
        );
    }

    #[test]
    fn union_realization_uses_checked_tags_and_payload_layouts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "union Choice\n",
            "{\n",
            "    Empty;\n",
            "    Number(value: i64);\n",
            "    Pair(small: i8, large: i64);\n",
            "}\n",
        ));

        let target = baseline_codegen_target();

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let union = symbols
            .unions()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("fixture must declare Choice"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let ty = named_type(values, NamedTypeSymbolId::Union(union.id()))
            .unwrap_or_else(|error| panic!("union type must be available: {error:?}"));

        let mappings = realized_types(&compilation, &target, [ty]);
        let mapping = &mappings[&ty];

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            panic!("Choice must map to a tagged union");
        };

        assert_eq!(
            variants
                .iter()
                .map(|variant| variant.tag().to_u64())
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2)]
        );

        assert_eq!(
            variants
                .iter()
                .map(|variant| {
                    variant
                        .fields()
                        .iter()
                        .map(CodegenFieldLayout::offset_bytes)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            [vec![], vec![8], vec![8, 16]]
        );

        assert_eq!(mappings[tag].layout().map(TargetValueLayout::size), Some(1));

        assert_eq!(mapping.layout().map(TargetValueLayout::size), Some(24));
    }

    #[test]
    fn bray_abi_passes_large_composites_indirectly_and_rejects_unsized_values() {
        let compilation = compilation("module app;\n");
        let target = baseline_codegen_target();

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let scalar = compilation
            .compiler_known_type(RepresentationRole::ScalarI32)
            .unwrap_or_else(|error| panic!("i32 must be available: {error:?}"));

        let generator = intern_type(values, TypeData::Generator(scalar));
        let slice = intern_type(values, TypeData::Slice(scalar));
        let mut mappings = realized_types(&compilation, &target, [generator, slice]);
        let mut pending = BTreeSet::new();

        let signature = CodegenCallableSignature::new(
            [CodegenParameterMapping::direct(generator, None, [])],
            CodegenResultMapping::direct(generator, None, []),
            CallableAbi::Bray,
            false,
        );

        let classified = compilation
            .classify_codegen_signature(
                signature,
                &target,
                &CancellationToken::new(),
                &mut mappings,
                &mut pending,
            )
            .unwrap_or_else(|error| panic!("Bray ABI must classify: {error:?}"));

        assert!(matches!(
            classified.parameters(),
            [CodegenParameterMapping::Indirect {
                kind: CodegenIndirectParameterKind::ByValue,
                ..
            }]
        ));

        assert!(matches!(
            classified.result(),
            CodegenResultMapping::Indirect { .. }
        ));

        let unsized_signature = CodegenCallableSignature::new(
            [CodegenParameterMapping::direct(slice, None, [])],
            CodegenResultMapping::Void,
            CallableAbi::Bray,
            false,
        );

        assert_eq!(
            compilation.classify_codegen_signature(
                unsized_signature,
                &target,
                &CancellationToken::new(),
                &mut mappings,
                &mut pending,
            ),
            Err(CodegenPreparationError::UnsizedTypeByValue(slice))
        );
    }

    #[test]
    fn foreign_abi_uses_the_native_aggregate_passing_contract() {
        let aggregate = CodegenTypeKind::aggregate([]);
        let mappings = BTreeMap::new();

        let sixteen_bytes = TargetValueLayout::new(
            16,
            NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN),
            TargetLayoutContract::Default,
        );

        let windows = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);
        let linux = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);
        let aarch64 = CodegenTarget::for_native(NativeTarget::Aarch64LinuxGnu);

        assert!(indirect_abi_value(
            CallableAbi::C,
            &aggregate,
            sixteen_bytes,
            &windows,
            &mappings,
        ));

        assert!(!indirect_abi_value(
            CallableAbi::C,
            &aggregate,
            sixteen_bytes,
            &linux,
            &mappings,
        ));

        assert!(!indirect_abi_value(
            CallableAbi::Bray,
            &aggregate,
            sixteen_bytes,
            &windows,
            &mappings,
        ));

        assert_eq!(
            indirect_parameter_kind(CallableAbi::C, &windows),
            CodegenIndirectParameterKind::Reference
        );

        assert_eq!(
            indirect_parameter_kind(CallableAbi::C, &linux),
            CodegenIndirectParameterKind::ByValue
        );

        assert_eq!(
            indirect_parameter_kind(CallableAbi::C, &aarch64),
            CodegenIndirectParameterKind::Reference
        );

        assert_eq!(
            indirect_parameter_kind(CallableAbi::Bray, &windows),
            CodegenIndirectParameterKind::ByValue
        );
    }

    #[test]
    fn aarch64_foreign_abi_passes_homogeneous_float_aggregates_directly() {
        let compilation = compilation("module app;\n");

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let element = intern_type(values, TypeData::Error);
        let alignment = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let element_layout = TargetValueLayout::new(8, alignment, TargetLayoutContract::Default);

        let mappings = BTreeMap::from([(
            element,
            CodegenTypeMapping::new(
                element,
                element_layout,
                CodegenTypeKind::Float(NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)),
            ),
        )]);

        let aggregate = CodegenTypeKind::aggregate([
            CodegenFieldLayout::new(None, element, 0),
            CodegenFieldLayout::new(None, element, 8),
            CodegenFieldLayout::new(None, element, 16),
        ]);

        let aggregate_layout = TargetValueLayout::new(24, alignment, TargetLayoutContract::Default);

        for native in [
            NativeTarget::Aarch64LinuxGnu,
            NativeTarget::Aarch64WindowsMsvc,
            NativeTarget::Aarch64MacOs,
        ] {
            assert!(!indirect_abi_value(
                CallableAbi::C,
                &aggregate,
                aggregate_layout,
                &CodegenTarget::for_native(native),
                &mappings,
            ));
        }

        assert!(indirect_abi_value(
            CallableAbi::C,
            &aggregate,
            aggregate_layout,
            &CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu),
            &mappings,
        ));
    }

    #[test]
    fn callable_indirection_closes_recursive_value_layouts_before_abi_classification() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Node\n",
            "{\n",
            "    visit: func(pos node: Node) -> unit;\n",
            "}\n",
        ));

        let target = baseline_codegen_target();

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let node = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("fixture must declare Node"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let ty = named_type(values, NamedTypeSymbolId::Struct(node.id()))
            .unwrap_or_else(|error| panic!("Node type must be available: {error:?}"));

        let mappings = realized_types(&compilation, &target, [ty]);

        assert_eq!(
            mappings[&ty].layout().map(TargetValueLayout::size),
            Some(pointer_layout(&target).size())
        );
    }

    fn realized_types(
        compilation: &Compilation,
        target: &CodegenTarget,
        demanded: impl IntoIterator<Item = TypeId>,
    ) -> BTreeMap<TypeId, CodegenTypeMapping> {
        compilation
            .codegen_types(
                demanded.into_iter().collect(),
                None,
                target,
                &CancellationToken::new(),
            )
            .unwrap_or_else(|error| panic!("types must realize: {error:?}"))
            .into_iter()
            .map(|mapping| (mapping.ty(), mapping))
            .collect()
    }

    fn baseline_codegen_target() -> CodegenTarget {
        SelectedTarget::baseline()
            .codegen_target()
            .unwrap_or_else(|error| panic!("baseline codegen target must be valid: {error:?}"))
    }
}
