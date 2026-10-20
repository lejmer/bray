use bray_bound_tree::{BoundBlockId, BoundExpression, BoundStructuredExpression, CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_symbols::TypeId;
use crate::fact::FactQueryError;
use super::build::SourceTemplateBuilder;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn conditional(&mut self, expression: &BoundStructuredExpression, ty: TypeId) -> Result<CheckedTemplateOperation, FactQueryError> {
        let conditions = expression.operands();
        let blocks = expression.blocks();
        assert_eq!(blocks.len(), conditions.len() + 1, "checked value-producing predicate conditional retains its else branch");
        let mut value = self.block_value(*blocks.last().expect("checked conditional retains an else block"))?;
        for (ordinal, (condition, branch)) in conditions.iter().zip(blocks).enumerate().rev() {
            let condition = self.expression(*condition)?;
            let when_true = self.block_value(*branch)?;
            let operation = CheckedTemplateOperation::Conditional { condition, when_true, when_false: value };
            if ordinal == 0 { return Ok(operation); }
            value = CheckedTemplateNodeId::new(u32::try_from(self.nodes.len()).expect("checked conditional fits its node table"));
            self.nodes.push(CheckedTemplateNode::new(operation, ty));
        }
        unreachable!("checked conditional retains at least one condition")
    }
    fn block_value(&mut self, block: BoundBlockId) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let block = self.unit.view().block(block).expect("checked predicate branch retains its block");
        let expression = block.items().iter().rev().find_map(|item| item.expression()).expect("checked value-producing block retains its result expression");
        let node = self.unit.view().expression(expression).expect("checked block expression is retained");
        let value = match node { BoundExpression::ControlTransfer(transfer) => transfer.operand().expect("checked predicate result carries a value"), _ => expression };
        self.expression(value)
    }
}
