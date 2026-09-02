use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU16, NonZeroU64};

use bray_codegen::{
    CodegenCallableSignature, CodegenIndirectParameterKind, CodegenInstance, CodegenLinkage,
    CodegenOperationMapping, CodegenParameterMapping, CodegenResultMapping, CodegenSymbolKey,
    CodegenSymbolMapping, CodegenTarget, CodegenTypeKind, CodegenTypeMapping, CodegenUnit,
    TargetAddressSpaceKind, mapped_runtime_references,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockKind, MirFrameReference, MirHelperReference, MirOperation, MirPlace, MirProjection,
    MirProjectionKind, MirRuntimeReference, MirUnit,
};
use bray_runtime_interface::{BinarySymbolName, ProtectedFrameOperation, RuntimeAbiRole};
use bray_symbols::{
    BorrowKind, CallableAbi, CallableExecution, ConstantTermData, ConstantValueKind,
    DeclaredLayoutMode, ForeignCallableDirection, NamedTypeSymbolId, NativeSymbolBinding,
    ReceiverMode, SemanticValueStore, StructSymbolId, SymbolKey, SymbolKeyData, TypeData, TypeId,
};
use bray_target::{
    TargetAtomicRepresentation, TargetLayoutContract, TargetScalarKind, TargetValueLayout,
};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::super::substitution::named_type;
use super::symbols::NativeBoundaryMapping;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn codegen_representation_type(
        &self,
        role: RepresentationRole,
    ) -> Result<TypeId, FactQueryError> {
        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CompilerKnownRepresentation(role),
                    ProductDataKind::CompilerKnownRepresentation,
                )
            })?;

        named_type(
            self.semantic_value_store()?,
            NamedTypeSymbolId::Struct(definition),
        )
    }

    pub(super) fn codegen_opaque_pointer_type(&self) -> Result<TypeId, FactQueryError> {
        let element = self.codegen_representation_type(RepresentationRole::ScalarU8)?;

        self.available_compiler_known_symbols()
            .unary_representation_type(
                self.semantic_value_store()?,
                RepresentationRole::RawPointer,
                element,
            )
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::UnaryRepresentation {
                        role: RepresentationRole::RawPointer,
                        argument: element,
                    },
                    ProductDataKind::CompilerKnownRepresentation,
                )
                .into()
            })
    }
}

