use super::core::UnitTranslator;
use super::support::{
    aggregate_value_element, insert_value, integer_constant, llvm, real_width, real_words,
};
use bray_codegen::{CodegenFailure, CodegenTypeBehavior, CodegenTypeKind};
use bray_ir::{MirImmediateValue, MirOperand};
use bray_symbols::{ConstantValueId, ConstantValueKind, RealConstantBits};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn operand(
        &mut self,
        operand: &MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match operand {
            MirOperand::Value(value) => self
                .values
                .get(value)
                .copied()
                .ok_or(CodegenFailure::GeneratedModuleInvariant),
            MirOperand::Immediate { value, ty } => {
                let ty = self.types.map(*ty)?;

                match value {
                    MirImmediateValue::Boolean(value) => match ty {
                        BasicTypeEnum::IntType(ty) => {
                            Ok(ty.const_int(u64::from(*value), false).into())
                        }
                        _ => Err(CodegenFailure::GeneratedModuleInvariant),
                    },
                    MirImmediateValue::Unit | MirImmediateValue::NullableAbsent => {
                        Ok(ty.const_zero())
                    }
                }
            }
            MirOperand::Copy(place) => {
                let pointer = self.place(place)?;

                let value = llvm(self.builder.build_load(
                    self.types.map(place.ty())?,
                    pointer,
                    "load",
                ))?;

                if matches!(operand, MirOperand::Copy(_)) {
                    self.retain_copied_value(value, place.ty())?;
                }

                Ok(value)
            }
            MirOperand::Move(place) => {
                let pointer = self.place(place)?;

                if !self.pending_moves.contains(place) {
                    self.pending_moves.push(place.clone());
                }

                llvm(
                    self.builder
                        .build_load(self.types.map(place.ty())?, pointer, "move"),
                )
            }
            MirOperand::Constant { value, .. } => self.constant(*value),
        }
    }

    pub(super) fn clear_moved_places(&mut self) -> Result<(), CodegenFailure> {
        let moved = std::mem::take(&mut self.pending_moves);

        for place in moved {
            let ownership_bearing = self
                .request
                .mappings()
                .ty(place.ty())
                .is_some_and(|mapping| {
                    matches!(
                        mapping.kind(),
                        CodegenTypeKind::Aggregate(_)
                            | CodegenTypeKind::Array { .. }
                            | CodegenTypeKind::Union { .. }
                    )
                });

            if !ownership_bearing {
                continue;
            }

            let pointer = self.place(&place)?;
            let value_type = self.types.map(place.ty())?;

            llvm(self.builder.build_store(pointer, value_type.const_zero()))?;
        }

        Ok(())
    }

    fn retain_copied_value(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if mapping.behavior() == Some(CodegenTypeBehavior::String) {
            return self.retain_string(value, ty);
        }

        // Recursive translation changes the insertion block, so retain an owned representation.
        let kind = mapping.kind().clone();

        match kind {
            CodegenTypeKind::Aggregate(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    let element = aggregate_value_element(self.request.mappings(), &fields, index)?;

                    let element =
                        u32::try_from(element).map_err(|_| CodegenFailure::ResourceExhausted)?;

                    let field_value = super::support::extract_value(&self.builder, value, element)?;

                    self.retain_copied_value(field_value, field.ty())?;
                }
            }
            CodegenTypeKind::Array { element, length } => {
                for index in 0..length {
                    let index =
                        u32::try_from(index).map_err(|_| CodegenFailure::ResourceExhausted)?;

                    let element_value = super::support::extract_value(&self.builder, value, index)?;

                    self.retain_copied_value(element_value, element)?;
                }
            }
            CodegenTypeKind::Union { tag, variants } => {
                self.retain_copied_union(value, tag, &variants)?;
            }
            CodegenTypeKind::Unit
            | CodegenTypeKind::Boolean
            | CodegenTypeKind::SignedInteger(_)
            | CodegenTypeKind::UnsignedInteger(_)
            | CodegenTypeKind::Float(_)
            | CodegenTypeKind::Pointer { .. }
            | CodegenTypeKind::UnsizedSlice { .. }
            | CodegenTypeKind::UnsizedTraitView
            | CodegenTypeKind::Callable(_) => {}
        }

        Ok(())
    }

    fn retain_copied_union(
        &mut self,
        value: BasicValueEnum<'context>,
        tag: bray_symbols::TypeId,
        variants: &[bray_codegen::CodegenUnionVariantLayout],
    ) -> Result<(), CodegenFailure> {
        let storage = llvm(self.builder.build_alloca(value.get_type(), "copy.union"))?;

        llvm(self.builder.build_store(storage, value))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag =
            llvm(self.builder.build_load(tag_type, storage, "copy.union.tag"))?.into_int_value();

        let done = self
            .types
            .context()
            .append_basic_block(self.function, "copy.union.retained");

        let cases = variants
            .iter()
            .enumerate()
            .map(|(index, variant)| {
                (
                    integer_constant(tag_type, variant.tag()),
                    self.types
                        .context()
                        .append_basic_block(self.function, &format!("copy.union.variant.{index}")),
                )
            })
            .collect::<Vec<_>>();

        llvm(self.builder.build_switch(tag, done, &cases))?;

        for (variant, (_, block)) in variants.iter().zip(&cases) {
            self.builder.position_at_end(*block);

            for field in variant.fields() {
                let field_pointer = self.constant_offset_pointer(storage, field.offset_bytes())?;

                let field_value = llvm(self.builder.build_load(
                    self.types.map(field.ty())?,
                    field_pointer,
                    "copy.union.field",
                ))?;

                self.retain_copied_value(field_value, field.ty())?;
            }

            llvm(self.builder.build_unconditional_branch(done))?;
        }

        self.builder.position_at_end(done);

        Ok(())
    }

    pub(super) fn constant(
        &mut self,
        value: ConstantValueId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .constant(value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = mapping.data().ty();

        // Child constants are translated recursively after releasing the parent mapping borrow.
        let kind = mapping.data().kind().clone();

        match kind {
            ConstantValueKind::Error => Err(CodegenFailure::GeneratedModuleInvariant),
            ConstantValueKind::Boolean(value) => {
                let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                Ok(ty.const_int(u64::from(value), false).into())
            }
            ConstantValueKind::Character(value) => {
                let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                Ok(ty.const_int(u64::from(u32::from(value)), false).into())
            }
            ConstantValueKind::Integer(value) => self.integer_constant(ty, &value),
            ConstantValueKind::Real(bits) => self.real_constant(ty, bits),
            ConstantValueKind::Complex { real, imaginary } => {
                let mut value = self.types.map(ty)?.const_zero();
                let real = self.real_constant_component(real)?;
                let imaginary = self.real_constant_component(imaginary)?;

                value = insert_value(&self.builder, value, real, 0)?;

                insert_value(&self.builder, value, imaginary, 1)
            }
            ConstantValueKind::String(text) => self.string_constant(value, ty, &text),
            ConstantValueKind::Unit | ConstantValueKind::NullableAbsent => {
                Ok(self.types.map(ty)?.const_zero())
            }
            ConstantValueKind::NullablePresent(child) => {
                let child = self.constant(child)?;

                self.construct_nullable_present(ty, child)
            }
            ConstantValueKind::Tuple(children) | ConstantValueKind::Array(children) => {
                self.aggregate_constant(ty, children.iter().copied())
            }
            ConstantValueKind::Product(fields) => {
                let mapping = self
                    .request
                    .mappings()
                    .ty(ty)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let CodegenTypeKind::Aggregate(layout) = mapping.kind() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let mut value = self.types.map(ty)?.const_zero();

                for field in fields.iter() {
                    let field_value = self.constant(*field.value())?;

                    let index = layout
                        .iter()
                        .position(|layout| {
                            layout.reference()
                                == Some(bray_ir::MirFieldReference::Struct(*field.field()))
                        })
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    value = insert_value(
                        &self.builder,
                        value,
                        field_value,
                        aggregate_value_element(self.request.mappings(), layout, index)?,
                    )?;
                }

                Ok(value)
            }
            ConstantValueKind::Union { variant, fields } => {
                self.union_constant(ty, variant, &fields)
            }
        }
    }

    pub(super) fn integer_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        value: &bray_symbols::IntegerConstant,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(integer_constant(ty, value).into())
    }

    pub(super) fn real_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        bits: RealConstantBits,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let target = self.types.map(ty)?;

        self.real_bits(target, bits)
    }

    pub(super) fn real_constant_component(
        &mut self,
        bits: RealConstantBits,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let width = real_width(bits);

        let target = match width {
            16 => self.types.context().f16_type().into(),
            32 => self.types.context().f32_type().into(),
            64 => self.types.context().f64_type().into(),
            128 => self.types.context().f128_type().into(),
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        self.real_bits(target, bits)
    }

    pub(super) fn real_bits(
        &self,
        target: BasicTypeEnum<'context>,
        bits: RealConstantBits,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let BasicTypeEnum::FloatType(float) = target else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let words = real_words(bits);
        let width = float.get_bit_width();

        let integer_type = self
            .types
            .context()
            .custom_width_int_type(
                std::num::NonZeroU32::new(width).unwrap_or(std::num::NonZeroU32::MIN),
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let integer = integer_type.const_int_arbitrary_precision(&words);

        llvm(self.builder.build_bit_cast(integer, float, "constant.real"))
    }

    pub(super) fn aggregate_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        children: impl IntoIterator<Item = ConstantValueId>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self
            .request
            .mappings()
            .ty(ty)
            .and_then(|mapping| match mapping.kind() {
                // Constant materialization mutates the value cache after this lookup.
                CodegenTypeKind::Aggregate(fields) => Some(fields.clone()),
                CodegenTypeKind::Array { .. } => None,
                _ => None,
            });

        let mut value = self.types.map(ty)?.const_zero();

        for (index, child) in children.into_iter().enumerate() {
            let child = self.constant(child)?;

            let element = match &fields {
                Some(fields) => aggregate_value_element(self.request.mappings(), fields, index)?,
                None => index,
            };

            value = insert_value(&self.builder, value, child, element)?;
        }

        Ok(value)
    }

    pub(super) fn construct_nullable_present(
        &mut self,
        ty: bray_symbols::TypeId,
        child: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapped = self.types.map(ty)?;

        if child.get_type() == mapped {
            return Ok(child);
        }

        let mut value = mapped.const_zero();

        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let payload = fields
            .len()
            .checked_sub(1)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if fields.len() > 1 {
            let tag_type = self.types.map(fields[0].ty())?;

            let BasicTypeEnum::IntType(tag_type) = tag_type else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            value = insert_value(
                &self.builder,
                value,
                tag_type.const_int(1, false).into(),
                aggregate_value_element(self.request.mappings(), fields, 0)?,
            )?;
        }

        insert_value(
            &self.builder,
            value,
            child,
            aggregate_value_element(self.request.mappings(), fields, payload)?,
        )
    }

    pub(super) fn string_constant(
        &mut self,
        value: ConstantValueId,
        ty: bray_symbols::TypeId,
        text: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let bytes = self.types.context().const_string(text.as_bytes(), false);
        let name = format!("bray.constant.string.{}", value.slot());

        let global = self
            .module
            .get_global(&name)
            .unwrap_or_else(|| self.module.add_global(bytes.get_type(), None, &name));

        global.set_constant(true);
        global.set_initializer(&bytes);

        let pointer: BasicValueEnum<'context> = global.as_pointer_value().into();

        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match mapping.kind() {
            CodegenTypeKind::Pointer { .. } => Ok(pointer),
            CodegenTypeKind::Aggregate(fields)
                if fields.len() == 3
                    && mapping.behavior() == Some(bray_codegen::CodegenTypeBehavior::String) =>
            {
                let mut value = self.types.map(ty)?.const_zero();
                let pointer_index = 0;
                let length_index = 2;

                let BasicTypeEnum::IntType(length_type) =
                    self.types.map(fields[length_index].ty())?
                else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                value = insert_value(
                    &self.builder,
                    value,
                    pointer,
                    aggregate_value_element(self.request.mappings(), fields, pointer_index)?,
                )?;

                insert_value(
                    &self.builder,
                    value,
                    length_type
                        .const_int(
                            u64::try_from(text.len())
                                .map_err(|_| CodegenFailure::ResourceExhausted)?,
                            false,
                        )
                        .into(),
                    aggregate_value_element(self.request.mappings(), fields, length_index)?,
                )
            }
            CodegenTypeKind::Unit
            | CodegenTypeKind::Boolean
            | CodegenTypeKind::SignedInteger(_)
            | CodegenTypeKind::UnsignedInteger(_)
            | CodegenTypeKind::Float(_)
            | CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::UnsizedSlice { .. }
            | CodegenTypeKind::UnsizedTraitView
            | CodegenTypeKind::Union { .. }
            | CodegenTypeKind::Callable(_) => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn union_constant(
        &mut self,
        ty: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        fields: &[bray_symbols::ConstantField<
            bray_symbols::UnionPayloadFieldSymbolId,
            ConstantValueId,
        >],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let variant = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let llvm_type = self.types.map(ty)?;
        let storage = llvm(self.builder.build_alloca(llvm_type, "constant.union"))?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(*tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag = integer_constant(tag_type, variant.tag());

        llvm(self.builder.build_store(storage, tag))?;

        for field in fields {
            let layout = variant
                .fields()
                .iter()
                .find(|layout| {
                    layout.reference()
                        == Some(bray_ir::MirFieldReference::UnionPayload(*field.field()))
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let destination = self.constant_offset_pointer(storage, layout.offset_bytes())?;
            let value = self.constant(*field.value())?;

            llvm(self.builder.build_store(destination, value))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "constant.union.value"),
        )
    }

    pub(super) fn constant_offset_pointer(
        &self,
        pointer: PointerValue<'context>,
        offset: u64,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let integer_type = self
            .types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None);

        let address = llvm(
            self.builder
                .build_ptr_to_int(pointer, integer_type, "address"),
        )?;

        let address = llvm(self.builder.build_int_add(
            address,
            integer_type.const_int(offset, false),
            "address.offset",
        ))?;

        llvm(
            self.builder
                .build_int_to_ptr(address, pointer.get_type(), "address.pointer"),
        )
    }

    pub(super) fn operand_type(
        &self,
        operand: &MirOperand,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        match operand {
            MirOperand::Value(value) => self
                .unit
                .value(*value)
                .map(bray_ir::MirValue::ty)
                .ok_or(CodegenFailure::GeneratedModuleInvariant),
            MirOperand::Constant { ty, .. } | MirOperand::Immediate { ty, .. } => Ok(*ty),
            MirOperand::Copy(place) | MirOperand::Move(place) => Ok(place.ty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

    use bray_codegen::test_support::{codegen_request_for_unit, codegen_target};
    use bray_codegen::{
        CodeGenerator, CodegenCallableSignature, CodegenDebugLocation, CodegenFieldLayout,
        CodegenLinkage, CodegenMappings, CodegenResultMapping, CodegenSourceFile, CodegenSymbolKey,
        CodegenSymbolMapping, CodegenTypeBehavior, CodegenTypeKind, CodegenTypeMapping,
        CodegenUnionVariantLayout, CodegenUnit, TargetAddressSpaceKind,
    };
    use bray_ir::{
        MirBlockKind, MirImmediateValue, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor,
        MirStorageKind, MirStoreKind, MirTargetFacts, MirTerminatorKind, MirUnitBuilder,
        MirUnitKind,
    };
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiVersion};
    use bray_symbols::testing::intern_type;
    use bray_symbols::{
        CallableAbi, IntegerConstant, SemanticValueStore, SymbolId, TypeData, TypeId,
        UnionVariantSymbolId,
    };
    use bray_target::{TargetLayoutContract, TargetValueLayout};
    use inkwell::context::Context;

    use super::super::super::super::backend::LlvmCodeGenerator;

    #[derive(Clone, Copy)]
    struct CompositeTypes {
        byte: TypeId,
        pointer: TypeId,
        usize: TypeId,
        boolean: TypeId,
        tag: TypeId,
        string: TypeId,
        nullable: TypeId,
        tuple: TypeId,
        union: TypeId,
    }

    #[test]
    fn copied_composites_recursively_retain_only_active_string_payloads() {
        let backend = LlvmCodeGenerator::try_new()
            .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}"));

        let fixture = copied_composite_fixture(&backend);
        let context = Context::create();

        let (_, module) = backend
            .prepare_module(fixture.request(), &context)
            .unwrap_or_else(|error| panic!("composite copy must generate: {error:?}"))
            .unwrap_or_else(|| panic!("composite copy generation must not be cancelled"));

        let ir = module.print_to_string().to_string();

        assert_eq!(ir.matches("atomicrmw add").count(), 4, "{ir}");
        assert!(ir.contains("switch i8 %copy.union.tag"), "{ir}");
        assert!(ir.contains("copy.union.variant.0"), "{ir}");
        assert!(ir.contains("copy.union.variant.1"), "{ir}");
        assert!(module.verify().is_ok(), "{ir}");
    }

    fn copied_composite_fixture(
        backend: &LlvmCodeGenerator,
    ) -> bray_codegen::test_support::CodegenRequestFixture {
        let target = codegen_target();
        let types = composite_types();
        let bound = bray_testing::test_bound_unit(292);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            MirTargetFacts::new(target.profile().clone(), RuntimeAbiVersion::new(1, 0)),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("copy test block must build: {error:?}"));

        for ty in [types.nullable, types.tuple, types.union] {
            let source_storage = builder
                .push_storage(source.clone(), MirStorageKind::Local, ty)
                .unwrap_or_else(|error| panic!("copy source storage must build: {error:?}"));

            let destination_storage = builder
                .push_storage(source.clone(), MirStorageKind::Local, ty)
                .unwrap_or_else(|error| panic!("copy destination storage must build: {error:?}"));

            builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Initialize,
                        destination: MirPlace::new(source_storage, [], ty),
                        value: MirOperand::Immediate {
                            value: MirImmediateValue::NullableAbsent,
                            ty,
                        },
                    },
                    None,
                )
                .unwrap_or_else(|error| panic!("copy source must initialize: {error:?}"));

            builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Initialize,
                        destination: MirPlace::new(destination_storage, [], ty),
                        value: MirOperand::Copy(MirPlace::new(source_storage, [], ty)),
                    },
                    None,
                )
                .unwrap_or_else(|error| panic!("composite copy must build: {error:?}"));
        }

        builder
            .set_terminator(entry, source.clone(), MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("copy test return must build: {error:?}"));

        let mir = builder
            .finish(entry)
            .unwrap_or_else(|error| panic!("copy test MIR must validate: {error:?}"));

        let unit = CodegenUnit::try_new(1, [mir])
            .unwrap_or_else(|error| panic!("copy test codegen unit must validate: {error:?}"));

        let mappings = composite_mappings(&unit, &target, types, source);

        codegen_request_for_unit(unit, target, mappings, backend.identity().clone())
    }

    fn composite_types() -> CompositeTypes {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store must initialize: {error:?}"));

        let byte = intern_type(&store, TypeData::Error);
        let pointer = intern_type(&store, TypeData::tuple([byte]));
        let usize = intern_type(&store, TypeData::tuple([pointer]));
        let boolean = intern_type(&store, TypeData::tuple([usize]));
        let tag = intern_type(&store, TypeData::tuple([boolean]));
        let string = intern_type(&store, TypeData::tuple([tag]));
        let nullable = intern_type(&store, TypeData::Nullable(string));
        let tuple = intern_type(&store, TypeData::tuple([string, string]));
        let union = intern_type(&store, TypeData::tuple([nullable, tuple]));

        CompositeTypes {
            byte,
            pointer,
            usize,
            boolean,
            tag,
            string,
            nullable,
            tuple,
            union,
        }
    }

    fn composite_mappings(
        unit: &CodegenUnit,
        target: &bray_codegen::CodegenTarget,
        types: CompositeTypes,
        source: MirSourceAnchor,
    ) -> CodegenMappings {
        let align1 = NonZeroU64::MIN;
        let align8 = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let width8 = NonZeroU16::new(8).unwrap_or(NonZeroU16::MIN);
        let width64 = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);

        let layout = |size, alignment| {
            TargetValueLayout::new(size, alignment, TargetLayoutContract::Default)
        };

        let present = UnionVariantSymbolId::from_symbol_id(SymbolId::new(1));
        let absent = UnionVariantSymbolId::from_symbol_id(SymbolId::new(2));

        let type_mappings = [
            CodegenTypeMapping::new(
                types.byte,
                layout(1, align1),
                CodegenTypeKind::UnsignedInteger(width8),
            ),
            CodegenTypeMapping::new(
                types.pointer,
                layout(8, align8),
                CodegenTypeKind::Pointer {
                    target: types.byte,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                types.usize,
                layout(8, align8),
                CodegenTypeKind::UnsignedInteger(width64),
            ),
            CodegenTypeMapping::new(types.boolean, layout(1, align1), CodegenTypeKind::Boolean),
            CodegenTypeMapping::new(
                types.tag,
                layout(1, align1),
                CodegenTypeKind::UnsignedInteger(width8),
            ),
            CodegenTypeMapping::new(
                types.string,
                layout(24, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.pointer, 0),
                    CodegenFieldLayout::new(None, types.pointer, 8),
                    CodegenFieldLayout::new(None, types.usize, 16),
                ]),
            )
            .with_behavior(Some(CodegenTypeBehavior::String)),
            CodegenTypeMapping::new(
                types.nullable,
                layout(32, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.boolean, 0),
                    CodegenFieldLayout::new(None, types.string, 8),
                ]),
            ),
            CodegenTypeMapping::new(
                types.tuple,
                layout(48, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.string, 0),
                    CodegenFieldLayout::new(None, types.string, 24),
                ]),
            ),
            CodegenTypeMapping::new(
                types.union,
                layout(32, align8),
                CodegenTypeKind::union(
                    types.tag,
                    [
                        CodegenUnionVariantLayout::new(
                            present,
                            IntegerConstant::from_u64(0),
                            [CodegenFieldLayout::new(None, types.string, 8)],
                        ),
                        CodegenUnionVariantLayout::new(absent, IntegerConstant::from_u64(1), []),
                    ],
                ),
            ),
        ];

        let instance = unit
            .instances()
            .first()
            .unwrap_or_else(|| panic!("copy test unit must contain one instance"));

        let name = BinarySymbolName::try_new("bray_composite_copy_test")
            .unwrap_or_else(|| panic!("copy test symbol must be valid"));

        let symbol = CodegenSymbolMapping::new(
            CodegenSymbolKey::Instance(instance.key().clone()),
            name,
            CodegenLinkage::Internal,
            CodegenCallableSignature::new([], CodegenResultMapping::Void, CallableAbi::Bray, false),
        );

        let file = CodegenSourceFile::try_new("composite-copy.bray")
            .unwrap_or_else(|| panic!("copy test source file must be valid"));

        let debug = CodegenDebugLocation::new(source, file, NonZeroU32::MIN, NonZeroU32::MIN);

        CodegenMappings::try_new(
            unit,
            target,
            type_mappings,
            [symbol],
            [],
            [],
            [],
            [],
            [],
            [debug],
        )
        .unwrap_or_else(|error| panic!("copy test mappings must validate: {error:?}"))
    }
}
