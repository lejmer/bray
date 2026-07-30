// rust-style: allow(module-too-large, reason = "the mapping tables share one recursive realization context and must remain auditable together")

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::num::{NonZeroU16, NonZeroU64};

use bray_base::StableDigestHasher;
use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_codegen::{
    CodegenCallableMapping, CodegenCallableSignature, CodegenConstantMapping,
    CodegenConstantTermMapping, CodegenFieldLayout, CodegenHelperMapping, CodegenLinkage,
    CodegenMappings, CodegenOperationMapping, CodegenParameterMapping, CodegenResultMapping,
    CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget, CodegenTerminatorMapping,
    CodegenTypeKind, CodegenTypeMapping, CodegenUnit, TargetAddressSpaceKind,
    child_constants, demanded_callable_references, demanded_constant_terms, demanded_constants,
    demanded_runtime_references, demanded_types,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirHelperReference, MirUnitKey, MirUnitKind};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ProtectedFrameOperation,
};
use bray_symbols::{
    CallableAbi, CallableDefinitionId, CallableSignatureFact, ConstantTermData,
    ConstantValueKind, GenericSubstitutionId, NamedTypeSymbolId, SymbolFactRequest, TypeData,
    TypeId,
};
use bray_target::{TargetLayoutContract, TargetScalarKind, TargetValueLayout};

use super::super::Compilation;
use super::super::CodegenFactError;
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
        let mut symbols = self.codegen_symbols(unit, executable_host, target, roots, cancellation)?;
        let callables = self.codegen_callables(unit, target, cancellation)?;
        let constants = self.codegen_constants(unit)?;

        let (constant_terms, terminators) = self.codegen_constant_terms(unit)?;

        let operations = self.codegen_operations(unit)?;

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

    fn codegen_operations(
        &self,
        unit: &CodegenUnit,
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
                    .map(|reference| self.codegen_helper(reference))
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
        reference: MirHelperReference,
    ) -> Result<CodegenHelperMapping, CodegenFactError> {
        let ty = match &reference {
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
        };

        if let Some(ty) = ty {
            if self.has_trivial_codegen_lifecycle(ty)? {
                return Ok(CodegenHelperMapping::lowered(reference));
            }
        }

        Err(CodegenFactError::UnsupportedHelper(reference))
    }

    fn has_trivial_codegen_lifecycle(&self, ty: TypeId) -> Result<bool, FactQueryError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let trivial = match data.as_ref() {
            TypeData::Named { definition, .. } => {
                super::super::foreign::compiler_known_representation(self, *definition)
                    .is_some_and(|role| {
                        matches!(
                            role,
                            RepresentationRole::Unit
                                | RepresentationRole::Never
                                | RepresentationRole::RawPointer
                        ) || super::super::representation::target_scalar(role).is_some()
                    })
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
                    let linkage = if roots.contains(instance.key()) {
                        CodegenLinkage::Export
                    } else {
                        CodegenLinkage::Internal
                    };

                    (
                        generated_symbol_name(
                            target,
                            linkage,
                            "instance",
                            instance.key(),
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
            symbols.push(CodegenSymbolMapping::new(
                CodegenSymbolKey::Instance(instance.clone()),
                generated_symbol_name(
                    target,
                    CodegenLinkage::Import,
                    "instance",
                    instance,
                )?,
                CodegenLinkage::Import,
                self.codegen_instance_signature(instance, cancellation)?,
            ));
        }

        for reference in demanded_runtime_references(unit) {
            let Some(binding) = executable_host.and_then(|host| host.role_binding(reference.role()))
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

        Ok(symbols)
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
            self.codegen_type(ty, target, cancellation, &mut mappings, &mut BTreeSet::new())?;
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
            TypeData::Tuple(elements) => {
                self.codegen_aggregate_type(
                    ty,
                    elements.iter().copied().map(|element| (None, element)),
                    TargetLayoutContract::Default,
                    target,
                    cancellation,
                    mappings,
                    pending,
                )?
            }
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
            TypeData::Borrow { target: pointee, .. }
            | TypeData::OwnedIndirection {
                target: pointee, ..
            } => pointer_mapping(ty, *pointee, target),
            TypeData::Callable(callable) => {
                let signature = callable_type_signature(callable);

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

                self.codegen_aggregate_type(
                    ty,
                    fields,
                    TargetLayoutContract::Default,
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
                TargetValueLayout::new(
                    0,
                    NonZeroU64::MIN,
                    TargetLayoutContract::Default,
                ),
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

            alignment = alignment.max(field_layout.alignment());

            offset = align_to(offset, field_layout.alignment())
                .ok_or(CodegenFactError::LayoutOverflow(ty))?;

            layouts.push(CodegenFieldLayout::new(reference, field, offset));

            offset = offset
                .checked_add(field_layout.size())
                .ok_or(CodegenFactError::LayoutOverflow(ty))?;
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
        let constants = self
            .checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

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

    fn codegen_instance_signature(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenFactError> {
        let definition = match instance.template() {
            MirUnitKey::Bound(key) => {
                let symbol = self
                    .symbol_graph()?
                    .symbol_for_key(key.declared_owner())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                CallableDefinitionId::try_new(symbol)
                    .ok_or(FactQueryError::InfrastructureFailure)?
            }
            MirUnitKey::ExternalCallable(definition) => *definition,
            MirUnitKey::ExecutableHost(_) => return Ok(void_signature(CallableAbi::Bray)),
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
            [
                template.value().callable_type(),
                template.value().result(),
            ],
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
            .chain(signature.parameters().iter().map(|parameter| parameter.ty()))
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
        TargetScalarKind::Char => (
            4,
            CodegenTypeKind::UnsignedInteger(nonzero_width(32)),
        ),
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
    callable: &bray_symbols::CallableTypeData,
) -> CodegenCallableSignature {
    CodegenCallableSignature::new(
        callable
            .parameters()
            .iter()
            .map(|parameter| CodegenParameterMapping::direct(parameter.ty(), None, [])),
        CodegenResultMapping::direct(callable.result(), None, []),
        callable.abi(),
        false,
    )
}

fn void_signature(abi: CallableAbi) -> CodegenCallableSignature {
    CodegenCallableSignature::new([], CodegenResultMapping::Void, abi, false)
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
