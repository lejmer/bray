use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU16, NonZeroU64};

use bray_codegen::{
    CodegenCallableSignature, CodegenIndirectParameterKind, CodegenInstance, CodegenLinkage,
    CodegenOperationMapping, CodegenParameterMapping, CodegenResultMapping, CodegenSymbolKey,
    CodegenSymbolMapping, CodegenTarget, CodegenTypeKind, CodegenTypeMapping, CodegenUnit,
    TargetAddressSpaceKind, mapped_runtime_references,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirHelperReference, MirOperation, MirRuntimeReference, MirUnit};
use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole};
use bray_symbols::{
    BorrowKind, CallableAbi, CallableExecution, ConstantTermData, DeclaredLayoutMode,
    ForeignCallableDirection, NamedTypeSymbolId, NativeSymbolBinding, ReceiverMode,
    SemanticValueStore, StructSymbolId, SymbolKey, SymbolKeyData, TypeData, TypeId,
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

        self.codegen_unary_representation_type(RepresentationRole::RawPointer, element)
    }

    pub(super) fn codegen_unary_representation_type(
        &self,
        role: RepresentationRole,
        element: TypeId,
    ) -> Result<TypeId, FactQueryError> {
        self.available_compiler_known_symbols()
            .unary_representation_type(self.semantic_value_store()?, role, element)
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::UnaryRepresentation {
                        role,
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
    let integer = values
        .constant_term_integer(term_id)
        .map_err(FactQueryError::SemanticValueStore)?;

    if let Some(integer) = integer {
        return integer
            .to_u64()
            .ok_or(CodegenPreparationError::InvalidArrayLength(term_id));
    }

    let term = values
        .constant_term_data(term_id)
        .map_err(FactQueryError::SemanticValueStore)?;

    match term.as_ref() {
        ConstantTermData::Typed { term, .. } => closed_array_length(values, *term),
        ConstantTermData::Value(_) => Err(CodegenPreparationError::InvalidArrayLength(term_id)),
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
    reference
        .runtime_role()
        .map(|role| helper_runtime_symbol(owner, role))
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
        MirBlockKind, MirCallTarget, MirCleanupPhase, MirFrameReference, MirHelperReference,
        MirOperationKind, MirProjectionKind, MirRuntimeReference, MirTerminatorKind, MirUnit,
        MirUnitId,
    };
    use bray_runtime_interface::RuntimeAbiRole;
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
                MirHelperReference::ComposeAwaitedFrame(MirFrameReference::Erased),
                runtime(RuntimeAbiRole::AwaitedFrameComposition),
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
            .filter(|operation| {
                matches!(
                    operation,
                    MirOperationKind::Finalize(_) | MirOperationKind::Destroy(_)
                )
            })
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

    fn reachable_cleanup_operations(
        mir: &MirUnit,
        entry: bray_ir::MirBlockId,
    ) -> Vec<&MirOperationKind> {
        let mut pending = vec![entry];
        let mut visited = BTreeSet::new();
        let mut operations = BTreeMap::new();

        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }

            let block = mir.block(id).expect("cleanup block must exist");

            for id in block.operations() {
                let operation = mir
                    .operation(*id)
                    .expect("cleanup operation must exist")
                    .kind();

                if matches!(
                    operation,
                    MirOperationKind::Cleanup { .. }
                        | MirOperationKind::Finalize(_)
                        | MirOperationKind::Destroy(_)
                ) {
                    operations.insert(*id, operation);
                }
            }

            block
                .terminator()
                .kind()
                .for_each_successor(|target| pending.push(target));
        }

        operations.into_values().collect()
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

        let absent = generated.block(branch.1).expect("absent block must exist");

        let operations = reachable_cleanup_operations(&generated, branch.0);
        assert_eq!(operations.len(), 2);
        assert!(absent.operations().is_empty());

        for (operation, expected) in operations.into_iter().zip(["finalize", "destroy"]) {
            let place = match (expected, operation) {
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
        let operations = reachable_cleanup_operations(&generated, branch.0);
        assert_eq!(operations.len(), 1);
        assert!(absent.operations().is_empty());

        assert!(matches!(
            operations[0],
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
            .map(|(_, block)| reachable_cleanup_operations(&generated, block))
            .collect::<Vec<_>>();

        assert_eq!(
            payload_blocks.iter().map(Vec::len).collect::<Vec<_>>(),
            [2, 0]
        );

        for (operation, expected) in payload_blocks[0].iter().zip(["finalize", "destroy"]) {
            let place = match (expected, operation) {
                ("finalize", MirOperationKind::Finalize(place))
                | ("destroy", MirOperationKind::Destroy(place)) => place,
                other => panic!("unexpected union lifecycle operation: {other:?}"),
            };

            assert!(matches!(
                place
                    .projections()
                    .last()
                    .map(|projection| projection.kind()),
                Some(MirProjectionKind::ActiveUnionPayloadElement { .. })
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
    fn type_wide_completion_omits_generated_finalizer_but_retains_destruction() {
        for declaration in [
            r#"
            struct Resource
            {
                finalize()
                    executes(pure, total) {}

                destruct() {}
            }
            "#,
            r#"
            struct Resource
            {
                async finalize()
                    executes(pure, total) {}

                destruct() {}
            }
            "#,
            r#"
            struct Resource
            {
                async finalize() -> Result<unit, unit>
                    executes(pure, total)
                    ensures(result matches Ok(_))
                {
                    return Ok(unit);
                }

                destruct() {}
            }
            "#,
        ] {
            let compilation = compilation(&format!("module app; {declaration}"));
            let target = codegen_target(&compilation);
            let symbols = compilation.symbol_graph().unwrap();

            let resource = symbols
                .structures()
                .iter()
                .find(|symbol| symbol.origin() == SymbolOrigin::Source)
                .unwrap();

            let ty = compilation
                .semantic_value_store()
                .unwrap()
                .intern_open_named_type(symbols, resource.id().into())
                .unwrap()
                .unwrap();

            let finalizer =
                generated_lifecycle(&compilation, &target, MirHelperReference::Finalize(ty), 90);

            assert!(finalizer.frame_descriptor().is_none(), "{declaration}");

            assert!(
                finalizer.operations().is_empty(),
                "{declaration}: {:?}",
                finalizer.operations()
            );

            let cleanup = generated_lifecycle(
                &compilation,
                &target,
                MirHelperReference::Cleanup {
                    phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                    ty,
                },
                91,
            );

            let helpers = cleanup
                .operations()
                .iter()
                .flat_map(|operation| operation.kind().helper_references())
                .collect::<Vec<_>>();

            assert!(
                !helpers.contains(&MirHelperReference::Finalize(ty)),
                "{declaration}: {helpers:?}"
            );

            assert!(
                helpers.contains(&MirHelperReference::Destroy(ty)),
                "{declaration}: {helpers:?}"
            );
        }
    }

    #[test]
    fn generator_destruction_reaches_required_element_lifecycle_and_releases_storage() {
        let dependency = crate::test_support::runtime_standard_library_dependency(
            &crate::SelectedTarget::baseline(),
        );

        let compilation = crate::test_support::compilation_with_dependencies(
            "module app; func main() {}",
            [dependency],
        );

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

        assert!(
            generated
                .operations()
                .iter()
                .any(|operation| matches!(operation.kind(),
                    MirOperationKind::Destroy(place) if place.ty() == element
                ))
        );

        assert!(generated.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Memory(memory) if memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate
        )));

        assert!(
            !generated
                .operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Generator(_)))
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

        assert!(
            dependencies
                .iter()
                .any(|dependency| dependency.generated_lifecycle_reference()
                    == Some(&MirHelperReference::Destroy(element)))
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

        assert!(generated.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Cleanup { phase: MirCleanupPhase::TaskCancellation, place } if place.ty() == element
        )));

        assert!(!generated.operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Memory(memory) if memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate
        )));

        assert!(
            !generated
                .operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Generator(_)))
        );
    }

    #[test]
    fn generator_runtime_helpers_use_stable_erased_abis() {
        let compilation = compilation("module app; func main() {}");

        let begin = compilation
            .codegen_runtime_signature(RuntimeAbiRole::GeneratorBegin)
            .expect("generator begin signature must realize");

        assert_eq!(begin.parameters().len(), 6);
        assert_eq!(begin.result(), &CodegenResultMapping::Void);
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
    fn finalizer_incidents_quiesce_returned_owners_before_transfer() {
        use bray_ir::{
            MirAbandonmentAction, MirAsyncOperation, MirFrameInitializer,
            MirGeneratedLifecycleRole, MirOperand,
        };

        use bray_lowering::SyntheticLoweringContext;

        for (error, value, asynchronous) in [
            ("unit", "unit", false),
            ("i32", "42", false),
            ("bool", "true", false),
            ("Task<unit>", "work().start()", true),
            ("Future<unit>", "work()", true),
        ] {
            let mode = if error == "Task<unit>" { "async " } else { "" };

            let compilation = compilation(&format!(
                r#"
                module app;
                struct Resource
                {{
                    {mode}finalize() -> Result<unit, {error}>
                    {{
                        return Error({value});
                    }}
                }}
                async func work()
                {{
                }}
                func main()
                {{
                }}
                "#
            ));

            let target = codegen_target(&compilation);
            let symbols = compilation.symbol_graph().unwrap();

            let resource = symbols
                .structures()
                .iter()
                .find(|symbol| symbol.origin() == SymbolOrigin::Source)
                .unwrap();

            let ty = named_type(
                compilation.semantic_value_store().unwrap(),
                NamedTypeSymbolId::Struct(resource.id()),
            )
            .unwrap();

            let context = super::super::synthetic::CompilationSyntheticLoweringContext::new(
                &compilation,
                &compilation.state.cancellation,
            )
            .unwrap();

            let cleanup = context.cleanup_type_execution(ty).unwrap();

            assert_eq!(
                cleanup.finalization_execution(),
                Some(if asynchronous {
                    bray_symbols::CallableExecution::Asynchronous
                } else {
                    bray_symbols::CallableExecution::Synchronous
                }),
                "{error}"
            );

            let error_type = context
                .lifecycle_callable(ty, bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)
                .unwrap()
                .unwrap()
                .2;

            let [_, error_type] = context
                .compiler_known_symbols()
                .representation_type_arguments(
                    context.semantic_values(),
                    RepresentationRole::Result,
                    error_type,
                )
                .unwrap()
                .unwrap();

            let destruction = generated_lifecycle(
                &compilation,
                &target,
                MirHelperReference::Abandon {
                    action: MirAbandonmentAction::Destroy,
                    ty: error_type,
                },
                89,
            );

            assert!(destruction.frame_descriptor().is_none(), "{error}");

            let generated =
                generated_lifecycle(&compilation, &target, MirHelperReference::Finalize(ty), 87);

            assert_eq!(
                generated.frame_descriptor().is_some(),
                asynchronous,
                "{error}"
            );

            let (transfer_block, transfer) = generated
                .blocks_with_ids()
                .find_map(|(id, block)| {
                    block.operations().iter().find_map(|operation| {
                        match generated.operation(*operation).unwrap().kind() {
                            MirOperationKind::Async(
                                MirAsyncOperation::TransferCleanupIncident { incident, .. },
                            ) => Some((id, incident)),
                            _ => None,
                        }
                    })
                })
                .expect("finalizer error must transfer to an owned incident");

            assert!(matches!(transfer, MirOperand::Move(_)));

            assert!(
                generated
                    .operations()
                    .iter()
                    .any(|operation| match operation.kind() {
                        MirOperationKind::Abandon {
                            action: MirAbandonmentAction::Quiesce,
                            ..
                        } => !asynchronous,
                        MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                            initializer:
                                MirFrameInitializer::Lifecycle {
                                    role:
                                        MirGeneratedLifecycleRole::Abandon(
                                            MirAbandonmentAction::Quiesce,
                                        ),
                                    ..
                                },
                            ..
                        }) => asynchronous,
                        _ => false,
                    }),
                "{error}"
            );

            assert!(!generated.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Destroy(_)
                    | MirOperationKind::Abandon {
                        action: MirAbandonmentAction::Destroy,
                        ..
                    }
            )));

            // Failure propagation must not continue to the ownership-transfer block.
            for (id, block) in generated.blocks_with_ids().filter(|(_, block)| {
                matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::PropagatePanic { .. }
                        | MirTerminatorKind::PropagateCancellation { .. }
                )
            }) {
                assert!(
                    !generated.reachable_blocks([id]).contains(&transfer_block),
                    "{error}: {block:?}"
                );
            }

            let wrapper = generated_lifecycle(
                &compilation,
                &target,
                MirHelperReference::StaticFinalize(ty),
                88,
            );

            assert!(wrapper.frame_descriptor().is_none());

            assert_eq!(
                wrapper
                    .operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(MirAsyncOperation::TransferCleanupIncident { .. })
                    ))
                    .count(),
                usize::from(!asynchronous)
            );

            let generator = context
                .semantic_values()
                .intern_type(TypeData::Generator(ty))
                .unwrap();

            let accumulated = generated_lifecycle(
                &compilation,
                &target,
                MirHelperReference::Destroy(generator),
                90,
            );

            assert_eq!(
                accumulated.frame_descriptor().is_some(),
                asynchronous,
                "{error}"
            );

            assert!(
                accumulated
                    .operations()
                    .iter()
                    .any(|operation| match operation.kind() {
                        MirOperationKind::Finalize(place) => !asynchronous && place.ty() == ty,
                        MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                            initializer:
                                MirFrameInitializer::Lifecycle {
                                    role: MirGeneratedLifecycleRole::Finalize,
                                    ty: element,
                                    ..
                                },
                            ..
                        }) => asynchronous && *element == ty,
                        _ => false,
                    }),
                "{error}"
            );

            let abandoned = generated_lifecycle(
                &compilation,
                &target,
                MirHelperReference::Abandon {
                    action: MirAbandonmentAction::Destroy,
                    ty: generator,
                },
                91,
            );

            assert!(abandoned.frame_descriptor().is_none());

            assert!(
                !abandoned
                    .operations()
                    .iter()
                    .any(|operation| matches!(operation.kind(), MirOperationKind::Finalize(_)))
            );
        }
    }

    #[test]
    fn task_result_cleanup_checks_each_outcome_before_resolving_its_owner() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);
        let values = compilation.semantic_value_store().unwrap();
        let payload = values.intern_type(TypeData::tuple([])).unwrap();

        let task = compilation
            .available_compiler_known_symbols()
            .unary_representation_type(values, RepresentationRole::Task, payload)
            .unwrap()
            .unwrap();

        for reference in [
            MirHelperReference::Finalize(task),
            MirHelperReference::StaticFinalize(task),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty: task,
            },
        ] {
            let combined = matches!(reference, MirHelperReference::Cleanup { .. });
            let static_wrapper = matches!(reference, MirHelperReference::StaticFinalize(_));
            let generated = generated_lifecycle(&compilation, &target, reference, 82);

            assert_eq!(generated.frame_descriptor().is_some(), !static_wrapper);

            assert_eq!(
                generated
                    .operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(bray_ir::MirAsyncOperation::ResolveTask { .. })
                    ))
                    .count(),
                usize::from(!static_wrapper)
            );

            assert_eq!(
                generated
                    .blocks()
                    .iter()
                    .filter(|block| matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::Suspend {
                            kind: bray_ir::MirSuspensionKind::TaskCompletion,
                            cancellation: None,
                            ..
                        }
                    ))
                    .count(),
                usize::from(!static_wrapper)
            );

            assert_eq!(
                generated
                    .operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::DestroyTerminalTask { .. }
                        )
                    ))
                    .count(),
                usize::from(combined),
            );
        }
    }

    #[test]
    fn task_quiescence_borrows_terminal_values_until_nested_cleanup_rejoins() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);
        let values = compilation.semantic_value_store().unwrap();
        let payload = values.intern_type(TypeData::tuple([])).unwrap();

        let task_type = |payload| {
            compilation
                .available_compiler_known_symbols()
                .unary_representation_type(values, RepresentationRole::Task, payload)
                .unwrap()
                .unwrap()
        };

        for completion in [payload, task_type(payload)] {
            let task = task_type(completion);

            let generated = generated_lifecycle(
                &compilation,
                &target,
                MirHelperReference::Abandon {
                    action: bray_ir::MirAbandonmentAction::Quiesce,
                    ty: task,
                },
                84,
            );

            assert!(generated.frame_descriptor().is_some());

            assert_eq!(
                generated
                    .operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::BorrowTaskCompletion { .. }
                        )
                    ))
                    .count(),
                1
            );

            assert_eq!(
                generated
                    .operations()
                    .iter()
                    .filter(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::ReleaseTaskCompletionBorrow { .. }
                        )
                    ))
                    .count(),
                1
            );

            assert!(!generated.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(
                    bray_ir::MirAsyncOperation::ResolveTask { .. }
                        | bray_ir::MirAsyncOperation::DestroyTerminalTask { .. }
                )
            )));

            assert_eq!(
                generated
                    .blocks()
                    .iter()
                    .filter(|block| matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::Suspend {
                            kind: bray_ir::MirSuspensionKind::TaskCompletion,
                            cancellation: None,
                            ..
                        }
                    ))
                    .count(),
                1
            );

            let release = generated
                .blocks()
                .iter()
                .find(|block| {
                    block.operations().iter().any(|id| {
                        matches!(
                            generated.operation(*id).unwrap().kind(),
                            MirOperationKind::Async(
                                bray_ir::MirAsyncOperation::ReleaseTaskCompletionBorrow { .. }
                            )
                        )
                    })
                })
                .unwrap();

            assert!(matches!(
                release.terminator().kind(),
                MirTerminatorKind::Goto(_)
            ));

            assert_eq!(
                generated
                    .blocks()
                    .iter()
                    .filter(|block| matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::Suspend {
                            kind: bray_ir::MirSuspensionKind::Awaited,
                            cancellation: None,
                            ..
                        }
                    ))
                    .count(),
                usize::from(completion != payload)
            );
        }
    }

    #[test]
    fn future_abandonment_borrows_for_quiescence_and_consumes_for_synchronous_destruction() {
        let compilation = compilation("module app; func main() {}");
        let target = codegen_target(&compilation);
        let values = compilation.semantic_value_store().unwrap();
        let payload = values.intern_type(TypeData::tuple([])).unwrap();

        let future = compilation
            .available_compiler_known_symbols()
            .unary_representation_type(values, RepresentationRole::Future, payload)
            .unwrap()
            .unwrap();

        let quiescence = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Abandon {
                action: bray_ir::MirAbandonmentAction::Quiesce,
                ty: future,
            },
            85,
        );

        assert!(
            quiescence
                .frame_descriptor()
                .unwrap()
                .capture_abandonment()
                .is_some()
        );

        assert!(quiescence.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                frame: bray_ir::MirOperand::Copy(_),
                entry: bray_ir::MirFrameEntry::CaptureQuiescence,
                ..
            })
        )));

        assert!(!quiescence.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(
                bray_ir::MirAsyncOperation::DestroyInactiveCaptures { .. }
                    | bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                        entry: bray_ir::MirFrameEntry::Body
                            | bray_ir::MirFrameEntry::CaptureCleanup,
                        ..
                    }
            )
        )));

        let destruction = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Abandon {
                action: bray_ir::MirAbandonmentAction::Destroy,
                ty: future,
            },
            86,
        );

        assert!(destruction.frame_descriptor().is_none());

        assert!(destruction.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::DestroyInactiveCaptures {
                frame: bray_ir::MirOperand::Move(_),
                ..
            })
        )));

        assert!(
            destruction.blocks().iter().all(|block| !matches!(
                block.terminator().kind(),
                MirTerminatorKind::Suspend { .. }
            ))
        );
    }

    #[test]
    fn owned_buffer_cleanup_covers_synchronous_and_asynchronous_elements() {
        use bray_checker::CheckerRequestContext;
        use bray_ir::MirAbandonmentAction;
        use bray_symbols::{GenericArgument, GenericOwnerId, GenericSubstitutionData};

        for (authorized, asynchronous) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            let request = crate::CompilationRequest::new(
                bray_symbols::PackageIdentity::try_new("std").unwrap(),
                vec![crate::test_support::source_input(
                    r#"
                    module std.memory;

                    struct RawBuffer<T>
                    {
                        pointer: RawPointer<T>;
                        capacity: usize;
                        initialized: usize;
                    }
                    "#,
                    0,
                )],
            );

            let request = if authorized {
                request.with_standard_library_source_authority()
            } else {
                request
            };

            let compilation = Compilation::load(request).unwrap();
            let symbols = compilation.symbol_graph().unwrap();

            let buffer = symbols
                .structures()
                .iter()
                .find(|symbol| symbol.origin() == SymbolOrigin::Source)
                .unwrap();

            let definition = NamedTypeSymbolId::Struct(buffer.id());
            let values = compilation.semantic_value_store().unwrap();

            let unit = compilation
                .compiler_known_type(RepresentationRole::Unit)
                .unwrap();

            let element = if asynchronous {
                compilation
                    .available_compiler_known_symbols()
                    .unary_representation_type(values, RepresentationRole::Task, unit)
                    .unwrap()
                    .unwrap()
            } else {
                compilation
                    .compiler_known_type(RepresentationRole::ScalarU8)
                    .unwrap()
            };

            let substitution = values
                .intern_generic_substitution(
                    GenericSubstitutionData::try_new(
                        GenericOwnerId::try_new(buffer.id().into()).unwrap(),
                        buffer
                            .generic_type_parameters()
                            .iter()
                            .copied()
                            .map(Into::into),
                        [GenericArgument::Type(element)],
                    )
                    .unwrap(),
                )
                .unwrap();

            let ty = values
                .intern_type(TypeData::Named {
                    definition,
                    substitution,
                })
                .unwrap();

            let cancellation = CancellationToken::new();

            let context = crate::compilation::checker::CompilationCheckerContext::new(
                compilation.binding_context(&cancellation).unwrap(),
            );

            assert_eq!(
                context
                    .raw_buffer_element(definition, substitution)
                    .unwrap(),
                authorized.then_some(element)
            );

            let checked = bray_checker::cleanup_type_execution(&context, ty).unwrap();

            assert!(
                !checked.diagnostics().has_errors(),
                "{:?}",
                checked.diagnostics()
            );

            assert_eq!(
                checked.value().quiescence_execution(),
                Some(if authorized && asynchronous {
                    bray_symbols::CallableExecution::Asynchronous
                } else {
                    bray_symbols::CallableExecution::Synchronous
                })
            );

            if !authorized {
                continue;
            }

            let target = codegen_target(&compilation);

            for ty in [
                ty,
                values.intern_type(TypeData::Generator(element)).unwrap(),
            ] {
                let quiescence = generated_lifecycle(
                    &compilation,
                    &target,
                    MirHelperReference::Abandon {
                        action: MirAbandonmentAction::Quiesce,
                        ty,
                    },
                    87,
                );

                let destruction = generated_lifecycle(
                    &compilation,
                    &target,
                    MirHelperReference::Abandon {
                        action: MirAbandonmentAction::Destroy,
                        ty,
                    },
                    88,
                );

                let ordinary =
                    generated_lifecycle(&compilation, &target, MirHelperReference::Destroy(ty), 89);

                assert_eq!(quiescence.frame_descriptor().is_some(), asynchronous);
                assert_eq!(ordinary.frame_descriptor().is_some(), asynchronous);
                assert!(destruction.frame_descriptor().is_none());

                assert!(!quiescence.operations().iter().any(|operation| match operation.kind() {
                MirOperationKind::Destroy(_) | MirOperationKind::Finalize(_)
                | MirOperationKind::Abandon { action: MirAbandonmentAction::Destroy | MirAbandonmentAction::Destructor, .. } => true,
                MirOperationKind::Memory(memory) => matches!(memory.kind(),
                    bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate | bray_bound_tree::CheckedMemoryOperationKind::RawBufferSetInitializedCount),
                _ => false,
            }));

                assert!(!destruction.blocks().iter().any(|block| matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::Suspend { .. }
                )));

                assert!(
                    !destruction
                        .operations()
                        .iter()
                        .any(|operation| matches!(operation.kind(), MirOperationKind::Finalize(_)))
                );

                let element_cleanup = destruction.blocks().iter().find(|block| block.operations().iter().any(|id| matches!(
                destruction.operation(*id).unwrap().kind(), MirOperationKind::Abandon { action: MirAbandonmentAction::Destroy, place } if place.ty() == element,
            ))).unwrap();

                let operations = element_cleanup
                    .operations()
                    .iter()
                    .map(|id| destruction.operation(*id).unwrap().kind())
                    .collect::<Vec<_>>();

                let count = operations.iter().position(|operation| matches!(operation, MirOperationKind::Memory(memory)
                if memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawBufferSetInitializedCount)).unwrap();

                let destroy = operations
                    .iter()
                    .position(|operation| matches!(operation, MirOperationKind::Abandon { .. }))
                    .unwrap();

                assert!(count < destroy);

                for mir in [&destruction, &ordinary] {
                    let release = mir
                        .blocks()
                        .iter()
                        .find(|block| {
                            block.operations().iter().any(|id| matches!(
                    mir.operation(*id).unwrap().kind(), MirOperationKind::Memory(memory)
                    if memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate,
                ))
                        })
                        .unwrap();

                    assert!(matches!(
                        release.terminator().kind(),
                        MirTerminatorKind::CheckCallOutcome { .. }
                    ));
                }
            }
        }
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
                .filter(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_))))
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

        let policy_calls = generated
            .blocks()
            .iter()
            .filter(|block| {
                block.operations().iter().any(|id| {
                    matches!(
                        generated.operation(*id).map(|operation| operation.kind()),
                        Some(MirOperationKind::Call(call)) if matches!(call.target(), MirCallTarget::Direct(_))
                    )
                })
            })
            .collect::<Vec<_>>();

        for block in &policy_calls {
            assert!(
                matches!(
                    block.terminator().kind(),
                    MirTerminatorKind::CheckCallOutcome { .. }
                ),
                "every storage policy call must retain its outcome: {block:?}",
            );
        }

        let MirTerminatorKind::CheckCallOutcome { completed, .. } =
            policy_calls[0].terminator().kind()
        else {
            panic!("storage projection must check the call before using its pointer");
        };

        assert_eq!(
            completed.arguments().len(),
            1,
            "only successful projection transfers its pointer",
        );

        let projected = generated.block(completed.target()).unwrap();

        assert_eq!(projected.parameters().len(), 1);

        assert!(projected.operations().iter().any(|id| matches!(
            generated.operation(*id).map(|operation| operation.kind()),
            Some(MirOperationKind::Finalize(_))
        )));
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
            | bray_ir::MirUnitKey::CompilerProvidedCallable(_)
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
    fn sized_boxes_preserve_the_storage_policy_representation() {
        let compilation = compilation("module app;");
        let target = baseline_codegen_target();
        let values = compilation.semantic_value_store().unwrap();

        let scalar = compilation
            .compiler_known_type(RepresentationRole::ScalarI64)
            .unwrap();

        let policy = intern_type(values, TypeData::tuple([scalar, scalar, scalar]));

        let owned = intern_type(
            values,
            TypeData::OwnedIndirection {
                storage: policy,
                target: scalar,
            },
        );

        let mappings = realized_types(&compilation, &target, [owned]);

        assert_eq!(
            mappings[&owned],
            mappings[&policy].representation_for(owned)
        );

        assert_eq!(
            mappings[&owned].layout().map(TargetValueLayout::size),
            Some(24)
        );
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
