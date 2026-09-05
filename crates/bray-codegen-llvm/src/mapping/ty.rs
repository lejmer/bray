use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use bray_codegen::{
    CodegenCallableSignature, CodegenFailure, CodegenInstanceKey, CodegenMappings,
    CodegenParameterMapping, CodegenResultMapping, CodegenTarget, CodegenTypeKind,
    CodegenTypeMapping, TargetScalarKind,
};
use bray_symbols::TypeId;
use inkwell::context::Context;
use inkwell::targets::TargetData;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};

/// Task-local LLVM type realization of immutable code generation mappings.
pub(crate) struct LlvmTypeMappings<'context, 'mappings> {
    context: &'context Context,
    mappings: &'mappings CodegenMappings,
    target: &'mappings CodegenTarget,
    target_data: &'mappings TargetData,
    instance: Option<&'mappings CodegenInstanceKey>,
    mapped: BTreeMap<TypeId, BasicTypeEnum<'context>>,
    active: BTreeSet<TypeId>,
    named_type_ordinal: u32,
}

impl<'context, 'mappings> LlvmTypeMappings<'context, 'mappings> {
    pub(crate) fn new(
        context: &'context Context,
        mappings: &'mappings CodegenMappings,
        target: &'mappings CodegenTarget,
        target_data: &'mappings TargetData,
    ) -> Self {
        Self {
            context,
            mappings,
            target,
            target_data,
            instance: None,
            mapped: BTreeMap::new(),
            active: BTreeSet::new(),
            named_type_ordinal: 0,
        }
    }

