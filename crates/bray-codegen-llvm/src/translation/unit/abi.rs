use bray_codegen::{CodegenAggregateCoercion, CodegenFailure};
use bray_symbols::TypeId;
use inkwell::values::{BasicValue, BasicValueEnum};

use super::core::UnitTranslator;
use super::support::{extract_value, insert_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn encode_abi_pieces(
        &mut self,
        value: BasicValueEnum<'context>,
        coercion: &CodegenAggregateCoercion,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let storage = self.allocate_temporary(value.get_type(), "abi.value")?;

        llvm(self.builder.build_store(storage, value))?;

        coercion
            .pieces()
            .iter()
            .map(|piece| {
                let pointer = self.constant_offset_pointer(storage, piece.offset_bytes)?;
                let ty = self.types.abi_scalar_type(piece.scalar)?;
                let value = llvm(self.builder.build_load(ty, pointer, "abi.piece"))?;

                value
                    .as_instruction_value()
                    .expect("ABI piece load is an instruction")
                    .set_alignment(1)
                    .map_err(CodegenFailure::unsupported_target_report)?;

                Ok(value)
            })
            .collect()
    }

    pub(super) fn decode_abi_pieces(
        &mut self,
        ty: TypeId,
        coercion: &CodegenAggregateCoercion,
        pieces: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        assert_eq!(
            coercion.pieces().len(),
            pieces.len(),
            "ABI register pieces match their mapping"
        );

        let ty = self.types.map(ty)?;
        let storage = self.allocate_temporary(ty, "abi.value")?;

        llvm(self.builder.build_store(storage, ty.const_zero()))?;

        for (piece, value) in coercion.pieces().iter().zip(pieces) {
            let pointer = self.constant_offset_pointer(storage, piece.offset_bytes)?;

            llvm(self.builder.build_store(pointer, *value))?
                .set_alignment(1)
                .map_err(CodegenFailure::unsupported_target_report)?;
        }

        llvm(self.builder.build_load(ty, storage, "abi.semantic.value"))
    }

    pub(super) fn encode_abi_result(
        &mut self,
        value: BasicValueEnum<'context>,
        coercion: &CodegenAggregateCoercion,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let pieces = self.encode_abi_pieces(value, coercion)?;

        if pieces.len() == 1 {
            return Ok(pieces[0]);
        }

        let mut result = self.types.coercion_result_type(coercion)?.const_zero();

        for (index, piece) in pieces.into_iter().enumerate() {
            result = insert_value(
                &self.builder,
                result,
                piece,
                crate::conversion::resource_limit(index, "ABI result piece")?,
            )?;
        }

        Ok(result)
    }

    pub(super) fn decode_abi_result(
        &mut self,
        value: BasicValueEnum<'context>,
        ty: TypeId,
        coercion: &CodegenAggregateCoercion,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let pieces = if coercion.pieces().len() == 1 {
            vec![value]
        } else {
            (0..coercion.pieces().len())
                .map(|index| {
                    extract_value(
                        &self.builder,
                        value,
                        crate::conversion::resource_limit(index, "ABI result piece")?,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        };

        self.decode_abi_pieces(ty, coercion, &pieces)
    }
}
