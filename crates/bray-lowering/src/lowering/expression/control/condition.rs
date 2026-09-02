use bray_ir::MirOperand;
use bray_symbols::ConstantValueKind;

use super::super::super::LoweringError;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn constant_boolean(
        &self,
        operand: &MirOperand,
    ) -> Result<Option<bool>, LoweringError> {
        let MirOperand::Constant { value, .. } = operand else {
            return Ok(None);
        };

        let data = self
            .input
            .semantic_values()
            .constant_value_data(*value)?;

        match data.kind() {
            ConstantValueKind::Boolean(value) => Ok(Some(*value)),
            _ => Ok(None),
        }
    }
}
