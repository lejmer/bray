use bray_codegen::CodegenFailure;
use inkwell::values::{BasicValueEnum, FloatValue};

use super::core::UnitTranslator;
use super::support::{extract_value, insert_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn numeric_arithmetic<const COMPONENTS: usize>(
        &mut self,
        role: bray_runtime_interface::RuntimeAbiRole,
        left: [FloatValue<'context>; COMPONENTS],
        right: [FloatValue<'context>; COMPONENTS],
    ) -> Result<[FloatValue<'context>; COMPONENTS], CodegenFailure> {
        let float = left[0].get_type();
        let width = float.get_bit_width();
        let left = self.numeric_words(left)?;
        let right = self.numeric_words(right)?;

        let word_array =
            self.types.context().i64_type().array_type(
                u32::try_from(COMPONENTS * 2).expect("numeric record word count fits u32"),
            );

        let output = self.allocate_temporary(word_array, "numeric.output")?;

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
                .build_load(word_array, output, "numeric.result.words"),
        )?;

        let wide = self.types.context().i128_type();
        let mut components = [float.const_zero(); COMPONENTS];

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

            let low = llvm(self.builder.build_int_z_extend(low, wide, "numeric.low"))?;
            let high = llvm(self.builder.build_int_z_extend(high, wide, "numeric.high"))?;

            let high = llvm(self.builder.build_left_shift(
                high,
                wide.const_int(64, false),
                "numeric.high.bits",
            ))?;

            let bits = llvm(self.builder.build_or(low, high, "numeric.bits"))?;

            let bits = llvm(
                self.builder.build_int_truncate_or_bit_cast(
                    bits,
                    self.types
                        .context()
                        .custom_width_int_type(
                            std::num::NonZeroU32::new(width).expect("component width is nonzero"),
                        )
                        .expect("IEEE bit width is a valid LLVM integer type"),
                    "numeric.component.bits",
                ),
            )?;

            *component = llvm(
                self.builder
                    .build_bit_cast(bits, float, "numeric.component"),
            )?
            .into_float_value();
        }

        Ok(components)
    }

    fn numeric_words<const COMPONENTS: usize>(
        &mut self,
        components: [FloatValue<'context>; COMPONENTS],
    ) -> Result<inkwell::values::PointerValue<'context>, CodegenFailure> {
        let context = self.types.context();
        let word = context.i64_type();
        let wide = context.i128_type();

        let word_array = word
            .array_type(u32::try_from(COMPONENTS * 2).expect("numeric record word count fits u32"));

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
                    "numeric.input.bits",
                ),
            )?
            .into_int_value();

            let bits = llvm(self.builder.build_int_z_extend_or_bit_cast(
                bits,
                wide,
                "numeric.input.wide",
            ))?;

            let low = llvm(
                self.builder
                    .build_int_truncate(bits, word, "numeric.input.low"),
            )?;

            let high = llvm(self.builder.build_right_shift(
                bits,
                wide.const_int(64, false),
                false,
                "numeric.input.high.bits",
            ))?;

            let high = llvm(
                self.builder
                    .build_int_truncate(high, word, "numeric.input.high"),
            )?;

            words = insert_value(&self.builder, words, low.into(), index * 2)?;
            words = insert_value(&self.builder, words, high.into(), index * 2 + 1)?;
        }

        let storage = self.allocate_temporary(word_array, "numeric.input")?;

        llvm(self.builder.build_store(storage, words))?;

        Ok(storage)
    }
}
