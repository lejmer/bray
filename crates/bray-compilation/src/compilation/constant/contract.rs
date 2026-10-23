use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_checker::{ConstantCallContract, ConstantCallTarget};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableContractClauseKind, CallablePredicateContractsQuery, PredicateSignatureTemplateQuery,
    SymbolQueryRequest, TypeData,
};

use crate::compilation::{Compilation, binder::binding_query_error};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn constant_callable_signature(
        &self,
        callable: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<bray_symbols::CallableSignature>>, FactQueryError> {
        let binding = self.binding_context(cancellation)?;
        let mut diagnostics = DiagnosticBag::new();

        let signature =
            self.resolve_callable_signature_types(&binding, callable, &mut diagnostics)?;

        let signature = if diagnostics.has_errors() {
            None
        } else {
            signature
        };

        Ok(DiagnosticResult::new(signature, diagnostics))
    }

    pub(in crate::compilation) fn constant_call_contract(
        &self,
        target: &ConstantCallTarget,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<ConstantCallContract>>, FactQueryError> {
        let values = self.semantic_value_store()?;
        let binding = self.binding_context(cancellation)?;
        let mut diagnostics = DiagnosticBag::new();
        let context = crate::compilation::checker::CompilationCheckerContext::new(binding);

        let mut normalizer =
            bray_checker::TypeNormalizer::new(&context, Some(target.substitution()));

        let contract = match target {
            ConstantCallTarget::Declaration(callable) => {
                let checked = self.constant_callable_signature(*callable, cancellation)?;

                let predicates = binding
                    .resolve_symbol_query(
                        SymbolQueryRequest::<CallablePredicateContractsQuery>::new(
                            callable.definition().callable_symbol(),
                        ),
                    )
                    .map_err(binding_query_error)?;

                diagnostics =
                    DiagnosticBag::merged_all([checked.diagnostics(), predicates.diagnostics()]);

                if diagnostics.has_errors() {
                    None
                } else {
                    let signature = checked
                        .value()
                        .as_ref()
                        .expect("checked declaration retains its constant signature");

                    Some(ConstantCallContract::new(
                        signature.input_types(values)?,
                        signature.result(),
                        predicates
                            .value()
                            .iter()
                            .filter(|clause| clause.kind() == CallableContractClauseKind::Requires)
                            .filter_map(|clause| clause.predicate()),
                    ))
                }
            }
            ConstantCallTarget::Anonymous(key, Some(substitution)) => {
                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

                diagnostics.add_range(declared.result().diagnostics().iter().cloned());

                if diagnostics.has_errors() {
                    None
                } else {
                    let template = declared
                        .result()
                        .value()
                        .callable_type()
                        .expect("anonymous body retains its checked callable type");

                    let ty = super::super::template::resolved_input_type(self, template)?;
                    let ty = values.substitute_type(ty, *substitution)?;
                    let ty = normalizer.normalize_type(ty)?;
                    let data = values.type_data(ty);

                    let TypeData::Callable(callable) = data.as_ref() else {
                        panic!("anonymous constant target retains a callable type");
                    };

                    Some(ConstantCallContract::new(
                        callable.parameters().iter().map(|parameter| parameter.ty()),
                        callable.result(),
                        callable
                            .phase_behaviors()
                            .invocation()
                            .predicate_requirements()
                            .iter()
                            .copied(),
                    ))
                }
            }
            ConstantCallTarget::Anonymous(_, None) => panic!(
                "constant contract projection requires the selected anonymous generic context"
            ),
            ConstantCallTarget::Default(provider, substitution) => {
                let checked = self.checked_default_template(*provider, cancellation)?;

                diagnostics.add_range(checked.diagnostics().iter().cloned());

                if diagnostics.has_errors() {
                    None
                } else {
                    let template = checked
                        .value()
                        .as_ref()
                        .expect("checked default retains its expression template");

                    let result = values.substitute_type(
                        template.nodes()[usize::try_from(template.result().raw())
                            .expect("checked result index fits host")]
                        .ty(),
                        *substitution,
                    )?;

                    let result = normalizer.normalize_type(result)?;

                    let inputs =
                        if let bray_bound_tree::DefaultValueProvider::CallableParameter(_) =
                            provider
                        {
                            let subject = binding
                                .runtime_default_subject(provider.symbol())
                                .map_err(binding_query_error)?
                                .expect("default retains its declaration subject");

                            let bray_symbols::AnySymbolId::CallableParameter(parameter) = subject
                            else {
                                panic!("callable default retains its parameter subject");
                            };

                            let parameter = binding
                                .callable_parameter(parameter)
                                .map_err(binding_query_error)?
                                .expect("default parameter retains its declaration");

                            let callable = bray_symbols::CallableInstanceData::new(
                                bray_symbols::CallableDefinitionId::try_new(
                                    parameter.owner().into_any(),
                                )
                                .expect("parameter owner is callable"),
                                *substitution,
                            );

                            let signature =
                                self.constant_callable_signature(callable, cancellation)?;

                            diagnostics.add_range(signature.diagnostics().iter().cloned());

                            if diagnostics.has_errors() {
                                return Ok(DiagnosticResult::new(None, diagnostics));
                            }

                            let signature = signature
                                .value()
                                .as_ref()
                                .expect("checked default owner retains its signature");

                            signature
                                .input_types(values)?
                                .into_iter()
                                .take(
                                    usize::try_from(parameter.ordinal())
                                        .expect("parameter ordinal fits host")
                                        + usize::from(signature.receiver().is_some()),
                                )
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };

                    Some(ConstantCallContract::new(inputs, result, []))
                }
            }
            ConstantCallTarget::Predicate(predicate) => {
                let signature = binding
                    .resolve_symbol_query(
                        SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                            predicate.definition(),
                        ),
                    )
                    .map_err(binding_query_error)?;

                diagnostics.add_range(signature.diagnostics().iter().cloned());

                if diagnostics.has_errors() {
                    None
                } else {
                    let inputs = signature
                        .value()
                        .parameters()
                        .iter()
                        .map(|parameter| {
                            let ty =
                                super::super::template::resolved_input_type(self, parameter.ty())?;

                            let ty = values.substitute_type(ty, predicate.substitution())?;

                            normalizer.normalize_type(ty).map_err(FactQueryError::from)
                        })
                        .collect::<Result<Vec<_>, _>>()?;

                    let context = self.checker_context(cancellation)?;

                    let result = bray_checker::representation_type_for_context(
                        &context,
                        bray_compiler_known::RepresentationRole::ScalarBool,
                    );

                    Some(ConstantCallContract::new(inputs, result, []))
                }
            }
        };

        diagnostics.extend(normalizer.take_diagnostics());

        let contract = if diagnostics.has_errors() {
            None
        } else {
            contract
        };

        Ok(DiagnosticResult::new(contract, diagnostics))
    }

    pub(in crate::compilation) fn constant_requirement_origin(
        &self,
        target: &ConstantCallTarget,
        index: usize,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_source::SourceSpan>, FactQueryError> {
        let anchor = match target {
            ConstantCallTarget::Declaration(callable) => {
                let binding = self.binding_context(cancellation)?;

                if binding
                    .imported_semantic_address(callable.definition().symbol())
                    .map_err(binding_query_error)?
                    .is_some()
                {
                    return Ok(None);
                }

                let predicates = binding
                    .resolve_symbol_query(
                        SymbolQueryRequest::<CallablePredicateContractsQuery>::new(
                            callable.definition().callable_symbol(),
                        ),
                    )
                    .map_err(binding_query_error)?;

                let clause = predicates
                    .value()
                    .iter()
                    .filter(|clause| clause.kind() == CallableContractClauseKind::Requires)
                    .nth(index)
                    .expect("failing requirement retains its declaration identity");

                let template = binding
                    .resolve_symbol_query(SymbolQueryRequest::<
                        bray_symbols::CallableContractTemplateQuery,
                    >::new(
                        callable.definition().callable_symbol()
                    ))
                    .map_err(binding_query_error)?;

                let bray_symbols::CallableContractTemplate::Source(template) = template.value()
                else {
                    panic!("source constant requirement retains its declaration template");
                };

                template
                    .expressions()
                    .iter()
                    .find(|expression| expression.ordinal() == clause.ordinal())
                    .expect("failing source requirement retains its expression")
                    .expression()
                    .syntax()
            }
            ConstantCallTarget::Anonymous(key, _) => {
                let declaration = self.execution_declaration(key.source().syntax())?;

                *declaration
                    .value()
                    .requirements()
                    .get(index)
                    .expect("anonymous requirement retains its source clause")
            }
            ConstantCallTarget::Default(_, _) | ConstantCallTarget::Predicate(_) => {
                panic!("declaration expressions have no independent callable requirements")
            }
        };

        Ok(Some(bray_source::SourceSpan::new(
            anchor.source_id(),
            anchor.full_range(),
        )))
    }
}
