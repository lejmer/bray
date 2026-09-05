use std::collections::BTreeMap;

use super::core::UnitTranslator;
use super::support::{
    aggregate_value_length, extract_value, insert_value, llvm, next_helper, pointer_value,
};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenResultMapping, CodegenTypeKind};
use bray_ir::{
    ConstructionInputId, ConstructionTarget, ConversionTarget, MirAggregate, MirAggregateKind,
    MirConstruction, MirConstructionInput, MirHelperReference, MirOperation, PatternOperation,
    PatternProjection, SelectedConversion,
};
use inkwell::values::BasicValueEnum;

#[derive(Clone, Copy)]
struct EvaluatedConstructionInput<'context> {
    value: BasicValueEnum<'context>,
    ty: bray_symbols::TypeId,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_aggregate(
        &mut self,
        operation: &MirOperation,
        aggregate: &MirAggregate,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation)?;

        // The mapping is immutable, but operand translation mutates task-local caches.
        let kind = self
            .type_mapping(result)
            .map(|mapping| mapping.kind().clone())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut value = self.types.map(result)?.const_zero();

        match aggregate.kind() {
            MirAggregateKind::Tuple | MirAggregateKind::Range => {
                let CodegenTypeKind::Aggregate(fields) = &kind else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                for (index, operand) in aggregate.operands().iter().enumerate() {
                    let operand = self.operand(operand)?;

                    let element = self.aggregate_value_element(fields, index)?;

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
                let [element] = aggregate.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let CodegenTypeKind::Array { length, .. } = kind else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let element = self.operand(element)?;

                let length = crate::conversion::resource_limit(length, "array_length")?;

                for index in 0..length {
                    value = insert_value(&self.builder, value, element, index)?;
                }
            }
            MirAggregateKind::NullablePresent => {
                let [operand] = aggregate.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let operand = self.operand(operand)?;

                value = self.construct_nullable_present(result, operand)?;
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
        let inputs = self.evaluate_construction_inputs(construction, &mut helpers)?;

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
                let input_values = inputs.iter().map(|input| input.value).collect::<Vec<_>>();

                let value = self
                    .invoke_helper(helper, &input_values)?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Ok(value)
            }
        }
    }

    fn evaluate_construction_inputs<'mapping>(
        &mut self,
        construction: &MirConstruction,
        helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
    ) -> Result<Vec<EvaluatedConstructionInput<'context>>, CodegenFailure> {
        let mut values = BTreeMap::new();

        for input in construction.inputs() {
            let MirConstructionInput::Explicit { ordinal, value, .. } = input else {
                continue;
            };

            let ty = self.operand_type(value)?;
            let value = self.operand(value)?;
            let input = EvaluatedConstructionInput { value, ty };

            if values.insert(*ordinal, input).is_some() {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }

        for input in construction.inputs() {
            let MirConstructionInput::Default {
                ordinal, provider, ..
            } = input
            else {
                continue;
            };

            let helper = next_helper(helpers, &MirHelperReference::ConstructionDefault(*provider))?;

            let (parameter_count, ty) = helper
                .symbol()
                .and_then(|key| self.request.mappings().symbol(key))
                .and_then(|symbol| {
                    let ty = match symbol.signature().result() {
                        CodegenResultMapping::Direct { ty, .. } => *ty,
                        CodegenResultMapping::Indirect { pointee, .. } => *pointee,
                        CodegenResultMapping::Void => return None,
                    };

                    Some((symbol.signature().parameters().len(), ty))
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let preceding = values
                .range(..*ordinal)
                .take(parameter_count)
                .map(|(_, input)| input.value)
                .collect::<Vec<_>>();

            if preceding.len() != parameter_count {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            let value = self
                .invoke_helper(helper, &preceding)?
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let input = EvaluatedConstructionInput { value, ty };

            if values.insert(*ordinal, input).is_some() {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }

        (0..construction.inputs().len())
            .map(|ordinal| {
                let ordinal =
                    crate::conversion::resource_limit(ordinal, "construction_field_ordinal")?;

                values
                    .get(&ordinal)
                    .copied()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)
            })
            .collect()
    }

    fn construct_product(
        &mut self,
        result: bray_symbols::TypeId,
        construction: &MirConstruction,
        inputs: &[EvaluatedConstructionInput<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        // Field layout is shared and must outlive the mapping borrow during operand translation.
        let fields = fields.clone();
        let mut value = self.types.map(result)?.const_zero();

        for construction_input in construction.inputs() {
            let (MirConstructionInput::Explicit { input, .. }
            | MirConstructionInput::Default { input, .. }) = construction_input;

            let input_value = inputs
                .get(crate::conversion::resource_limit::<usize, _>(
                    construction_input.ordinal(),
                    "construction_input_ordinal",
                )?)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let ConstructionInputId::StructField(field) = input else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            let index = fields
                .iter()
                .position(|layout| {
                    layout.reference() == Some(bray_ir::MirFieldReference::Struct(*field))
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let input_value =
                self.convert(input_value.value, input_value.ty, fields[index].ty())?;

            value = insert_value(
                &self.builder,
                value,
                input_value,
                crate::conversion::resource_limit::<usize, _>(
                    self.aggregate_element(&fields, index)?,
                    "aggregate_element_index",
                )?,
            )?;
        }

        Ok(value)
    }

    fn construct_union(
        &mut self,
        result: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        construction: &MirConstruction,
        inputs: &[EvaluatedConstructionInput<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let variant = mapping
            .kind()
            .union_variant(variant)
            .cloned()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let llvm_type = self.types.map(result)?;
        let storage = self.allocate_temporary(llvm_type, "construction.union")?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        self.store_union_tag(storage, *tag, variant.tag())?;

        for construction_input in construction.inputs() {
            let (MirConstructionInput::Explicit { input, .. }
            | MirConstructionInput::Default { input, .. }) = construction_input;

            let input_value = inputs
                .get(crate::conversion::resource_limit::<usize, _>(
                    construction_input.ordinal(),
                    "construction_input_ordinal",
                )?)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let ConstructionInputId::UnionPayloadField(field) = input else {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            };

            let layout = variant
                .payload_field(*field)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let destination = self.constant_offset_pointer(storage, layout.offset_bytes())?;
            let input_value = self.convert(input_value.value, input_value.ty, layout.ty())?;

            llvm(self.builder.build_store(destination, input_value))?;
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
            ConversionTarget::Identity
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion => {
                self.convert(value, conversion.source_type(), conversion.target_type())
            }
            ConversionTarget::NullablePresent => {
                self.construct_nullable_present(conversion.target_type(), value)
            }
            ConversionTarget::Trait { fulfillment, .. } => {
                let helper = next_helper(helpers, &MirHelperReference::Conversion(*fulfillment))?;

                self.invoke_helper(helper, &[value])?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)
            }
            ConversionTarget::TraitConstraint { .. } => {
                Err(CodegenFailure::GeneratedModuleInvariant)
            }
            ConversionTarget::Composite(children) => {
                let source_fields = self.aggregate_fields(conversion.source_type())?;
                let target_fields = self.aggregate_fields(conversion.target_type())?;
                let mut result = self.types.map(conversion.target_type())?.const_zero();

                for (index, child) in children.iter().enumerate() {
                    let child_value = extract_value(
                        &self.builder,
                        value,
                        self.aggregate_element(&source_fields, index)?,
                    )?;

                    let child_value =
                        self.translate_conversion_plan(child_value, child, helpers)?;

                    let element = self.aggregate_value_element(&target_fields, index)?;

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
                let storage = self.allocate_temporary(projected.get_type(), "pattern.borrow")?;

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
            .type_mapping(subject_type)
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
            self.aggregate_element(fields, index)?,
        )
    }

    pub(super) fn aggregate_fields(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<std::sync::Arc<[bray_codegen::CodegenFieldLayout]>, CodegenFailure> {
        let mapping = self
            .type_mapping(ty)
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
            .type_mapping(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let layout = mapping
            .kind()
            .union_variant(variant)
            .and_then(|layout| layout.payload_field(field))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let storage = self.allocate_temporary(subject.get_type(), "pattern.union")?;

        llvm(self.builder.build_store(storage, subject))?;

        let field = self.constant_offset_pointer(storage, layout.offset_bytes())?;

        llvm(
            self.builder
                .build_load(self.types.map(result_type)?, field, "pattern.union.field"),
        )
    }
}
