use super::{encoding::sequence, values::OrderEncoder};
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;
use bray_symbols::StaticInstanceKey;
use std::sync::Arc;

impl Compilation {
    pub(in crate::compilation::product) fn static_declaration_order_prefix(
        &self,
        declaration: bray_symbols::StaticSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Vec<u8>, CodegenPreparationError> {
        let context = self.binding_context(cancellation)?;

        let encoder = OrderEncoder {
            compilation: self,
            context: &context,
        };

        Ok(super::encoding::sequence_prefix([
            encoder.symbol(declaration.into())?
        ]))
    }

    pub(in crate::compilation::product) fn static_cleanup_order_key(
        &self,
        instance: &StaticInstanceKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<[u8]>, CodegenPreparationError> {
        let context = self.binding_context(cancellation)?;

        let encoder = OrderEncoder {
            compilation: self,
            context: &context,
        };

        let declaration = instance.template().declaration();
        let template = self.static_instance_template(declaration)?;
        let requirements = template.value().witness_requirements();

        assert_eq!(
            requirements.len(),
            instance.selected_witnesses().len(),
            "closed static witness arity must match its template"
        );

        let mut witnesses = requirements
            .iter()
            .zip(instance.selected_witnesses())
            .map(|(requirement, implementation)| {
                Ok(sequence([
                    encoder.symbol_key(requirement)?,
                    encoder.implementation(*implementation)?,
                ]))
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        witnesses.sort_unstable();

        // The selected package configuration validates the complete target profile once.
        // Its terms are equal throughout this cleanup domain and need no per-static copy.
        Ok(sequence([
            encoder.symbol(declaration.into())?,
            encoder.substitution(instance.substitution())?,
            sequence(witnesses),
        ])
        .into_bytes())
    }
}