    pub(crate) const fn context(&self) -> &'context Context {
        self.context
    }

    pub(crate) const fn target_data(&self) -> &'mappings TargetData {
        self.target_data
    }

    pub(crate) const fn target(&self) -> &'mappings CodegenTarget {
        self.target
    }

    pub(crate) const fn select_instance(&mut self, instance: &'mappings CodegenInstanceKey) {
        self.instance = Some(instance);
    }

    pub(crate) fn default_pointer_type(&self) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        self.map_pointer(bray_codegen::TargetAddressSpaceKind::Default)
    }

    #[cfg(test)]
    pub(crate) fn map_all(&mut self) -> Result<(), CodegenFailure> {
        for mapping in self.mappings.types() {
            self.map(mapping.ty())?;
        }

        Ok(())
    }

    pub(crate) fn map(&mut self, ty: TypeId) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        if let Some(instance) = self.instance {
            let Some(mapping) = self.mappings.instance_ty(instance, ty) else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            if mapping.ty() != ty {
                return self.map(mapping.ty());
            }
        }

        if self.active.contains(&ty) {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        if let Some(mapped) = self.mapped.get(&ty) {
            return Ok(*mapped);
        }

        let Some(mapping) = self.mappings.ty(ty) else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if mapping.backend_type() != ty {
            let mapped = self.map(mapping.backend_type())?;

            self.mapped.insert(ty, mapped);

            return Ok(mapped);
        }

        self.active.insert(ty);

        let mapped = self.map_kind(mapping)?;

        self.active.remove(&ty);

        if let Some(layout) = mapping.layout()
            && !layout_matches(self.target_data, mapped, layout)
        {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        self.mapped.insert(ty, mapped);

        Ok(mapped)
    }

    pub(crate) fn function_type(
        &mut self,
        signature: &CodegenCallableSignature,
    ) -> Result<FunctionType<'context>, CodegenFailure> {
        let mut parameters = Vec::new();

        if let CodegenResultMapping::Indirect { pointer, .. } = signature.result() {
            parameters.push(self.map(*pointer)?.into());
        }

        for parameter in signature.parameters() {
            let ty = match parameter {
                CodegenParameterMapping::Ignore => continue,
                CodegenParameterMapping::Direct { ty, .. } => *ty,
                CodegenParameterMapping::Indirect { pointer, .. } => *pointer,
            };

            parameters.push(BasicMetadataTypeEnum::from(self.map(ty)?));
        }

        if signature.has_panic_report_context() {
            parameters.push(self.default_pointer_type()?.into());
        }

        match signature.result() {
            CodegenResultMapping::Void | CodegenResultMapping::Indirect { .. } => Ok(self
                .context
                .void_type()
                .fn_type(&parameters, signature.is_variadic())),
            CodegenResultMapping::Direct { ty, .. } => {
                Ok(self.map(*ty)?.fn_type(&parameters, signature.is_variadic()))
            }
        }
    }

    fn map_kind(
        &mut self,
        mapping: &CodegenTypeMapping,
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        match mapping.kind() {
            CodegenTypeKind::Unit => Ok(self.context.struct_type(&[], false).into()),
            CodegenTypeKind::Boolean => self.map_scalar(TargetScalarKind::Boolean),
            CodegenTypeKind::SignedInteger(width) | CodegenTypeKind::UnsignedInteger(width) => {
                self.map_scalar(TargetScalarKind::Integer(*width))
            }
            CodegenTypeKind::Float(width) => self.map_scalar(TargetScalarKind::Float(*width)),
            CodegenTypeKind::Pointer { address_space, .. } => self.map_pointer(*address_space),
            CodegenTypeKind::Callable(_) => {
                self.map_pointer(bray_codegen::TargetAddressSpaceKind::Function)
            }
            CodegenTypeKind::Opaque => Ok(self.context.struct_type(&[], false).into()),
            CodegenTypeKind::Aggregate(fields) => self.map_aggregate(mapping, fields),
            CodegenTypeKind::Array { element, length } => {
                let length = crate::conversion::target_value(*length, "array_length")?;

                Ok(self.map(*element)?.array_type(length).into())
            }
            CodegenTypeKind::UnsizedSlice { .. } => self.map_unsized_slice(),
            CodegenTypeKind::UnsizedTraitView => self.map_unsized_trait_view(),
            CodegenTypeKind::Union { .. } => self.map_union(mapping),
        }
    }

    fn map_unsized_slice(&self) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let pointer = self.map_pointer(bray_codegen::TargetAddressSpaceKind::Default)?;

        let length = self
            .context
            .ptr_sized_int_type(self.target_data, None)
            .into();

        Ok(self.context.struct_type(&[pointer, length], false).into())
    }

    fn map_unsized_trait_view(&self) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let pointer = self.map_pointer(bray_codegen::TargetAddressSpaceKind::Default)?;

        Ok(self.context.struct_type(&[pointer, pointer], false).into())
    }

    fn map_scalar(
        &self,
        kind: TargetScalarKind,
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        match kind {
            TargetScalarKind::Boolean => Ok(self.context.bool_type().into()),
            TargetScalarKind::Integer(width) => {
                let width = NonZeroU32::new(u32::from(width.get())).unwrap_or(NonZeroU32::MIN);

                self.context
                    .custom_width_int_type(width)
                    .map(Into::into)
                    .map_err(CodegenFailure::unsupported_target_report)
            }
            TargetScalarKind::Float(width) => match width.get() {
                16 => Ok(self.context.f16_type().into()),
                32 => Ok(self.context.f32_type().into()),
                64 => Ok(self.context.f64_type().into()),
                128 => Ok(self.context.f128_type().into()),
                _ => Err(CodegenFailure::UnsupportedTarget),
            },
        }
    }

    fn map_pointer(
        &self,
        kind: bray_codegen::TargetAddressSpaceKind,
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let number = self
            .target
            .data_layout()
            .address_space(kind)
            .ok_or(CodegenFailure::UnsupportedTarget)?;

        let address_space = crate::conversion::target_value(number, "address_space")?;

        Ok(self.context.ptr_type(address_space).into())
    }

    fn map_aggregate(
        &mut self,
        mapping: &CodegenTypeMapping,
        fields: &[bray_codegen::CodegenFieldLayout],
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let layout = mapping
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let name = self.next_type_name("bray.type")?;
        let structure = self.context.opaque_struct_type(&name);

        let mut current_offset = 0_u64;
        let mut elements = Vec::new();
        let mut field_elements = Vec::new();

        for field in fields {
            if field.offset_bytes() < current_offset {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            push_padding(
                self.context,
                &mut elements,
                field.offset_bytes() - current_offset,
            )?;

            let field_type = self.map(field.ty())?;

            let element = crate::conversion::target_value(elements.len(), "aggregate_field_count")?;

            elements.push(field_type);
            field_elements.push((element, field.offset_bytes()));

            let Some(field_mapping) = self.mappings.ty(field.ty()) else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            let field_layout = field_mapping
                .layout()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            current_offset = field
                .offset_bytes()
                .checked_add(field_layout.size())
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;
        }

        if current_offset > layout.size() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        push_padding(self.context, &mut elements, layout.size() - current_offset)?;

        push_alignment_carrier(self.context, &mut elements, mapping)?;
        structure.set_body(&elements, false);

        if field_elements.iter().any(|(element, expected)| {
            self.target_data.offset_of_element(&structure, *element) != Some(*expected)
        }) {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        Ok(structure.into())
    }

    fn map_union(
        &mut self,
        mapping: &CodegenTypeMapping,
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let layout = mapping
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let name = self.next_type_name("bray.union")?;
        let structure = self.context.opaque_struct_type(&name);

        let mut elements = Vec::new();

        push_padding(self.context, &mut elements, layout.size())?;

        push_alignment_carrier(self.context, &mut elements, mapping)?;
        structure.set_body(&elements, false);

        Ok(structure.into())
    }

    fn next_type_name(&mut self, prefix: &str) -> Result<String, CodegenFailure> {
        let ordinal = self.named_type_ordinal;

        self.named_type_ordinal = self
            .named_type_ordinal
            .checked_add(1)
            .ok_or(CodegenFailure::UnsupportedTarget)?;

        Ok(format!("{prefix}.{ordinal}"))
    }
}

fn push_padding<'context>(
    context: &'context Context,
    elements: &mut Vec<BasicTypeEnum<'context>>,
    size: u64,
) -> Result<(), CodegenFailure> {
    if size == 0 {
        return Ok(());
    }

    let size = crate::conversion::target_value(size, "padding_size")?;

    elements.push(context.i8_type().array_type(size).into());

    Ok(())
}

