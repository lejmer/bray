use std::sync::Arc;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundReferenceTarget, BoundUnitKey, CheckedTemplate, CheckedTemplateInputKind,
    CheckedTemplateKind,
};
use bray_ir::MirPredicateReference;
use bray_symbols::{
    CallableContractTemplate, CallableContractTemplateQuery, CallableContractsQuery,
    CallableSignatureQuery, GenericDeclarationTemplateQuery, GenericOwnerId,
    PredicateDefinitionState, PredicateSignatureTemplateQuery, SymbolOrdinal, SymbolQueryRequest,
};

use crate::compilation::binder::{binding_query_error, imported_declaration_template_at};
use crate::compilation::template::{
    SourceTemplateInput, callable_template_inputs, generic_template_inputs, resolved_input_type,
};
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

                signature
                    .value()
                    .parameters()
                    .iter()
                    .map(|parameter| resolved_input_type(self, parameter.ty()).map_err(Into::into))
                    .collect()
            }
        }
    }

    pub(in crate::compilation::product::realization) fn predicate_checked_template(
        &self,
        reference: &MirPredicateReference,
        cancellation: &CancellationToken,
    ) -> Result<Arc<CheckedTemplate>, CodegenPreparationError> {
        let context = self.binding_context(cancellation)?;

        let (owner, kind, ordinal) = match reference {
            MirPredicateReference::Clause { callable, ordinal } => (
                callable.instance().definition().symbol(),
                CheckedTemplateKind::CallableContract,
                *ordinal,
            ),
            MirPredicateReference::Predicate(predicate) => (
                predicate.definition().into_any(),
                CheckedTemplateKind::PredicateDefinition,
                SymbolOrdinal::new(0),
            ),
            MirPredicateReference::Condition { .. } => {
                panic!("erased conditions lower directly from their retained terms")
            }
        };

        if let Some(address) = context
            .imported_semantic_address(owner)
            .map_err(binding_query_error)?
        {
            let imported = imported_declaration_template_at(&context, address, kind, ordinal)
                .map_err(binding_query_error)?;

            if imported.diagnostics().has_errors() {
                return Err(CodegenPreparationError::Diagnostics(
                    imported.diagnostics().clone(),
                ));
            }

            // The helper retains the immutable graph independently of this imported query view.
            return Ok(Arc::new(
                imported
                    .value()
                    .as_ref()
                    .expect("runtime predicate has its complete imported checked template")
                    .template()
                    .clone(),
            ));
        }

        let generic_parameters = if let Some(owner) = GenericOwnerId::try_new(owner) {
            let generic = context
                .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                    owner,
                ))
                .map_err(binding_query_error)?;

            if generic.diagnostics().has_errors() {
                return Err(CodegenPreparationError::Diagnostics(
                    generic.diagnostics().clone(),
                ));
            }

            generic.value().parameters().to_vec()
        } else {
            Vec::new()
        };

        let (key, expression, inputs, dependency) = match reference {
            MirPredicateReference::Clause { callable, ordinal } => {
                let callable = callable.instance().definition().callable_symbol();

                let template = context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableContractTemplateQuery>::new(
                        callable,
                    ))
                    .map_err(binding_query_error)?;

                let clauses = context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                        callable,
                    ))
                    .map_err(binding_query_error)?;

                let signature = context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                        callable,
                    ))
                    .map_err(binding_query_error)?;

                for diagnostics in [
                    template.diagnostics(),
                    clauses.diagnostics(),
                    signature.diagnostics(),
                ] {
                    if diagnostics.has_errors() {
                        return Err(CodegenPreparationError::Diagnostics(diagnostics.clone()));
                    }
                }

                let CallableContractTemplate::Source(template) = template.value() else {
                    panic!("local runtime requirement retains its source contract");
                };

                let expression = template
                    .expressions()
                    .iter()
                    .find(|expression| expression.ordinal() == *ordinal)
                    .expect("runtime requirement retains its selected expression");

                let predicate = clauses
                    .value()
                    .invocation_preconditions()
                    .iter()
                    .find(|clause| clause.ordinal() == *ordinal)
                    .and_then(|clause| clause.predicate())
                    .expect("runtime requirement has its checked predicate meaning");

                let owner = context
                    .symbol_key(owner)
                    .map_err(binding_query_error)?
                    .expect("runtime requirement owner has stable identity")
                    .clone();

                let source = self.bound_source(expression.unit_syntax())?;

                let key = BoundUnitKey::contract_clause(owner, source)
                    .expect("selected runtime requirement has a callable contract unit");

                let inputs = callable_template_inputs(self, signature.value(), generic_parameters)?;

                (
                    key,
                    expression.expression().syntax(),
                    inputs,
                    predicate.dependency_contract(),
                )
            }
            MirPredicateReference::Predicate(predicate) => {
                let definition = self
                    .predicate_definition_with_cancellation(predicate.definition(), cancellation)?;

                let signature = context
                    .resolve_symbol_query(
                        SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                            predicate.definition(),
                        ),
                    )
                    .map_err(binding_query_error)?;

                for diagnostics in [definition.diagnostics(), signature.diagnostics()] {
                    if diagnostics.has_errors() {
                        return Err(CodegenPreparationError::Diagnostics(diagnostics.clone()));
                    }
                }

                let PredicateDefinitionState::Defined(definition) = definition.value() else {
                    panic!("runtime evaluation requires a checked defined predicate");
                };

                let key = self
                    .predicate_definition_key(predicate.definition())?
                    .expect("local checked predicate retains its definition unit");

                let expression = key.source().syntax();
                let mut inputs = Vec::new();

                for (ordinal, parameter) in signature.value().parameters().iter().enumerate() {
                    inputs.push(SourceTemplateInput::new(
                        CheckedTemplateInputKind::Parameter(SymbolOrdinal::new(
                            u32::try_from(ordinal)
                                .expect("predicate parameter ordinal fits its symbol table"),
                        )),
                        Some(BoundReferenceTarget::Surface(parameter.parameter().into())),
                        resolved_input_type(self, parameter.ty())?,
                    ));
                }

                inputs.extend(generic_template_inputs(self, generic_parameters)?);

                (
                    key,
                    expression,
                    inputs,
                    definition.semantic().dependency_contract(),
                )
            }
            MirPredicateReference::Condition { .. } => {
                unreachable!("erased predicate was handled before source lookup")
            }
        };

        let checked =
            self.checked_source_template(key, kind, expression, inputs, dependency, cancellation)?;

        match checked.value() {
            Some(template) => Ok(Arc::new(template.clone())),
            None => Err(CodegenPreparationError::Diagnostics(
                checked.diagnostics().clone(),
            )),
        }
    }
}
