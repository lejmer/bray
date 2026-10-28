use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::MirBinaryOperator;
use bray_symbols::TypeId;
use inkwell::values::{BasicValueEnum, FloatValue};

use super::core::UnitTranslator;
use super::support::{float_predicate, llvm};

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
            let [real, imaginary] = self.numeric_arithmetic(role, left, right)?;

            return self.construct_positional_product(ty, &[real.into(), imaginary.into()]);
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
}
