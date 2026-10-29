use bray_bound_tree::BoundExpressionId;
use bray_compiler_known::ImplementationHook;
use bray_ir::{MirBlockId, MirOperand, MirPlace};
use bray_symbols::TypeId;

use crate::lowering::{LoweringError, block::LoweredExpression, lowerer::Lowerer};
use crate::synthetic::GeneratorIterationLowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::expression::control) fn generator_cursor(
        &mut self,
        id: BoundExpressionId,
        block: MirBlockId,
        value: MirOperand,
        generator: TypeId,
        cursor: TypeId,
        hook: ImplementationHook,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let source = self.expression_source(id);

        if let MirOperand::Move(place) = &value {
            self.set_storage_initialized(block, &source, place, false)?;
        }

        let mut lowering = GeneratorIterationLowerer {
            values: self.input.semantic_values(),
            symbols: self.input.available_compiler_known_symbols(),
            builder: &mut self.builder,
            source,
        };

        lowering.generator_cursor(block, value, generator, cursor, hook)
    }

    pub(in crate::lowering::expression::control) fn generator_next(
        &mut self,
        id: BoundExpressionId,
        block: MirBlockId,
        cursor: MirPlace,
        result: TypeId,
        hook: ImplementationHook,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.expression_source(id);

        let mut lowering = GeneratorIterationLowerer {
            values: self.input.semantic_values(),
            symbols: self.input.available_compiler_known_symbols(),
            builder: &mut self.builder,
            source: source.clone(),
        };

        let (block, value) = lowering.generator_next(block, cursor, result, hook)?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }
}