pub(super) fn atomic_storage_is_padding_free(
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
        CodegenTypeKind::Opaque
        | CodegenTypeKind::Union { .. }
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

pub(super) fn atomic_representation_for_type(
    compilation: &Compilation,
    ty: TypeId,
    target: &CodegenTarget,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAtomicRepresentation>, CodegenPreparationError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(FactQueryError::SemanticValueStore)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(None);
    };

    let role =
        super::super::super::foreign::compiler_known_representation(compilation, *definition);

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

pub(super) const fn atomic_storage_role(
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

pub(super) fn source_backed_symbol_key(key: &SymbolKey) -> bool {
    match key.data() {
        SymbolKeyData::SourceDeclaration { .. } => true,
        SymbolKeyData::Synthesized(synthesized) => source_backed_symbol_key(synthesized.subject()),
        SymbolKeyData::Root(_)
        | SymbolKeyData::Module { .. }
        | SymbolKeyData::CompilerKnownDeclaration { .. }
        | SymbolKeyData::External(_) => false,
    }
}

pub(super) const fn target_layout_contract(layout: DeclaredLayoutMode) -> TargetLayoutContract {
    match layout {
        DeclaredLayoutMode::Default => TargetLayoutContract::Default,
        DeclaredLayoutMode::Stable => TargetLayoutContract::Stable,
        DeclaredLayoutMode::C => TargetLayoutContract::C,
        DeclaredLayoutMode::Transparent => TargetLayoutContract::Transparent,
    }
}

pub(super) fn signature_types(
    signature: &CodegenCallableSignature,
) -> impl Iterator<Item = TypeId> + '_ {
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

pub(in crate::compilation::product) fn closed_array_length(
    values: &SemanticValueStore,
    term_id: bray_symbols::ConstantTermId,
) -> Result<u64, CodegenPreparationError> {
    let term = values
        .constant_term_data(term_id)
        .map_err(FactQueryError::SemanticValueStore)?;

    match term.as_ref() {
        ConstantTermData::Typed { term, .. } => closed_array_length(values, *term),
        ConstantTermData::Value(value) => {
            let data = values
                .constant_value_data(*value)
                .map_err(FactQueryError::SemanticValueStore)?;

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

pub(super) fn native_boundary_mapping(
    symbol: &str,
    direction: ForeignCallableDirection,
    binding: NativeSymbolBinding,
) -> Result<NativeBoundaryMapping, CodegenPreparationError> {
    let name =
        BinarySymbolName::try_new(symbol).ok_or(CodegenPreparationError::InvalidSymbolName)?;

    let linkage = match (direction, binding) {
        (ForeignCallableDirection::Import, NativeSymbolBinding::Strong) => CodegenLinkage::Import,
        (ForeignCallableDirection::Export, NativeSymbolBinding::Strong) => CodegenLinkage::Export,
        (ForeignCallableDirection::Export, NativeSymbolBinding::Weak) => CodegenLinkage::Weak,
        (ForeignCallableDirection::Import, NativeSymbolBinding::Weak) => {
            return Err(CodegenPreparationError::InvalidAbiMapping);
        }
    };

    Ok(match direction {
        ForeignCallableDirection::Import => NativeBoundaryMapping::Direct { name, linkage },
        ForeignCallableDirection::Export => NativeBoundaryMapping::Callback { name, linkage },
    })
}

pub(super) fn scalar_mapping(
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

pub(super) fn pointer_mapping(
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

pub(super) fn pointer_layout(target: &CodegenTarget) -> TargetValueLayout {
    TargetValueLayout::new(
        u64::from(target.machine().pointer_width_bits().get().div_ceil(8)),
        NonZeroU64::from(target.machine().pointer_alignment_bytes()),
        TargetLayoutContract::Default,
    )
}

pub(super) fn callable_type_signature(
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
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::UnaryRepresentation {
                        role: RepresentationRole::Future,
                        argument: callable.result(),
                    },
                    ProductDataKind::CompilerKnownRepresentation,
                )
            })?
    } else {
        callable.result()
    };

    let result = if is_void_result(compilation, result_type)? {
        CodegenResultMapping::Void
    } else {
        CodegenResultMapping::direct(result_type, None, [])
    };

    let signature = CodegenCallableSignature::new(
        callable
            .parameters()
            .iter()
            .map(|parameter| CodegenParameterMapping::direct(parameter.ty(), None, [])),
        result,
        callable.abi(),
        false,
    );

    Ok(synchronous_bray_signature(
        signature,
        callable.abi(),
        callable.execution(),
    ))
}

pub(super) fn synchronous_bray_signature(
    signature: CodegenCallableSignature,
    abi: CallableAbi,
    execution: CallableExecution,
) -> CodegenCallableSignature {
    if abi == CallableAbi::Bray && execution == CallableExecution::Synchronous {
        signature.with_panic_report_context()
    } else {
        signature
    }
}

pub(super) fn operation_result_type(mir: &MirUnit, operation: &MirOperation) -> Option<TypeId> {
    operation
        .result()
        .and_then(|result| mir.value(result))
        .map(bray_ir::MirValue::ty)
}

pub(super) fn void_signature(abi: CallableAbi) -> CodegenCallableSignature {
    CodegenCallableSignature::new([], CodegenResultMapping::Void, abi, false)
}

pub(super) fn lifecycle_operation_block_kind(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> Result<MirBlockKind, FactQueryError> {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Destroy => Ok(MirBlockKind::Ordinary),
        bray_ir::MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::TaskCancellation) => {
            Ok(MirBlockKind::CleanupBroadcast)
        }
        bray_ir::MirGeneratedLifecycleRole::Finalize
        | bray_ir::MirGeneratedLifecycleRole::StaticFinalize
        | bray_ir::MirGeneratedLifecycleRole::Cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
        ) => Err(ProductQueryFailure::UnsupportedLifecycleRole { role }.into()),
    }
}

pub(super) fn receiver_codegen_type(
    values: &SemanticValueStore,
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
            .map_err(FactQueryError::SemanticValueStore),
        None => Ok(ty),
    }
}

pub(super) fn projected_lifecycle_place(
    parent: &MirPlace,
    kind: MirProjectionKind,
    ty: TypeId,
) -> MirPlace {
    let mut projections = parent.projections().to_vec();
    projections.push(MirProjection::new(kind, parent.ty(), ty));

    MirPlace::new(parent.storage(), projections, ty)
}

pub(super) fn helper_runtime_symbol(
    owner: &CodegenInstance,
    role: RuntimeAbiRole,
) -> CodegenSymbolKey {
    CodegenSymbolKey::Runtime(MirRuntimeReference::new(
        role,
        owner.key().target().runtime_abi(),
    ))
}

