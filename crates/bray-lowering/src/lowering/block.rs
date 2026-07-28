use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundLocalBinding, StorageBinding, StorageBindingTarget,
};
use bray_ir::{MirBlockId, MirOperand, MirOperationKind, MirSourceAnchor};

use super::lowerer::Lowerer;
use super::LoweringError;

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
        let mut value = None;

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
            value = lowered.value;
        }

        Ok(LoweredExpression::continuing(current, value, source))
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
            return Err(LoweringError::MissingOperationResult(
                binding.initializer(),
            ));
        };

        let ([] | [_]) = binding.bindings() else {
            return Err(LoweringError::UnsupportedPattern(binding.pattern()));
        };

        let Some(binding_id) = binding.bindings().first().copied() else {
            return Ok(LoweredExpression::continuing(
                current,
                None,
                self.source(binding.origin()),
            ));
        };

        let binding_type = self
            .input
            .pattern_facts()
            .binding_type(binding_id)
            .ok_or_else(|| LoweringError::UnsupportedPattern(binding.pattern()))?;

        if binding_type.is_recovered() || binding_type.projection().is_some() {
            return Err(LoweringError::UnsupportedPattern(binding.pattern()));
        }

        let storage = self
            .input
            .storage_plan()
            .binding(StorageBindingTarget::Local(binding_id))
            .ok_or(LoweringError::UnsupportedPattern(binding.pattern()))?;

        let place = match storage {
            StorageBinding::Identity(identity) => {
                self.place_for_identity(identity, binding_type.ty(), binding.origin())?
            }
            StorageBinding::Access(access) => self.place_for_access(access)?,
        };

        let source = self.source(binding.origin());

        self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Store {
                destination: place,
                value,
            },
            None,
        )?;

        Ok(LoweredExpression::continuing(current, None, source))
    }
}
