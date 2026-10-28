use std::collections::BTreeMap;

use super::core::UnitTranslator;
use super::support::{llvm, next_helper};
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{
    ConstructionInputId, ConstructionTarget, ConversionTarget, MirAggregate, MirAggregateKind,
    MirConstruction, MirHelperReference, MirOperation, SelectedConversion,
};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_aggregate(
        &mut self,
        operation: &MirOperation,
        aggregate: &MirAggregate,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation);

        // The mapping is immutable, but operand translation mutates task-local caches.
        let kind = self
            .type_mapping(result)
            .map(|mapping| mapping.kind().clone())
            .expect(
                "checked MIR construction translation requires an established mapping or value",
            );

        let mapped = self.types.map(result)?;
        let mut value = self.initialize_value(mapped.const_zero(), result, "aggregate.value")?;

        match aggregate.kind() {
            MirAggregateKind::Tuple | MirAggregateKind::Range => {
                let CodegenTypeKind::Aggregate(fields) = &kind else {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
                };

                for (index, operand) in aggregate.operands().iter().enumerate() {
                    let operand = self.operand(operand)?;

                    value = self.insert_aggregate_field(value, fields, index, operand)?;
                }
            }
            MirAggregateKind::Array => {
                for (index, operand) in aggregate.operands().iter().enumerate() {
                    let operand = self.operand(operand)?;

                    value = self.insert_array_element(value, result, index, operand)?;
                }
            }
            MirAggregateKind::RepeatedArray => {
                let [element] = aggregate.operands() else {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
                };

                let CodegenTypeKind::Array { length, .. } = kind else {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
                };

                let element = self.operand(element)?;

                let length = crate::conversion::resource_limit(length, "array_length")?;

                for index in 0..length {
                    value = self.insert_array_element(value, result, index, element)?;
                }
            }
            MirAggregateKind::NullablePresent => {
                let [operand] = aggregate.operands() else {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
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
        let inputs = self.evaluate_construction_inputs(construction)?;

        let result = self.operation_result_type(operation);

        match construction.target() {
            ConstructionTarget::Struct(_) => {
                if helpers.next().is_some() {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
                }

                self.construct_product(result, construction, &inputs)
            }
            ConstructionTarget::UnionVariant(variant) => {
                if helpers.next().is_some() {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
                }

                self.construct_union(result, variant, construction, &inputs)
            }
            ConstructionTarget::TypeForm { callable, .. } => {
                let helper = next_helper(&mut helpers, &MirHelperReference::TypeForm(callable));
                let value = self.invoke_operation_helper(operation_id, helper, &inputs)?;
                let value = self.materialize_call_result(value, result)?;

                if helpers.next().is_some() {
                    panic!(
                        "checked MIR construction translation violated an established compiler contract"
                    );
                }

                Ok(value)
            }
        }
    }

    fn evaluate_construction_inputs(
        &mut self,
        construction: &MirConstruction,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let mut values = BTreeMap::new();

        for input in construction.inputs() {
            let value = self.operand(input.value())?;

            if values.insert(input.ordinal(), value).is_some() {
                panic!(
                    "checked MIR construction translation violated an established compiler contract"
                );
            }
        }

        (0..construction.inputs().len())
            .map(|ordinal| {
                let ordinal =
                    crate::conversion::resource_limit(ordinal, "construction_field_ordinal")?;

                Ok(values
                    .get(&ordinal)
                    .copied()
                    .expect("construction inputs must cover every ordinal"))
            })
            .collect()
    }

    fn construct_product(
        &mut self,
        result: bray_symbols::TypeId,
        construction: &MirConstruction,
        inputs: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self.type_mapping(result).expect(
            "checked MIR construction translation requires an established mapping or value",
        );

        let CodegenTypeKind::Aggregate(fields) = mapping.kind() else {
            panic!(
                "checked MIR construction translation violated an established compiler contract"
            );
        };

        // Field layout is shared and must outlive the mapping borrow during operand translation.
        let fields = fields.clone();
        let mapped = self.types.map(result)?;

        let mut value =
            self.initialize_value(mapped.const_zero(), result, "construction.product")?;

        for construction_input in construction.inputs() {
            let input = construction_input.input();

            let input_value = inputs
                .get(crate::conversion::resource_limit::<usize, _>(
                    construction_input.ordinal(),
                    "construction_input_ordinal",
                )?)
                .expect(
                    "checked MIR construction translation requires an established mapping or value",
                );

            let ConstructionInputId::StructField(field) = input else {
                panic!(
                    "checked MIR construction translation violated an established compiler contract"
                );
            };

            let index = fields
                .iter()
                .position(|layout| {
                    layout.reference() == Some(bray_ir::MirFieldReference::Struct(field))
                })
                .expect(
                    "checked MIR construction translation requires an established mapping or value",
                );

            value = self.insert_aggregate_field(value, &fields, index, *input_value)?;
        }

        Ok(value)
    }

    fn construct_union(
        &mut self,
        result: bray_symbols::TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        construction: &MirConstruction,
        inputs: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let mapping = self.type_mapping(result).expect(
            "checked MIR construction translation requires an established mapping or value",
        );

        let CodegenTypeKind::Union { tag, .. } = mapping.kind() else {
            panic!(
                "checked MIR construction translation violated an established compiler contract"
            );
        };

        let variant = mapping.kind().union_variant(variant).cloned().expect(
            "checked MIR construction translation requires an established mapping or value",
        );

        let llvm_type = self.types.map(result)?;
        let storage = self.allocate_temporary(llvm_type, "construction.union")?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        self.store_union_tag(storage, *tag, variant.tag())?;

        for construction_input in construction.inputs() {
            let input = construction_input.input();

            let input_value = inputs
                .get(crate::conversion::resource_limit::<usize, _>(
                    construction_input.ordinal(),
                    "construction_input_ordinal",
                )?)
                .expect(
                    "checked MIR construction translation requires an established mapping or value",
                );

            let ConstructionInputId::UnionPayloadField(field) = input else {
                panic!(
                    "checked MIR construction translation violated an established compiler contract"
                );
            };

            let layout = variant.payload_field(field).expect(
                "checked MIR construction translation requires an established mapping or value",
            );

            let destination = self.constant_offset_pointer(storage, layout.offset_bytes())?;

            self.store_value(destination, *input_value, layout.ty())?;
        }

        self.place_value(storage, result, "construction.union.value")
    }

    pub(super) fn translate_conversion_plan(
        &mut self,
        value: BasicValueEnum<'context>,
        conversion: &SelectedConversion,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match conversion.target() {
            ConversionTarget::CallableContract => Ok(value),
            ConversionTarget::Identity
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion => {
                self.convert(value, conversion.source_type(), conversion.target_type())
            }
            ConversionTarget::NullablePresent => {
                self.construct_nullable_present(conversion.target_type(), value)
            }
            ConversionTarget::Trait { .. } | ConversionTarget::TraitConstraint { .. } => {
                panic!(
                    "source conversion calls must be lowered through checked MIR outcomes before LLVM translation"
                )
            }
            ConversionTarget::Composite(children) => {
                let target_fields = self.aggregate_fields(conversion.target_type());
                let mapped = self.types.map(conversion.target_type())?;

                let mut result = self.initialize_value(
                    mapped.const_zero(),
                    conversion.target_type(),
                    "conversion.value",
                )?;

                for (index, child) in children.iter().enumerate() {
                    let child_value =
                        self.aggregate_field_value(value, conversion.source_type(), index)?;

                    let child_value = self.translate_conversion_plan(child_value, child)?;

                    result =
                        self.insert_aggregate_field(result, &target_fields, index, child_value)?;
                }

                Ok(result)
            }
        }
    }
}