pub(super) fn direct_helper_symbol(
    owner: &CodegenInstance,
    reference: &MirHelperReference,
) -> Option<CodegenSymbolKey> {
    if let Some(role) = reference.runtime_role() {
        return Some(helper_runtime_symbol(owner, role));
    }

    let symbol = match reference {
        MirHelperReference::MoveInactiveFrame(frame) => match frame {
            MirFrameReference::Known(frame) => CodegenSymbolKey::ProtectedFrame {
                frame: *frame,
                operation: ProtectedFrameOperation::MoveBeforeStart,
            },
            MirFrameReference::Erased => return None,
        },
        MirHelperReference::CommitAwaitedCompletion(frame) => match frame {
            MirFrameReference::Known(frame) => CodegenSymbolKey::ProtectedFrame {
                frame: *frame,
                operation: ProtectedFrameOperation::CompletionMove,
            },
            MirFrameReference::Erased => return None,
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
        | MirHelperReference::StandardLibrary(_)
        | MirHelperReference::Finalize(_)
        | MirHelperReference::StaticFinalize(_)
        | MirHelperReference::Destroy(_)
        | MirHelperReference::Cleanup { .. }
        | MirHelperReference::CreateFrame(_)
        | MirHelperReference::ComposeAwaitedFrame(_)
        | MirHelperReference::DestroyTerminalTask => return None,
    };

    Some(symbol)
}

pub(super) fn codegen_runtime_references(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
    symbols: &[CodegenSymbolMapping],
) -> BTreeSet<MirRuntimeReference> {
    mapped_runtime_references(unit, operations, symbols)
}

pub(super) fn dependency_symbol(
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

pub(super) fn is_void_result(
    compilation: &Compilation,
    ty: TypeId,
) -> Result<bool, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(FactQueryError::SemanticValueStore)?;

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return Ok(false);
    };

    Ok(matches!(
        super::super::super::foreign::compiler_known_representation(compilation, *definition),
        Some(RepresentationRole::Unit | RepresentationRole::Never)
    ))
}

pub(super) fn sized_layout(
    mappings: &BTreeMap<TypeId, CodegenTypeMapping>,
    ty: TypeId,
) -> Result<TargetValueLayout, CodegenPreparationError> {
    mappings
        .get(&ty)
        .and_then(CodegenTypeMapping::layout)
        .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))
}

pub(super) fn ensure_target_alignment(
    ty: TypeId,
    alignment: NonZeroU64,
    target: &CodegenTarget,
) -> Result<(), CodegenPreparationError> {
    if alignment > target.profile().properties().alignments().max_storage() {
        return Err(CodegenPreparationError::UnsupportedType(ty));
    }

    Ok(())
}