fn push_alignment_carrier<'context>(
    context: &'context Context,
    elements: &mut Vec<BasicTypeEnum<'context>>,
    mapping: &CodegenTypeMapping,
) -> Result<(), CodegenFailure> {
    let alignment = mapping
        .layout()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .alignment()
        .get();

    let lanes = crate::conversion::target_value(alignment, "alignment_carrier_lanes")?;

    // Wide integers have target-limited ABI alignment. A zero-length vector array supplies
    // the requested power-of-two alignment without adding payload bytes or instructions.
    let carrier = context.i8_type().vec_type(lanes);

    elements.push(carrier.array_type(0).into());

    Ok(())
}

fn layout_matches(
    target_data: &TargetData,
    ty: BasicTypeEnum<'_>,
    layout: bray_target::TargetValueLayout,
) -> bool {
    target_data.get_store_size(&ty) == layout.size()
        && u64::from(target_data.get_abi_alignment(&ty)) == layout.alignment().get()
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU64};

    use bray_codegen::test_support::codegen_request;
    use bray_codegen::{
        CodegenFieldLayout, CodegenMappings, CodegenResultMapping, CodegenTypeKind,
        CodegenTypeMapping, TargetAddressSpaceKind,
    };
    use bray_symbols::testing::intern_type;
    use bray_symbols::{SemanticValueStore, TypeData};
    use bray_target::{TargetLayoutContract, TargetValueLayout};
    use inkwell::context::Context;

    use super::LlvmTypeMappings;
    use crate::machine::LlvmTargetMachine;

    #[test]
    fn demanded_types_map_to_exact_llvm_representations() {
        let fixture = codegen_request();
        let request = fixture.request();
        let types = mapped_type_fixture();
        let mappings = combined_mappings(request, &types);

        let Ok(machine) = LlvmTargetMachine::create(request.target()) else {
            panic!("test target must construct an LLVM machine");
        };

        let target_data = machine.target_data();
        let context = Context::create();

        let mut llvm = LlvmTypeMappings::new(&context, &mappings, request.target(), &target_data);

        assert_eq!(llvm.map_all(), Ok(()));

        let representations: Vec<_> = types
            .mappings
            .iter()
            .map(|mapping| {
                llvm.map(mapping.ty())
                    .map(|ty| ty.print_to_string().to_string())
            })
            .collect();

        assert!(representations.iter().all(Result::is_ok));

        assert_eq!(
            llvm.map(types.unsized_slice)
                .map(|ty| ty.print_to_string().to_string()),
            Ok("{ ptr, i64 }".to_owned())
        );

        assert_eq!(
            llvm.map(types.unsized_trait_view)
                .map(|ty| ty.print_to_string().to_string()),
            Ok("{ ptr, ptr }".to_owned())
        );
    }

    #[test]
    fn identified_type_names_follow_stable_realization_order() {
        let fixture = codegen_request();
        let request = fixture.request();
        let types = mapped_type_fixture();
        let mappings = combined_mappings(request, &types);

        let Ok(machine) = LlvmTargetMachine::create(request.target()) else {
            panic!("test target must construct an LLVM machine");
        };

        let target_data = machine.target_data();
        let context = Context::create();
        let mut llvm = LlvmTypeMappings::new(&context, &mappings, request.target(), &target_data);

        let aggregate = llvm
            .map(types.aggregate)
            .unwrap_or_else(|error| panic!("aggregate type must map: {error:?}"));

        let union = llvm
            .map(types.union)
            .unwrap_or_else(|error| panic!("union type must map: {error:?}"));

        assert!(
            aggregate
                .print_to_string()
                .to_string()
                .starts_with("%bray.type.0 = type")
        );

        assert!(
            union
                .print_to_string()
                .to_string()
                .starts_with("%bray.union.1 = type")
        );
    }

    #[test]
    fn aggregate_mapping_rejects_implicit_field_offset_changes() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();
        let types = mapped_type_fixture();
        let invalid_aggregate = types.invalid_aggregate;
        let four = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);

        let invalid = CodegenTypeMapping::new(
            invalid_aggregate,
            layout(8, four),
            CodegenTypeKind::aggregate([
                CodegenFieldLayout::new(None, types.byte, 0),
                CodegenFieldLayout::new(None, types.scalar, 1),
            ]),
        );

        let all_types = mappings
            .types()
            .iter()
            .cloned()
            .chain(types.mappings)
            .chain([invalid]);

        let Ok(mappings) = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            all_types,
            mappings.instance_types().iter().cloned(),
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().cloned(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.terminators().iter().cloned(),
            mappings.debug_locations().iter().cloned(),
        ) else {
            panic!("invalid physical field offsets remain backend validation input");
        };

        let Ok(machine) = LlvmTargetMachine::create(request.target()) else {
            panic!("test target must construct an LLVM machine");
        };

        let target_data = machine.target_data();
        let context = Context::create();
        let mut llvm = LlvmTypeMappings::new(&context, &mappings, request.target(), &target_data);

        assert_eq!(
            llvm.map(invalid_aggregate),
            Err(bray_codegen::CodegenFailure::UnsupportedTarget)
        );
    }

    #[test]
    fn alignment_carriers_preserve_small_and_overaligned_aggregate_layouts() {
        let fixture = codegen_request();
        let request = fixture.request();
        let machine = LlvmTargetMachine::create(request.target()).unwrap();
        let target_data = machine.target_data();
        let context = Context::create();

        for alignment in [1, 2, 4, 8, 16, 32, 64, 128] {
            let mut types = mapped_type_fixture();
            let alignment = NonZeroU64::new(alignment).unwrap();

            let mapping = types
                .mappings
                .iter_mut()
                .find(|mapping| mapping.ty() == types.aggregate)
                .unwrap();

            *mapping = CodegenTypeMapping::new(
                types.aggregate,
                layout(alignment.get(), alignment),
                CodegenTypeKind::aggregate([CodegenFieldLayout::new(None, types.byte, 0)]),
            );

            let mappings = combined_mappings(request, &types);

            let mut llvm =
                LlvmTypeMappings::new(&context, &mappings, request.target(), &target_data);

            let mapped = llvm
                .map(types.aggregate)
                .unwrap_or_else(|error| panic!("alignment {alignment}: {error:?}"));

            assert_eq!(target_data.get_store_size(&mapped), alignment.get());

            assert_eq!(
                u64::from(target_data.get_abi_alignment(&mapped)),
                alignment.get()
            );
        }
    }

    struct MappedTypeFixture {
        mappings: Vec<CodegenTypeMapping>,
        byte: bray_symbols::TypeId,
        scalar: bray_symbols::TypeId,
        aggregate: bray_symbols::TypeId,
        union: bray_symbols::TypeId,
        invalid_aggregate: bray_symbols::TypeId,
        unsized_slice: bray_symbols::TypeId,
        unsized_trait_view: bray_symbols::TypeId,
    }

    fn mapped_type_fixture() -> MappedTypeFixture {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("test semantic value store must be available");
        };

        let byte = intern_type(&store, TypeData::Error);
        let scalar = intern_type(&store, TypeData::tuple([byte]));
        let pointer = intern_type(&store, TypeData::Slice(scalar));
        let array = intern_type(&store, TypeData::tuple([scalar]));
        let aggregate = intern_type(&store, TypeData::tuple([scalar, scalar]));
        let callable = intern_type(&store, TypeData::Generator(scalar));
        let union = intern_type(&store, TypeData::Nullable(scalar));
        let invalid_aggregate = intern_type(&store, TypeData::tuple([byte, scalar]));
        let unsized_slice = intern_type(&store, TypeData::Slice(byte));
        let unsized_trait_view = intern_type(&store, TypeData::tuple([byte, byte, byte]));

        let one = NonZeroU64::MIN;
        let four = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);
        let eight = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let byte_width = NonZeroU16::new(8).unwrap_or(NonZeroU16::MIN);
        let integer_width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);

        let mappings = vec![
            CodegenTypeMapping::new(
                byte,
                layout(1, one),
                CodegenTypeKind::UnsignedInteger(byte_width),
            ),
            CodegenTypeMapping::new(
                scalar,
                layout(4, four),
                CodegenTypeKind::SignedInteger(integer_width),
            ),
            CodegenTypeMapping::new(
                pointer,
                layout(8, eight),
                CodegenTypeKind::Pointer {
                    target: scalar,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                array,
                layout(8, four),
                CodegenTypeKind::Array {
                    element: scalar,
                    length: 2,
                },
            ),
            CodegenTypeMapping::new(
                aggregate,
                layout(8, four),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, scalar, 0),
                    CodegenFieldLayout::new(None, scalar, 4),
                ]),
            ),
            CodegenTypeMapping::new(
                callable,
                layout(8, eight),
                CodegenTypeKind::callable(
                    [],
                    CodegenResultMapping::direct(scalar, None, []),
                    bray_symbols::CallableAbi::Bray,
                    false,
                ),
            ),
            CodegenTypeMapping::new(union, layout(8, eight), CodegenTypeKind::union(scalar, [])),
            CodegenTypeMapping::new_unsized(
                unsized_slice,
                CodegenTypeKind::UnsizedSlice { element: byte },
            ),
            CodegenTypeMapping::new_unsized(unsized_trait_view, CodegenTypeKind::UnsizedTraitView),
            CodegenTypeMapping::new(
                intern_type(&store, TypeData::tuple([])),
                layout(0, one),
                CodegenTypeKind::Unit,
            ),
        ];

        MappedTypeFixture {
            mappings,
            byte,
            scalar,
            aggregate,
            union,
            invalid_aggregate,
            unsized_slice,
            unsized_trait_view,
        }
    }

    fn combined_mappings(
        request: bray_codegen::CodegenRequest<'_>,
        types: &MappedTypeFixture,
    ) -> CodegenMappings {
        let mappings = request.mappings();

        let all_types = mappings
            .types()
            .iter()
            .cloned()
            .chain(types.mappings.iter().cloned());

        CodegenMappings::try_new(
            request.unit(),
            request.target(),
            all_types,
            mappings.instance_types().iter().cloned(),
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().cloned(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.terminators().iter().cloned(),
            mappings.debug_locations().iter().cloned(),
        )
        .unwrap_or_else(|error| panic!("representative type mappings must validate: {error:?}"))
    }

    fn layout(size: u64, alignment: NonZeroU64) -> TargetValueLayout {
        TargetValueLayout::new(size, alignment, TargetLayoutContract::Default)
    }
}
