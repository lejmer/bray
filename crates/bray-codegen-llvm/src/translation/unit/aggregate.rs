use bray_codegen::{CodegenFailure, CodegenFieldLayout, CodegenTypeKind};
use bray_ir::{PatternOperation, PatternProjection};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::core::UnitTranslator;
use super::support::{extract_value, insert_value, integer_constant, llvm, pointer_value};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn initialize_value(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
        name: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if !self.types.is_address_value(ty)? {
            return Ok(value);
        }

        let storage = self.allocate_temporary(value.get_type(), name)?;

        llvm(self.builder.build_store(storage, value))?;

        Ok(storage.into())
    }

    pub(super) fn place_value(
        &mut self,
        pointer: PointerValue<'context>,
        ty: bray_symbols::TypeId,
        name: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if self.types.is_address_value(ty)? {
            Ok(pointer.into())
        } else {
            llvm(self.builder.build_load(self.types.map(ty)?, pointer, name))
        }
    }

    pub(super) fn load_value(
        &mut self,
        pointer: PointerValue<'context>,
        ty: bray_symbols::TypeId,
        name: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let value = self.place_value(pointer, ty, name)?;

        self.snapshot_value(value, ty, name)
    }

    pub(super) fn abi_register_value(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if self.types.is_address_value(ty)? {
            let pointer = super::support::pointer_value(value)
                .expect("address-valued ABI input retains its storage");

            llvm(
                self.builder
                    .build_load(self.types.map(ty)?, pointer, "abi.register.value"),
            )
        } else {
            Ok(value)
        }
    }

    pub(super) fn snapshot_value(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
        name: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if !self.types.is_address_value(ty)? {
            return Ok(value);
        }

        let mapped = self.types.map(ty)?;
        let storage = self.allocate_temporary(mapped, name)?;

        self.store_value(storage, value, ty)?;

        Ok(storage.into())
    }

    pub(super) fn store_value(
        &mut self,
        destination: PointerValue<'context>,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        if self.types.is_address_value(ty)? {
            let source = super::support::pointer_value(value)
                .expect("address-valued aggregate has its storage carrier");

            if source == destination {
                return Ok(());
            }

            let storage = self.types.map(ty)?;
            let size = self.types.target_data().get_store_size(&storage);

            llvm(self.builder.build_memmove(
                destination,
                1,
                source,
                1,
                self.pointer_integer_type().const_int(size, false),
            ))?;
        } else {
            llvm(self.builder.build_store(destination, value))?;
        }

        Ok(())
    }

    pub(super) fn construct_positional_product(
        &mut self,
        ty: bray_symbols::TypeId,
        values: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self.aggregate_fields(ty);

        if fields.len() != values.len() {
            panic!("checked MIR memory translation violated an established compiler contract");
        }

        let mapped = self.types.map(ty)?;
        let mut result = self.initialize_value(mapped.const_zero(), ty, "aggregate.product")?;

        for (index, value) in values.iter().copied().enumerate() {
            result = self.insert_aggregate_field(result, &fields, index, value)?;
        }

        Ok(result)
    }

    pub(super) fn insert_aggregate_field(
        &mut self,
        value: BasicValueEnum<'context>,
        fields: &[CodegenFieldLayout],
        index: usize,
        child: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let field = fields
            .get(index)
            .expect("aggregate field ordinal is checked");

        if let BasicValueEnum::PointerValue(storage) = value {
            let destination = self.representation_offset_pointer(storage, field.offset_bytes())?;

            self.store_value(destination, child, field.ty())?;

            Ok(value)
        } else {
            let child = self.abi_register_value(child, field.ty())?;

            insert_value(
                &self.builder,
                value,
                child,
                self.aggregate_value_element(fields, index)?,
            )
        }
    }

    pub(super) fn insert_array_element(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
        index: usize,
        child: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self.type_mapping(ty).expect("array value has its mapping");

        let CodegenTypeKind::Array { element, length } = mapping.kind() else {
            panic!("array value has its array representation");
        };

        let element = *element;
        let ordinal = u64::try_from(index).expect("array index fits u64");

        assert!(ordinal < *length, "array element ordinal is checked");

        if self.types.is_address_value(ty)? {
            let storage = pointer_value(value).expect("array value retains its storage");

            let offset = self
                .mapped_type_size(element)
                .checked_mul(ordinal)
                .expect("array element offset fits its checked layout");

            let destination = self.representation_offset_pointer(storage, offset)?;

            self.store_value(destination, child, element)?;

            Ok(value)
        } else {
            insert_value(&self.builder, value, child, index)
        }
    }

    pub(super) fn equal_pattern_values(
        &mut self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        if !self.types.is_address_value(ty)? {
            return self.equal_values(left, right);
        }

        // Recursive comparison emits child operations after releasing the immutable mapping.
        let kind = self
            .type_mapping(ty)
            .expect("pattern value has its mapping")
            .kind()
            .clone();

        match kind {
            CodegenTypeKind::Aggregate(fields) => self.equal_layout_fields(left, right, &fields),
            CodegenTypeKind::Array { element, length } => {
                let mut equal = self.types.context().bool_type().const_int(1, false);

                for index in 0..length {
                    let left = self.array_element_value(left, ty, index)?;
                    let right = self.array_element_value(right, ty, index)?;
                    let child = self.equal_pattern_values(left, right, element)?;

                    equal = llvm(self.builder.build_and(equal, child, "pattern.equal.all"))?;
                }

                Ok(equal)
            }
            CodegenTypeKind::Union {
                tag: None,
                variants,
            } => {
                let [variant] = variants.as_ref() else {
                    panic!("untagged union has exactly one represented variant");
                };

                self.equal_layout_fields(left, right, variant.fields())
            }
            CodegenTypeKind::Union {
                tag: Some(_),
                variants,
            } => self.equal_union_values(left, right, ty, &variants),
            unexpected => {
                panic!("address-valued pattern has an aggregate representation: {unexpected:?}")
            }
        }
    }

    fn equal_layout_fields(
        &mut self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
        fields: &[CodegenFieldLayout],
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let mut equal = self.types.context().bool_type().const_int(1, false);

        for (index, field) in fields.iter().enumerate() {
            let left = self.layout_field_value(left, fields, index)?;
            let right = self.layout_field_value(right, fields, index)?;
            let child = self.equal_pattern_values(left, right, field.ty())?;

            equal = llvm(self.builder.build_and(equal, child, "pattern.equal.all"))?;
        }

        Ok(equal)
    }

    fn equal_union_values(
        &mut self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
        ty: bray_symbols::TypeId,
        variants: &[bray_codegen::CodegenUnionVariantLayout],
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let left_tag = self.union_tag(left, ty)?;
        let right_tag = self.union_tag(right, ty)?;

        let same = llvm(self.builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            left_tag,
            right_tag,
            "pattern.equal.tag",
        ))?;

        let source = self
            .builder
            .get_insert_block()
            .expect("comparison has its source block");

        let context = self.types.context();
        let inspect = context.append_basic_block(self.function, "pattern.equal.payload");
        let done = context.append_basic_block(self.function, "pattern.equal.done");
        let no = context.bool_type().const_zero();

        llvm(self.builder.build_conditional_branch(same, inspect, done))?;
        self.builder.position_at_end(inspect);

        let cases = variants
            .iter()
            .map(|variant| {
                let tag = variant
                    .tag()
                    .expect("tagged union variant has its discriminant");

                let block = context.append_basic_block(self.function, "pattern.equal.variant");

                (integer_constant(left_tag.get_type(), tag), block)
            })
            .collect::<Vec<_>>();

        llvm(self.builder.build_switch(left_tag, done, &cases))?;

        let mut incoming = vec![(no, source), (no, inspect)];

        for (variant, (_, block)) in variants.iter().zip(cases) {
            self.builder.position_at_end(block);

            // Only the common active variant may read its initialized payload.
            let equal = self.equal_layout_fields(left, right, variant.fields())?;

            let end = self
                .builder
                .get_insert_block()
                .expect("payload comparison has its continuation");

            llvm(self.builder.build_unconditional_branch(done))?;
            incoming.push((equal, end));
        }

        self.builder.position_at_end(done);

        let result = llvm(
            self.builder
                .build_phi(context.bool_type(), "pattern.equal.union"),
        )?;

        for (value, block) in incoming {
            result.add_incoming(&[(&value, block)]);
        }

        Ok(result.as_basic_value().into_int_value())
    }

    pub(super) fn store_union_tag(
        &mut self,
        storage: PointerValue<'context>,
        tag: Option<bray_symbols::TypeId>,
        value: Option<&bray_symbols::IntegerConstant>,
    ) -> Result<(), CodegenFailure> {
        let (Some(tag), Some(value)) = (tag, value) else {
            return Ok(());
        };

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            panic!("checked MIR translation violated an established compiler contract");
        };

        llvm(
            self.builder
                .build_store(storage, integer_constant(tag_type, value)),
        )?;

        Ok(())
    }

    pub(super) fn translate_pattern_projection(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        result_type: bray_symbols::TypeId,
        projection: PatternProjection,
        operation: PatternOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let projected_type = if matches!(
            operation,
            PatternOperation::SharedBorrow | PatternOperation::MutableBorrow
        ) {
            let mapping = self
                .type_mapping(result_type)
                .expect("borrowed pattern result has its mapping");

            let CodegenTypeKind::Pointer { target, .. } = mapping.kind() else {
                panic!("borrowed pattern result has a pointer representation");
            };

            *target
        } else {
            result_type
        };

        let projected = match projection {
            PatternProjection::ProductField(field) => self.project_declared_field(
                subject,
                subject_type,
                bray_ir::MirFieldReference::Struct(field),
            )?,
            PatternProjection::TupleElement(index) => self.aggregate_field_value(
                subject,
                subject_type,
                usize::try_from(index.raw()).expect("tuple index fits usize"),
            )?,
            PatternProjection::ElementFromStart(index) => {
                self.array_element_value(subject, subject_type, u64::from(index.raw()))?
            }
            PatternProjection::ElementFromEnd(index) => {
                let mapping = self
                    .type_mapping(subject_type)
                    .expect("array projection has its type mapping");

                let CodegenTypeKind::Array { length, .. } = mapping.kind() else {
                    panic!("array projection has its array mapping");
                };

                let length = *length;

                let ordinal = length
                    .checked_sub(u64::from(index.raw()))
                    .and_then(|value| value.checked_sub(1))
                    .expect("checked MIR construction translation requires an established mapping or value");

                self.array_element_value(subject, subject_type, ordinal)?
            }
            PatternProjection::ActiveUnionPayloadField { variant, field } => {
                self.project_union_payload(subject, subject_type, variant, field, projected_type)?
            }
            PatternProjection::NullableValue => {
                self.project_value(subject, subject_type, projected_type)?
            }
            PatternProjection::OwnedTarget => {
                let pointer = pointer_value(subject).expect(
                    "checked MIR construction translation requires an established mapping or value",
                );

                self.place_value(pointer, projected_type, "pattern.owned")?
            }
        };

        match operation {
            PatternOperation::Observe | PatternOperation::Consume | PatternOperation::Copy => {
                self.snapshot_value(projected, projected_type, "pattern.value")
            }
            PatternOperation::SharedBorrow | PatternOperation::MutableBorrow => {
                let mapped = self.types.map(projected_type)?;
                let storage = self.allocate_temporary(mapped, "pattern.borrow")?;

                self.store_value(storage, projected, projected_type)?;

                Ok(storage.into())
            }
            PatternOperation::Recovered => panic!(
                "checked MIR construction translation violated an established compiler contract"
            ),
        }
    }

    pub(super) fn project_declared_field(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        reference: bray_ir::MirFieldReference,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self.aggregate_fields(subject_type);

        let index = fields
            .iter()
            .position(|field| field.reference() == Some(reference))
            .expect("declared field has its checked aggregate layout");

        self.aggregate_field_value(subject, subject_type, index)
    }

    pub(super) fn aggregate_field_value(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        index: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self.aggregate_fields(subject_type);

        self.layout_field_value(subject, &fields, index)
    }

    pub(super) fn layout_field_value(
        &mut self,
        subject: BasicValueEnum<'context>,
        fields: &[CodegenFieldLayout],
        index: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let field = fields
            .get(index)
            .expect("aggregate field ordinal is checked");

        if let BasicValueEnum::PointerValue(storage) = subject {
            let pointer = self.representation_offset_pointer(storage, field.offset_bytes())?;

            self.place_value(pointer, field.ty(), "aggregate.field")
        } else {
            let value = extract_value(
                &self.builder,
                subject,
                self.aggregate_element(fields, index)?,
            )?;

            self.initialize_value(value, field.ty(), "aggregate.field")
        }
    }

    pub(super) fn array_element_value(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        index: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(subject_type)
            .expect("array value has its mapping");

        let CodegenTypeKind::Array { element, length } = mapping.kind() else {
            panic!("array value has its array representation");
        };

        let element = *element;

        assert!(index < *length, "array value index is checked");

        if self.types.is_address_value(subject_type)? {
            let storage = pointer_value(subject).expect("array value retains its storage");

            let offset = self
                .mapped_type_size(element)
                .checked_mul(index)
                .expect("array element offset fits its checked layout");

            let pointer = self.representation_offset_pointer(storage, offset)?;

            self.place_value(pointer, element, "array.element")
        } else {
            extract_value(
                &self.builder,
                subject,
                crate::conversion::resource_limit(index, "array element index")?,
            )
        }
    }

    pub(super) fn aggregate_fields(
        &self,
        ty: bray_symbols::TypeId,
    ) -> std::sync::Arc<[bray_codegen::CodegenFieldLayout]> {
        let mapping = self.type_mapping(ty).unwrap_or_else(|| {
            panic!(
                "codegen instance {:?} has no aggregate mapping for type {ty:?}",
                self.instance.key()
            )
        });

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            panic!(
                "type {ty:?} must map to an aggregate, got {:?}",
                mapping.kind()
            );
        };

        // Composite conversion releases the mapping borrow while translating child values.
        fields.clone()
    }

    pub(super) fn project_union_payload(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        field: bray_symbols::UnionPayloadFieldSymbolId,
        result_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self.type_mapping(subject_type).expect(
            "checked MIR construction translation requires an established mapping or value",
        );

        let layout = mapping
            .kind()
            .union_variant(variant)
            .and_then(|layout| layout.payload_field(field))
            .expect(
                "checked MIR construction translation requires an established mapping or value",
            );

        let offset = layout.offset_bytes();

        let storage = if self.types.is_address_value(subject_type)? {
            pointer_value(subject).expect("union value retains its storage")
        } else {
            let storage = self.allocate_temporary(subject.get_type(), "pattern.union")?;

            llvm(self.builder.build_store(storage, subject))?;

            storage
        };

        let field = self.representation_offset_pointer(storage, offset)?;

        self.place_value(field, result_type, "pattern.union.field")
    }
}
