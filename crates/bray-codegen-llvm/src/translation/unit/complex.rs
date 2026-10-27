use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::MirBinaryOperator;
use bray_symbols::TypeId;
use inkwell::values::{BasicValueEnum, FloatValue};

use super::core::UnitTranslator;
use super::support::{extract_value, float_predicate, insert_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn is_complex_operand(&self, ty: TypeId) -> bool {
        matches!(
            self.type_mapping(ty).map(|mapping| mapping.kind()),
            Some(CodegenTypeKind::Aggregate(_))
        )
    }

    fn complex_components(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: TypeId,
    ) -> Result<[FloatValue<'context>; 2], CodegenFailure> {
        let fields = self.aggregate_fields(ty);

        assert_eq!(
            fields.len(),
            2,
            "checked complex arithmetic has two real components"
        );

        Ok([
            self.layout_field_value(value, &fields, 0)?
                .into_float_value(),
            self.layout_field_value(value, &fields, 1)?
                .into_float_value(),
        ])
    }

    pub(super) fn complex_negate(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [real, imaginary] = self.complex_components(value, ty)?;

        let real = llvm(self.builder.build_float_neg(real, "complex.negate.real"))?;

        let imaginary = llvm(
            self.builder
                .build_float_neg(imaginary, "complex.negate.imaginary"),
        )?;

        self.construct_positional_product(ty, &[real.into(), imaginary.into()])
    }

    pub(super) fn complex_binary(
        &mut self,
        operator: MirBinaryOperator,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
        ty: TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let left = self.complex_components(left, ty)?;
        let right = self.complex_components(right, ty)?;

        if matches!(
            operator,
            MirBinaryOperator::Equal | MirBinaryOperator::NotEqual
        ) {
            let real = llvm(self.builder.build_float_compare(
                float_predicate(operator),
                left[0],
                right[0],
                "complex.compare.real",
            ))?;

            let imaginary = llvm(self.builder.build_float_compare(
                float_predicate(operator),
                left[1],
                right[1],
                "complex.compare.imaginary",
            ))?;

            let result = if operator == MirBinaryOperator::Equal {
                llvm(self.builder.build_and(real, imaginary, "complex.equal"))?
            } else {
                llvm(self.builder.build_or(real, imaginary, "complex.not_equal"))?
            };

            return Ok(result.into());
        }

        if let Some(role) = operator.runtime_role() {
            return self.complex_arithmetic(role, left, right, ty);
        }

        assert!(
            matches!(
                operator,
                MirBinaryOperator::Add | MirBinaryOperator::Subtract
            ),
            "complex multiplication, division and power use their selected numeric helper"
        );

        let real = self.float_binary(operator, left[0], right[0])?;
        let imaginary = self.float_binary(operator, left[1], right[1])?;

        self.construct_positional_product(ty, &[real, imaginary])
    }
    fn complex_arithmetic(
        &mut self,
        role: bray_runtime_interface::RuntimeAbiRole,
        left: [FloatValue<'context>; 2],
        right: [FloatValue<'context>; 2],
        ty: TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let float = left[0].get_type();
        let width = float.get_bit_width();
        let left = self.complex_words(left)?;
        let right = self.complex_words(right)?;
        let word_array = self.types.context().i64_type().array_type(4);
        let output = self.allocate_temporary(word_array, "complex.output")?;

        llvm(self.builder.build_store(output, word_array.const_zero()))?;

        let runtime = bray_ir::MirRuntimeReference::new(role, self.unit.target().runtime_abi());

        let width_value = self
            .types
            .context()
            .i32_type()
            .const_int(u64::from(width), false);

        self.invoke_native_runtime(
            runtime,
            &[width_value.into(), left.into(), right.into(), output.into()],
        )?;

        let words = llvm(
            self.builder
                .build_load(word_array, output, "complex.result.words"),
        )?;

        let wide = self.types.context().i128_type();
        let mut components = [float.const_zero(); 2];

        for (index, component) in components.iter_mut().enumerate() {
            let low = extract_value(
                &self.builder,
                words,
                u32::try_from(index * 2).expect("component word ordinal fits u32"),
            )?
            .into_int_value();

            let high = extract_value(
                &self.builder,
                words,
                u32::try_from(index * 2 + 1).expect("component word ordinal fits u32"),
            )?
            .into_int_value();

            let low = llvm(self.builder.build_int_z_extend(low, wide, "complex.low"))?;
            let high = llvm(self.builder.build_int_z_extend(high, wide, "complex.high"))?;

            let high = llvm(self.builder.build_left_shift(
                high,
                wide.const_int(64, false),
                "complex.high.bits",
            ))?;

            let bits = llvm(self.builder.build_or(low, high, "complex.bits"))?;

            let bits = llvm(
                self.builder.build_int_truncate_or_bit_cast(
                    bits,
                    self.types
                        .context()
                        .custom_width_int_type(
                            std::num::NonZeroU32::new(width).expect("component width is nonzero"),
                        )
                        .expect("IEEE bit width is a valid LLVM integer type"),
                    "complex.component.bits",
                ),
            )?;

            *component = llvm(
                self.builder
                    .build_bit_cast(bits, float, "complex.component"),
            )?
            .into_float_value();
        }

        self.construct_positional_product(ty, &[components[0].into(), components[1].into()])
    }

    fn complex_words(
        &mut self,
        components: [FloatValue<'context>; 2],
    ) -> Result<inkwell::values::PointerValue<'context>, CodegenFailure> {
        let context = self.types.context();
        let word = context.i64_type();
        let wide = context.i128_type();
        let word_array = word.array_type(4);
        let mut words: BasicValueEnum<'context> = word_array.const_zero().into();

        for (index, component) in components.into_iter().enumerate() {
            let width = std::num::NonZeroU32::new(component.get_type().get_bit_width())
                .expect("component width is nonzero");

            let bits = llvm(
                self.builder.build_bit_cast(
                    component,
                    context
                        .custom_width_int_type(width)
                        .expect("IEEE bit width is a valid LLVM integer type"),
                    "complex.input.bits",
                ),
            )?
            .into_int_value();

            let bits = llvm(self.builder.build_int_z_extend_or_bit_cast(
                bits,
                wide,
                "complex.input.wide",
            ))?;

            let low = llvm(
                self.builder
                    .build_int_truncate(bits, word, "complex.input.low"),
            )?;

            let high = llvm(self.builder.build_right_shift(
                bits,
                wide.const_int(64, false),
                false,
                "complex.input.high.bits",
            ))?;

            let high = llvm(
                self.builder
                    .build_int_truncate(high, word, "complex.input.high"),
            )?;

            words = insert_value(&self.builder, words, low.into(), index * 2)?;
            words = insert_value(&self.builder, words, high.into(), index * 2 + 1)?;
        }

        let storage = self.allocate_temporary(word_array, "complex.input")?;

        llvm(self.builder.build_store(storage, words))?;

        Ok(storage)
    }
}
