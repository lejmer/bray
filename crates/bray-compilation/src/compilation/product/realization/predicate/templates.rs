use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::CheckedTemplate;
use bray_ir::MirPredicateReference;
use bray_symbols::{CallableSignatureQuery, PredicateSignatureTemplateQuery, SymbolQueryRequest};

use crate::compilation::binder::binding_query_error;
use crate::compilation::template::callable_template_inputs;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product::realization) fn predicate_input_types(
        &self,
        reference: &MirPredicateReference,
        cancellation: &CancellationToken,
    ) -> Result<Vec<bray_symbols::TypeId>, CodegenPreparationError> {
        let context = self.binding_context(cancellation)?;

        match reference {
            MirPredicateReference::Condition { input_types, .. } => Ok(input_types.to_vec()),
            MirPredicateReference::Clause { callable, .. } => {
                let signature = context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                        callable.instance().definition().callable_symbol(),
                    ))
                    .map_err(binding_query_error)?;

                if signature.diagnostics().has_errors() {
                    return Err(CodegenPreparationError::Diagnostics(
                        signature.diagnostics().clone(),
                    ));
                }

                Ok(
                    callable_template_inputs(self, signature.value(), Vec::new())?
                        .into_iter()
                        .map(|input| input.ty)
                        .collect(),
                )
            }
            MirPredicateReference::Predicate(predicate) => {
                let signature = context
                    .resolve_symbol_query(
                        SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                            predicate.definition(),
                        ),
                    )
                    .map_err(binding_query_error)?;

                if signature.diagnostics().has_errors() {
                    return Err(CodegenPreparationError::Diagnostics(
                        signature.diagnostics().clone(),
                    ));
                }

                Ok(crate::compilation::template::predicate_template_inputs(self, signature.value(), Vec::new())?
                    .into_iter().map(|input| input.ty).collect())
            }
        }
    }

    pub(in crate::compilation::product::realization) fn predicate_checked_template(
        &self,
        reference: &MirPredicateReference,
        cancellation: &CancellationToken,
    ) -> Result<Arc<CheckedTemplate>, CodegenPreparationError> {
        let checked = match reference {
            MirPredicateReference::Predicate(predicate) => self.constant_predicate_template(predicate.definition(), cancellation)?,
            MirPredicateReference::Clause { callable, ordinal } => self.declared_predicate_template(callable.instance(), *ordinal, cancellation)?,
            MirPredicateReference::Condition { .. } => panic!("erased conditions lower directly from their retained terms"),
        };
        if checked.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(checked.into_parts().1));
        }
        Ok(Arc::new(checked.into_parts().0.expect("admitted runtime predicate retains its complete checked graph")))
    }
}
