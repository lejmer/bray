use bray_bound_tree::{
    BoundBlockExpression, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundLocalBinding,
};
use bray_ir::{MirBlockId, MirOperand, MirSourceAnchor};
use bray_symbols::TypeId;

use super::LoweringError;
use super::lowerer::{Lowerer, YieldTarget};

pub(super) struct LoweredExpression {
    pub(super) block: Option<MirBlockId>,
    pub(super) value: Option<MirOperand>,
    pub(super) source: MirSourceAnchor,
}
impl LoweredExpression {
    pub(super) const fn continuing(
        block: MirBlockId,
        value: Option<MirOperand>,
        source: MirSourceAnchor,
    ) -> Self {
        Self {
            block: Some(block),
            value,
            source,
        }
    }

    pub(super) const fn terminated(source: MirSourceAnchor) -> Self {
        Self {
            block: None,
            value: None,
            source,
        }
    }
}

impl Lowerer<'_> {
    pub(super) fn lower_block_expression(
        &mut self,
        expression_id: BoundExpressionId,
        expression: &BoundBlockExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.source(expression.origin());

        let (join, result, result_type) =
            self.push_result_join(expression_id, expression.origin())?;

        let completion = self.lower_yielding_block(
            expression.block(),
            current,
            join,
            result_type,
            self.active_scopes.len(),
        )?;

        self.finish_result_edge(completion, join, result_type)?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    pub(super) fn lower_block(
        &mut self,
        id: BoundBlockId,
        mut current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let block = self.input.unit().view().block(id).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingBoundNode {value:?}",
                value = id
            )
        });

        if block.is_recovered() {
            panic!(
                "lowering contract violation: RecoveredBoundNode {value:?}",
                value = id
            );
        }

        let source = self.source(block.origin());
        self.active_scopes.push(id);

        let result = (|| {
            for item in block.items() {
                let lowered = match item {
                    BoundBlockItem::LocalBinding(binding) => {
                        self.lower_local_binding(binding, current)?
                    }
                    BoundBlockItem::LocalConstant(_) => {
                        LoweredExpression::continuing(current, None, Self::retained_source(&source))
                    }
                    BoundBlockItem::Expression(expression) => {
                        self.lower_expression(*expression, current)?
                    }
                };

                let Some(continuation) = lowered.block else {
                    return Ok(lowered);
                };

                current = continuation;
            }

            current = self.finish_scope(id, current, &source, id.into())?;

            Ok(LoweredExpression::continuing(current, None, source))
        })();

        self.active_scopes.pop();

        result
    }

    pub(super) fn lower_yielding_block(
        &mut self,
        id: BoundBlockId,
        current: MirBlockId,
        target: MirBlockId,
        result_type: TypeId,
        scope_depth: usize,
    ) -> Result<LoweredExpression, LoweringError> {
        let syntax = self
            .input
            .unit()
            .view()
            .block(id)
            .map(|block| block.origin().source_anchor().syntax())
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingBoundNode {value:?}",
                    value = id
                )
            });

        self.yield_targets.push(YieldTarget::Result {
            syntax,
            block: target,
            result_type,
            scope_depth,
        });

        let completion = self.lower_block(id, current);

        self.yield_targets.pop();

        completion
    }

    fn lower_local_binding(
        &mut self,
        binding: &BoundLocalBinding,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        if binding.is_recovered() {
            panic!(
                "lowering contract violation: RecoveredBoundNode {value:?}",
                value = binding.pattern()
            );
        }

        let initializer = self.lower_expression(binding.initializer(), current)?;

        let Some(current) = initializer.block else {
            return Ok(initializer);
        };

        let Some(mut value) = initializer.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = binding.initializer()
            );
        };

        let source = self.source(binding.origin());

        if let Some(declared) = self
            .input
            .patterns()
            .pattern(binding.pattern())
            .map(|pattern| pattern.input_type())
        {
            let initializer_type = self.expression_type(binding.initializer());

            value = self
                .adapt_nullable_present(
                    binding.initializer(),
                    current,
                    Self::retained_source(&source),
                    value,
                    initializer_type,
                    declared,
                )?
                .0;
        }

        let current = self.lower_pattern_bindings(binding.pattern(), value, current)?;

        Ok(LoweredExpression::continuing(current, None, source))
    }
}
