// rust-style: allow(module-too-large, reason = "the mapping tables share one recursive realization context and must remain auditable together")

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU16, NonZeroU64};

use bray_base::StableDigestHasher;
use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_codegen::{
    CodegenCallableMapping, CodegenCallableSignature, CodegenConstantMapping, CodegenInstance,
    CodegenConstantTermMapping, CodegenFieldLayout, CodegenHelperMapping,
    CodegenInstanceDependency, CodegenInstanceKey, CodegenLinkage, CodegenMappings,
    CodegenOperationMapping, CodegenParameterMapping, CodegenResultMapping, CodegenSymbolKey,
    CodegenSymbolMapping, CodegenTarget, CodegenTerminatorMapping, CodegenTypeKind,
    CodegenTypeMapping, CodegenUnit, TargetAddressSpaceKind, child_constants,
    demanded_callable_references, demanded_callable_references_for_mir,
    demanded_constant_terms, demanded_constants, demanded_runtime_references, demanded_types,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirCallTarget, MirFrameInitializer, MirFrameReference, MirHelperReference,
    MirOperationKind, MirRuntimeReference, MirUnitKey, MirUnitKind,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ProtectedFrameOperation, RuntimeAbiRole,
};
use bray_symbols::{
    AnySymbolId, CallableAbi, CallableDefinitionId, CallableSignatureFact, ConstantTermData,
    ConstantValueKind, DeclaredLayoutMode, ForeignCallableDirection, GenericSubstitutionId,
    NamedTypeSymbolId, SymbolFactRequest, TypeData, TypeId,
};
use bray_target::{TargetLayoutContract, TargetScalarKind, TargetValueLayout};

