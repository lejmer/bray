use bray_codegen::CodegenSpecialization;
use bray_symbols::GenericSubstitutionId;

use super::super::super::{CodegenPreparationError, Compilation};
use super::encoding::StructuralValueEncoder;
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation::product) fn codegen_specialization(
        &self,
        substitution: GenericSubstitutionId,
    ) -> Result<CodegenSpecialization, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        values
            .require_concrete_substitution(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let substitution = values
            .generic_substitution_data(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if substitution.bindings().is_empty() {
            return Ok(CodegenSpecialization::NonGeneric);
        }

        let binding_context = self.binding_context(&self.state.cancellation)?;

        let arguments = substitution
            .bindings()
            .iter()
            .map(|binding| {
                StructuralValueEncoder::argument_key(values, &binding_context, binding.argument())
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CodegenSpecialization::generic(arguments))
    }
}
