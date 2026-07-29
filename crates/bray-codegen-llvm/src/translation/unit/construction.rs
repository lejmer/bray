use super::core::UnitTranslator;
use super::support::{
    aggregate_element, aggregate_value_length, extract_value, insert_value, llvm, next_helper,
    pointer_value, u128_words,
};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenTypeKind};
use bray_ir::{
    ConstructionInputId, ConstructionTarget, ConversionTarget, MirAggregate, MirAggregateKind,
    MirConstruction, MirConstructionInput, MirHelperReference, MirOperation, PatternOperation,
    PatternProjection, SelectedConversion,
};
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_aggregate(
        &mut self,
        operation: &MirOperation,
        aggregate: &MirAggregate,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation)?;

        // The mapping is immutable, but operand translation mutates task-local caches.
        let kind = self
            .request
            .mappings()
            .ty(result)
            .map(|mapping| mapping.kind().clone())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut value = self.types.map(result)?.const_zero();

        match aggregate.kind() {
            MirAggregateKind::Tuple => {
                let CodegenTypeKind::Aggregate(fields) = &kind else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                for (index, operand) in aggregate.operands().iter().enumerate() {
                    let operand = self.operand(operand)?;

                    let element =
                        super::support::aggregate_value_element(
                            self.request.mappings(),
                            fields,
                            index,
                        )?;

                    value = insert_value(&self.builder, value, operand, element)?;
                }
            }
            MirAggregateKind::Array => {
                for (index, operand) in aggregate.operands().iter().enumerate() {
                    let operand = self.operand(operand)?;

                    value = insert_value(&self.builder, value, operand, index)?;
                }
            }
            MirAggregateKind::RepeatedArray => {
                let [element, _count] = aggregate.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let CodegenTypeKind::Array { length, .. } = kind else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let element = self.operand(element)?;

                let length =
                    usize::try_from(length).map_err(|_| CodegenFailure::ResourceExhausted)?;

                for index in 0..length {
                    value = insert_value(&self.builder, value, element, index)?;
                }
            }
        }

        Ok(value)
    }

    pub(super) fn translate_construction(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        operation: &MirOperation,
        construction: &MirConstruction,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let helpers = self.operation_helpers(operation_id)?;
        let mut helpers = helpers.iter();
        let mut inputs = Vec::with_capacity(construction.inputs().len());

        for input in construction.inputs() {
            let value = match input {
                MirConstructionInput::Explicit { value, .. } => self.operand(value)?,
                MirConstructionInput::Default { provider, .. } => {
                    let helper = next_helper(
                        &mut helpers,
                        &MirHelperReference::ConstructionDefault(*provider),
                    )?;

                    self.invoke_helper(helper, &inputs)?
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
                }
            };

            inputs.push(value);
        }

        let result = self.operation_result_type(operation)?;

        match construction.target() {
            ConstructionTarget::Struct(_) => {
                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                self.construct_product(result, construction, &inputs)
            }
            ConstructionTarget::UnionVariant(variant) => {
                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                self.construct_union(result, variant, construction, &inputs)
            }
            ConstructionTarget::TypeForm { callable, .. } => {
                let helper = next_helper(&mut helpers, &MirHelperReference::TypeForm(callable))?;

                let value = self
                    .invoke_helper(helper, &inputs)?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Ok(value)
            }
        }
    }

    pub(super) fn construct_product(
        &mut self,
        result: bray_symbols::TypeId,
        construction: &MirConstruction,
        inputs: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        // Field layout is shared and must outlive the mapping borrow during operand translation.
        let fields = fields.clone();
        let mut value = self.types.map(result)?.const_zero();

        for (input, input_value) in construction.inputs().iter().zip(inputs) {
            let (MirConstructionInput::Explicit { input, .. }
            | MirConstructionInput::Default { input, .. }) = input;

            let ConstructionInputId::StructField(field) = input else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            let index = fields
                .iter()
                .position(|layout| {
                    layout.reference() == Some(bray_ir::MirFieldReference::Struct(*field))
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            value = insert_value(
                &self.builder,
                value,
                *input_value,
                usize::try_from(aggregate_element(
                    self.request.mappings(),
                    &fields,
                    index,
                )?)
                    .map_err(|_| CodegenFailure::ResourceExhausted)?,
            )?;
        }

        Ok(value)
    }

    pub(super) fn construct_union(
        &mut self,
        result: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        construction: &MirConstruction,
        inputs: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag = *tag;

        let variant = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .cloned()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let llvm_type = self.types.map(result)?;
        let storage = llvm(self.builder.build_alloca(llvm_type, "construction.union"))?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag_value = tag_type.const_int_arbitrary_precision(&u128_words(variant.tag()));

        llvm(self.builder.build_store(storage, tag_value))?;

        for (input, input_value) in construction.inputs().iter().zip(inputs) {
            let (MirConstructionInput::Explicit { input, .. }
            | MirConstructionInput::Default { input, .. }) = input;

            let ConstructionInputId::UnionPayloadField(field) = input else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            let layout = variant
                .fields()
                .iter()
                .find(|layout| {
                    layout.reference() == Some(bray_ir::MirFieldReference::UnionPayload(*field))
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let destination = self.constant_offset_pointer(storage, layout.offset_bytes())?;

            llvm(self.builder.build_store(destination, *input_value))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "construction.union.value"),
        )
    }

    pub(super) fn translate_conversion_plan<'mapping>(
        &mut self,
        value: BasicValueEnum<'context>,
        conversion: &SelectedConversion,
        helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match conversion.target() {
            ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {
                self.convert(value, conversion.source_type(), conversion.target_type())
            }
            ConversionTarget::Trait { fulfillment, .. } => {
                let helper = next_helper(helpers, &MirHelperReference::Conversion(*fulfillment))?;

                self.invoke_helper(helper, &[value])?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)
            }
            ConversionTarget::Composite(children) => {
                let source_fields = self.aggregate_fields(conversion.source_type())?;
                let target_fields = self.aggregate_fields(conversion.target_type())?;
                let mut result = self.types.map(conversion.target_type())?.const_zero();

                for (index, child) in children.iter().enumerate() {
                    let child_value = extract_value(
                        &self.builder,
                        value,
                        aggregate_element(self.request.mappings(), &source_fields, index)?,
                    )?;

                    let child_value =
                        self.translate_conversion_plan(child_value, child, helpers)?;

                    let element = super::support::aggregate_value_element(
                        self.request.mappings(),
                        &target_fields,
                        index,
                    )?;

                    result = insert_value(&self.builder, result, child_value, element)?;
                }

                Ok(result)
            }
        }
    }

    pub(super) fn translate_pattern_projection(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        result_type: bray_symbols::TypeId,
        projection: PatternProjection,
        operation: PatternOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let projected = match projection {
            PatternProjection::ProductField(field) => self.project_declared_field(
                subject,
                subject_type,
                bray_ir::MirFieldReference::Struct(field),
            )?,
            PatternProjection::TupleElement(index) | PatternProjection::ElementFromStart(index) => {
                extract_value(&self.builder, subject, index.raw())?
            }
            PatternProjection::ElementFromEnd(index) => {
                let length = aggregate_value_length(subject)?;

                let ordinal = length
                    .checked_sub(index.raw())
                    .and_then(|value| value.checked_sub(1))
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                extract_value(&self.builder, subject, ordinal)?
            }
            PatternProjection::ActiveUnionPayloadField { variant, field } => {
                self.project_union_payload(subject, subject_type, variant, field, result_type)?
            }
            PatternProjection::NullableValue => {
                self.project_value(subject, subject_type, result_type)?
            }
            PatternProjection::OwnedTarget => {
                let pointer =
                    pointer_value(subject).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_load(
                    self.types.map(result_type)?,
                    pointer,
                    "pattern.owned",
                ))?
            }
        };

        match operation {
            PatternOperation::Observe | PatternOperation::Consume | PatternOperation::Copy => {
                Ok(projected)
            }
            PatternOperation::SharedBorrow | PatternOperation::MutableBorrow => {
                let storage = llvm(
                    self.builder
                        .build_alloca(projected.get_type(), "pattern.borrow"),
                )?;

                llvm(self.builder.build_store(storage, projected))?;

                Ok(storage.into())
            }
            PatternOperation::Recovered => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn project_declared_field(
        &self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        reference: bray_ir::MirFieldReference,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let BasicValueEnum::StructValue(subject) = subject else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let mapping = self
            .request
            .mappings()
            .ty(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let index = fields
            .iter()
            .position(|field| field.reference() == Some(reference))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        extract_value(
            &self.builder,
            subject.into(),
            aggregate_element(self.request.mappings(), fields, index)?,
        )
    }

    fn aggregate_fields(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<std::sync::Arc<[bray_codegen::CodegenFieldLayout]>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        // Composite conversion releases the mapping borrow while translating child values.
        Ok(fields.clone())
    }

    pub(super) fn project_union_payload(
        &mut self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        field: bray_symbols::UnionPayloadFieldSymbolId,
        result_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let layout = variants
            .iter()
            .find(|layout| layout.variant() == variant)
            .and_then(|layout| {
                layout.fields().iter().find(|field_layout| {
                    field_layout.reference()
                        == Some(bray_ir::MirFieldReference::UnionPayload(field))
                })
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let storage = llvm(
            self.builder
                .build_alloca(subject.get_type(), "pattern.union"),
        )?;

        llvm(self.builder.build_store(storage, subject))?;

        let field = self.constant_offset_pointer(storage, layout.offset_bytes())?;

        llvm(
            self.builder
                .build_load(self.types.map(result_type)?, field, "pattern.union.field"),
        )
    }
}