use super::super::CodegenFactError;
use super::super::Compilation;
use super::super::substitution::empty_substitution;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn codegen_mappings_for_product(
        &self,
        unit: &CodegenUnit,
        executable_host: Option<&ExecutableHostContract>,
        target: &CodegenTarget,
        roots: &BTreeSet<bray_codegen::CodegenInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<CodegenMappings, CodegenFactError> {
        let callables = self.codegen_callables(unit, target, cancellation)?;
        let constants = self.codegen_constants(unit)?;

        let (constant_terms, terminators) = self.codegen_constant_terms(unit)?;

        let operations = self.codegen_operations(unit, target, cancellation)?;

        let mut symbols = self.codegen_symbols(
            unit,
            &operations,
            executable_host,
            target,
            roots,
            cancellation,
        )?;

        let mut demanded = demanded_types(unit);

        demanded.extend(constants.iter().map(|constant| constant.data().ty()));

        for symbol in &symbols {
            demanded.extend(signature_types(symbol.signature()));
        }

        let types = self.codegen_types(demanded, target, cancellation)?;

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

    pub(super) fn codegen_operations(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenOperationMapping>, CodegenFactError> {
        let mut mappings = Vec::new();

        for instance in unit.instances() {
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

    pub(super) fn codegen_runtime_references(
        &self,
        unit: &CodegenUnit,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<BTreeSet<MirRuntimeReference>, CodegenFactError> {
        let operations = self.codegen_operations(unit, target, cancellation)?;

        Ok(codegen_runtime_references(unit, &operations))
    }

    pub(super) fn codegen_instance_dependencies(
        &self,
        owner: &CodegenInstanceKey,
        mir: &bray_ir::MirUnit,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenInstanceDependency>, CodegenFactError> {
        let mut dependencies = demanded_callable_references_for_mir(mir)
            .into_iter()
            .map(|reference| {
                self.codegen_instance_key(reference.instance(), target, cancellation)
                    .map(CodegenInstanceDependency::definition)
            })
            .collect::<Result<Vec<_>, _>>()?;

        for reference in mir
            .operations()
            .iter()
            .flat_map(|operation| operation.kind().helper_references())
        {
            if let Some(instance) =
                self.codegen_helper_instance_key(owner, &reference, target, cancellation)?
            {
                dependencies.push(CodegenInstanceDependency::definition(instance));
            }
        }

        Ok(dependencies)
    }

    fn codegen_helper_instance_key(
        &self,
        owner: &CodegenInstanceKey,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<CodegenInstanceKey>, CodegenFactError> {
        let instance = match reference {
            MirHelperReference::AnonymousCallable(unit) => {
                concrete_bound_helper_instance(owner, unit.clone())
            }
            MirHelperReference::CallableDefault(provider) => concrete_bound_helper_instance(
                owner,
                self.runtime_default_unit((*provider).into(), reference)?,
            ),
            MirHelperReference::ConstructionDefault(provider) => concrete_bound_helper_instance(
                owner,
                self.runtime_default_unit(provider.symbol(), reference)?,
            ),
            MirHelperReference::TypeForm(callable)
            | MirHelperReference::Conversion(callable) => {
                self.codegen_instance_key(*callable, target, cancellation)?
            }
            MirHelperReference::BeginGenerator
            | MirHelperReference::PushGenerator
            | MirHelperReference::FinishGenerator
            | MirHelperReference::PanicReport
            | MirHelperReference::Finalize(_)
            | MirHelperReference::Destroy(_)
            | MirHelperReference::Cleanup { .. }
            | MirHelperReference::CreateFrame(_)
            | MirHelperReference::MoveInactiveFrame(_)
            | MirHelperReference::ComposeAwaitedFrame(_)
            | MirHelperReference::CommitAwaitedCompletion(_)
            | MirHelperReference::DestroyTerminalTask => return Ok(None),
        };

        Ok(Some(instance))
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

    fn codegen_helper(
        &self,
        owner: &CodegenInstance,
        operation: &MirOperationKind,
        reference: MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenHelperMapping, CodegenFactError> {
        if let Some(ty) = helper_lifecycle_type(&reference)
            && self.has_trivial_codegen_lifecycle(ty)?
        {
            return Ok(CodegenHelperMapping::lowered(reference));
        }

        if let Some(symbol) = direct_helper_symbol(owner, &reference) {
            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        if let Some(instance) =
            self.codegen_helper_instance_key(owner.key(), &reference, target, cancellation)?
        {
            let symbol = dependency_symbol(owner, &instance, &reference)?;

            return Ok(CodegenHelperMapping::new(reference, symbol));
        }

        if !matches!(reference, MirHelperReference::CreateFrame(_)) {
            return Err(CodegenFactError::MissingHelperInstance(reference));
        }

        let symbol =
            self.frame_creation_symbol(owner, operation, &reference, target, cancellation)?;

        Ok(CodegenHelperMapping::new(reference, symbol))
    }

    fn frame_creation_symbol(
        &self,
        owner: &CodegenInstance,
        operation: &MirOperationKind,
        reference: &MirHelperReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<CodegenSymbolKey, CodegenFactError> {
        let MirOperationKind::Async(MirAsyncOperation::CreateFrame { initializer, .. }) = operation
        else {
            return Err(CodegenFactError::MissingHelperInstance(reference.clone()));
        };

        match initializer {
            MirFrameInitializer::Callable(call) => match call.target() {
                MirCallTarget::Direct(callable) => {
                    let instance =
                        self.codegen_instance_key(callable.instance(), target, cancellation)?;

                    dependency_symbol(owner, &instance, reference)
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
                MirUnitKind::Synchronous | MirUnitKind::ProtectedAsyncFrame(_) => {
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
                        self.codegen_instance_signature(instance.key(), cancellation)?,
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
                self.codegen_instance_signature(instance, cancellation)?,
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

    fn codegen_native_boundary(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<(BinarySymbolName, CodegenLinkage)>, CodegenFactError> {
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
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenCallableMapping>, CodegenFactError> {
        demanded_callable_references(unit)
            .into_iter()
            .map(|(owner, reference)| {
                let instance =
                    self.codegen_instance_key(reference.instance(), target, cancellation)?;

                Ok(CodegenCallableMapping::new(owner, reference, instance))
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
            for term in demanded_constant_terms(instance.mir()) {
                let data = values
                    .constant_term_data(term)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let ConstantTermData::Value(value) = data.as_ref() else {
                    return Err(CodegenFactError::OpenConstantTerm(term));
                };

                resolved.insert((instance.key().clone(), term), *value);

                terms.push(CodegenConstantTermMapping::new(
                    instance.key().clone(),
                    term,
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
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenTypeMapping>, CodegenFactError> {
        let mut mappings = BTreeMap::new();

        for ty in demanded {
            self.codegen_type(
                ty,
                target,
                cancellation,
                &mut mappings,
                &mut BTreeSet::new(),
            )?;
        }

        Ok(mappings.into_values().collect())
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

    pub(in crate::compilation) fn codegen_instance_signature(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenFactError> {
        let definition = match instance.template() {
            MirUnitKey::ExecutableHost(_) => return Ok(void_signature(CallableAbi::Bray)),
            MirUnitKey::Bound(_) | MirUnitKey::ExternalCallable(_) => {
                self.codegen_callable_definition(instance)?
            }
        };

        if !matches!(
            instance.specialization(),
            bray_codegen::CodegenSpecialization::NonGeneric
        ) {
            return Err(CodegenFactError::UnsupportedSpecialization(
                instance.clone(),
            ));
        }

        let values = self.semantic_value_store()?;
        let substitution = empty_substitution(values, definition.symbol())?;
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
            MirUnitKey::ExecutableHost(_) => Err(FactQueryError::InfrastructureFailure),
        }
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

fn concrete_bound_helper_instance(
    owner: &CodegenInstanceKey,
    unit: bray_bound_tree::BoundUnitKey,
) -> CodegenInstanceKey {
    CodegenInstanceKey::new(
        MirUnitKey::Bound(unit),
        owner.specialization().clone(),
        owner.witnesses().iter().cloned(),
        owner.target().clone(),
    )
}

const fn helper_lifecycle_type(reference: &MirHelperReference) -> Option<TypeId> {
    match reference {
        MirHelperReference::Finalize(ty)
        | MirHelperReference::Destroy(ty)
        | MirHelperReference::Cleanup { ty, .. } => Some(*ty),
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
        | MirHelperReference::DestroyTerminalTask => None,
    }
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
    use bray_codegen::{
        CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenSymbolKey,
    };
    use bray_ir::{MirCleanupPhase, MirFrameReference, MirHelperReference, MirRuntimeReference};
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole,
    };
    use bray_testing::{test_mir_type, test_mir_unit, test_mir_unit_with_declaration};

    use super::{dependency_symbol, direct_helper_symbol};
    use crate::compilation::CodegenFactError;

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
    fn value_lifecycle_helpers_do_not_conflate_values_with_protected_frames() {
        let owner = CodegenInstance::non_generic(test_mir_unit(1));
        let ty = test_mir_type();

        let references = [
            MirHelperReference::Finalize(ty),
            MirHelperReference::Destroy(ty),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty,
            },
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty,
            },
        ];

        for reference in references {
            assert_eq!(direct_helper_symbol(&owner, &reference), None);
        }
    }

    #[test]
    fn declaration_helpers_require_the_exact_concrete_dependency() {
        let owner_mir = test_mir_unit(2);
        let dependency = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(3, 4));

        let owner = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&owner_mir),
            owner_mir,
            [CodegenInstanceDependency::definition(dependency.clone())],
        )
        .unwrap_or_else(|error| panic!("test helper dependency must validate: {error:?}"));

        let reference = MirHelperReference::AnonymousCallable(
            match dependency.template() {
                bray_ir::MirUnitKey::Bound(unit) => unit.clone(),
                bray_ir::MirUnitKey::ExecutableHost(_)
                | bray_ir::MirUnitKey::ExternalCallable(_) => {
                    panic!("test dependency must be bound")
                }
            },
        );

        assert_eq!(
            dependency_symbol(&owner, &dependency, &reference),
            Ok(CodegenSymbolKey::Instance(dependency.clone()))
        );

        let missing = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(5, 6));

        assert_eq!(
            dependency_symbol(&owner, &missing, &reference),
            Err(CodegenFactError::MissingHelperInstance(reference))
        );
    }
}