pub(super) fn indirect_abi_value(
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

pub(super) fn indirect_parameter_kind(
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

pub(super) fn is_homogeneous_float_aggregate(
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

pub(super) fn align_to(value: u64, alignment: NonZeroU64) -> Option<u64> {
    let mask = alignment.get().checked_sub(1)?;

    value.checked_add(mask).map(|value| value & !mask)
}

pub(super) fn packed_alignment(alignment: NonZeroU64, packing: Option<NonZeroU64>) -> NonZeroU64 {
    packing.map_or(alignment, |packing| alignment.min(packing))
}

pub(super) fn nonzero_width(width: u16) -> NonZeroU16 {
    NonZeroU16::new(width).unwrap_or(NonZeroU16::MIN)
}

pub(super) fn codegen_checker_error(
    error: bray_checker::CheckerQueryError<FactQueryError>,
) -> CodegenPreparationError {
    match error {
        bray_checker::CheckerQueryError::Cancelled => FactQueryError::Cancelled.into(),
        bray_checker::CheckerQueryError::Infrastructure(error) => {
            FactQueryError::CheckerInfrastructure(error).into()
        }
        bray_checker::CheckerQueryError::Upstream(error) => error.into(),
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
        MirBlockKind, MirCallTarget, MirCleanupPhase, MirFrameReference, MirGeneratorOperation,
        MirHelperReference, MirOperationKind, MirProjectionKind, MirRuntimeReference,
        MirTerminatorKind, MirUnit, MirUnitId,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole, RuntimeRoleContractEffect,
    };
    use bray_symbols::testing::intern_type;
    use bray_symbols::{
        BorrowKind, CallableAbi, NamedTypeSymbolId, ProductKind, ReceiverMode, SymbolOrigin,
        TraitApplicationData, TypeData, TypeId,
    };
    use bray_target::{NativeTarget, TargetLayoutContract, TargetValueLayout};
    use bray_testing::{test_mir_unit, test_mir_unit_for_target, test_mir_unit_with_declaration};

    use super::{
        dependency_symbol, direct_helper_symbol, indirect_abi_value, indirect_parameter_kind,
        is_void_result, pointer_layout, receiver_codegen_type,
    };
    use crate::compilation::CodegenPreparationError;
    use crate::compilation::product::specialization::ConcreteCodegenInstance;
    use crate::compilation::substitution::{empty_substitution, named_type};
    use crate::test_support::{compilation, compilation_with_product};
    use crate::{CancellationToken, Compilation, SelectedTarget};

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
    fn generated_panic_report_destruction_uses_the_non_reporting_runtime_role() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);

        let report = compilation
            .compiler_known_type(RepresentationRole::PanicReport)
            .expect("panic report representation must resolve");

        let generated = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Destroy(report),
            78,
        );

        let runtime_roles = generated.operations().iter().filter_map(|operation| {
            let MirOperationKind::Call(call) = operation.kind() else {
                return None;
            };

            let MirCallTarget::Runtime(runtime) = call.target() else {
                return None;
            };

            Some(runtime.role())
        });

        assert_eq!(
            runtime_roles.collect::<Vec<_>>(),
            [RuntimeAbiRole::PanicReportDestruction]
        );
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
                    .map(|variant| variant.tag().and_then(|tag| tag.to_u64()))
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
                .unwrap_or_else(|error| panic!("{role:?} pointer type must intern: {error:?}"))
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
    fn real_product_callable_roots_retain_signatures() {
        let source = concat!(
            "module app;\n",
            "static ANSWER: i32 = 42;\n",
            "func main() -> i32\n",
            "{\n",
            "    return ANSWER;\n",
            "}\n",
        );

        let compilation = compilation_with_product(source, ProductKind::Executable);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let cancellation = CancellationToken::new();
        let target = codegen_target(&compilation);

        let semantic = compilation
            .product_semantics()
            .unwrap_or_else(|error| panic!("product semantics must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("product roots must resolve: {error:?}"));

        assert!(roots.iter().any(|root| root.callable_instance().is_some()));

        for root in roots {
            compilation
                .codegen_instance_signature(&root, &cancellation)
                .unwrap_or_else(|error| panic!("root signature must realize: {error:?}"));
        }
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
    fn panic_report_transfer_runtime_helpers_use_the_owned_report_and_status_types() {
        let compilation = compilation("module app; func main() {}");

        let report = compilation
            .codegen_representation_type(RepresentationRole::PanicReport)
            .expect("panic report representation must realize");

        let status = compilation
            .codegen_representation_type(RepresentationRole::ScalarU32)
            .expect("status representation must realize");

        for role in [
            RuntimeAbiRole::PanicReporting,
            RuntimeAbiRole::PanicReportDestruction,
        ] {
            let signature = compilation
                .codegen_runtime_signature(role)
                .unwrap_or_else(|error| panic!("{role:?} signature must realize: {error:?}"));

            assert_eq!(
                signature.parameters(),
                [CodegenParameterMapping::direct(report, None, [])]
            );

            assert_eq!(
                signature.result(),
                &CodegenResultMapping::direct(status, None, [])
            );
        }
    }

    #[test]
    fn task_event_runtime_helpers_use_event_and_status_scalars() {
        let compilation = compilation("module app; func main() {}");

        let event = compilation
            .codegen_representation_type(RepresentationRole::ScalarUsize)
            .expect("task event representation must realize");

        let status = compilation
            .codegen_representation_type(RepresentationRole::ScalarU32)
            .expect("status representation must realize");

        let creation = compilation
            .codegen_runtime_signature(RuntimeAbiRole::TaskEventCreation)
            .expect("task event creation signature must realize");

        let signal = compilation
            .codegen_runtime_signature(RuntimeAbiRole::TaskEventSignal)
            .expect("task event signal signature must realize");

        let destruction = compilation
            .codegen_runtime_signature(RuntimeAbiRole::TaskEventDestruction)
            .expect("task event destruction signature must realize");

        assert!(creation.parameters().is_empty());

        assert_eq!(
            creation.result(),
            &CodegenResultMapping::direct(event, None, [])
        );

        for signature in [signal, destruction] {
            assert_eq!(
                signature.parameters(),
                [CodegenParameterMapping::direct(event, None, [])]
            );

            assert_eq!(
                signature.result(),
                &CodegenResultMapping::direct(status, None, [])
            );
        }
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
                .map(|variant| variant.tag().and_then(|tag| tag.to_u64()))
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

        let tag = tag.unwrap_or_else(|| panic!("Choice must include tag storage"));

        assert_eq!(
            mappings[&tag].layout().map(TargetValueLayout::size),
            Some(1)
        );

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

    #[test]
    fn raw_pointer_indirection_closes_recursive_value_layouts() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Node\n",
            "{\n",
            "    next: RawPointer<Node>;\n",
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
