use bray_bound_tree::{BoundBlockId, BoundBlockItem, BoundLocalBinding};
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
    pub(super) fn lower_block(
        &mut self,
        id: BoundBlockId,
        mut current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let block = self
            .input
            .unit()
            .view()
            .block(id)
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?;

        if block.is_recovered() {
            return Err(LoweringError::RecoveredBoundNode(id.into()));
        }

        let source = self.source(block.origin());

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

        Ok(LoweredExpression::continuing(current, None, source))
    }

    pub(super) fn lower_yielding_block(
        &mut self,
        id: BoundBlockId,
        current: MirBlockId,
        target: MirBlockId,
        result_type: TypeId,
    ) -> Result<LoweredExpression, LoweringError> {
        let syntax = self
            .input
            .unit()
            .view()
            .block(id)
            .map(|block| block.origin().source_anchor().syntax())
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?;

        self.yield_targets.push(YieldTarget::Result {
            syntax,
            block: target,
            result_type,
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
            return Err(LoweringError::RecoveredBoundNode(binding.pattern().into()));
        }

        let initializer = self.lower_expression(binding.initializer(), current)?;

        let Some(current) = initializer.block else {
            return Ok(initializer);
        };

        let Some(value) = initializer.value else {
            return Err(LoweringError::MissingOperationResult(binding.initializer()));
        };

        let source = self.source(binding.origin());
        let current = self.lower_pattern_bindings(binding.pattern(), value, current)?;

        Ok(LoweredExpression::continuing(current, None, source))
    }
}
