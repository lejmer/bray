use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundUnitKey, CheckedTemplate, CheckedTemplateInputKind, CheckedTemplateKind,
};
use bray_checker::ConstantCallTarget;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableContractTemplate, CallableContractTemplateQuery, CallableContractsQuery,
    CallableSignatureQuery, GenericDeclarationTemplateQuery, GenericOwnerId, SymbolOrdinal,
    SymbolQueryRequest,
};

use super::{SourceTemplateInput, callable_template_inputs, generic_template_inputs};
use crate::compilation::{
    Compilation,
    binder::{binding_query_error, imported_declaration_template_at},
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    /// Produces the same checked expression for native checks and closed constant invocation.
    pub(in crate::compilation) fn declared_predicate_template(
        &self,
        callable: bray_symbols::CallableInstanceData,
        ordinal: SymbolOrdinal,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<CheckedTemplate>>, FactQueryError> {
        let binding = self.binding_context(cancellation)?;
        let owner = callable.definition().symbol();

        if let Some(address) = binding
            .imported_semantic_address(owner)
            .map_err(binding_query_error)?
        {
            return imported_declaration_template_at(
                &binding,
                address,
                CheckedTemplateKind::CallableContract,
                ordinal,
            )
            .map(|template| {
                template.map(|template| template.map(|template| template.template().clone()))
            })
            .map_err(binding_query_error);
        }

        let source = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractTemplateQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        let clauses = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        let signature = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        let diagnostics = DiagnosticBag::merged_all([
            source.diagnostics(),
            clauses.diagnostics(),
            signature.diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(None, diagnostics));
        }

        let CallableContractTemplate::Source(source) = source.value() else {
            panic!("local requirement retains its source contract");
        };

        let expression = source
            .expressions()
            .iter()
            .find(|expression| expression.ordinal() == ordinal)
            .expect("selected requirement retains its source expression");

        let predicate = clauses
            .value()
            .invocation_preconditions()
            .iter()
            .find(|clause| clause.ordinal() == ordinal)
            .and_then(|clause| clause.predicate())
            .expect("selected requirement retains its checked meaning");

        let generic = if let Some(owner) = GenericOwnerId::try_new(owner) {
            let generic = binding
                .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                    owner,
                ))
                .map_err(binding_query_error)?;

            if generic.diagnostics().has_errors() {
                return Ok(DiagnosticResult::new(None, generic.diagnostics().clone()));
            }

            generic.value().parameters().to_vec()
        } else {
            Vec::new()
        };

        let owner = binding
            .symbol_key(owner)
            .map_err(binding_query_error)?
            .expect("source requirement owner retains its stable key")
            .clone();

        let key =
            BoundUnitKey::contract_clause(owner, self.bound_source(expression.unit_syntax())?)
                .expect("callable owns its selected contract clause");

        self.checked_source_template(
            key,
            CheckedTemplateKind::CallableContract,
            expression.expression().syntax(),
            callable_template_inputs(self, signature.value(), generic)?,
            predicate.dependency_contract(),
            cancellation,
        )
    }

    pub(in crate::compilation) fn constant_requirement_template(
        &self,
        target: &ConstantCallTarget,
        index: usize,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<CheckedTemplate>>, FactQueryError> {
        let binding = self.binding_context(cancellation)?;

        match target {
            ConstantCallTarget::Declaration(callable) => {
                let clauses = binding
                    .resolve_symbol_query(SymbolQueryRequest::<
                        bray_symbols::CallablePredicateContractsQuery,
                    >::new(
                        callable.definition().callable_symbol()
                    ))
                    .map_err(binding_query_error)?;

                if clauses.diagnostics().has_errors() {
                    return Ok(DiagnosticResult::new(None, clauses.diagnostics().clone()));
                }

                let clause = clauses
                    .value()
                    .iter()
                    .filter(|clause| {
                        clause.kind() == bray_symbols::CallableContractClauseKind::Requires
                    })
                    .nth(index)
                    .expect("constant invocation retains its selected requirement");

                self.declared_predicate_template(*callable, clause.ordinal(), cancellation)
            }
            ConstantCallTarget::Anonymous(owner, _) => {
                let syntax = self.execution_declaration_node(owner.source().syntax())?;

                let (clause, expression) =
                    crate::compilation::binder::callable_requirement_source(syntax, index);

                let key = BoundUnitKey::callable_type_clause(
                    owner.declared_owner().clone(),
                    owner.source(),
                    self.bound_source(clause)?,
                );

                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

                if declared.result().diagnostics().has_errors() {
                    return Ok(DiagnosticResult::new(
                        None,
                        declared.result().diagnostics().clone(),
                    ));
                }

                let references = self.execution_unit_inputs(&key, cancellation)?;

                let mut inputs = references
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, reference)| {
                        let template = declared
                            .result()
                            .value()
                            .evidence()
                            .iter()
                            .find(|evidence| {
                                evidence.term()
                                    == bray_bound_tree::DeclaredValueTypeTerm::Value(reference)
                            })
                            .expect("clause formal input retains its established type");

                        Ok(SourceTemplateInput::new(
                            CheckedTemplateInputKind::Parameter(SymbolOrdinal::new(
                                u32::try_from(ordinal)
                                    .expect("clause ordinal fits compiler identity"),
                            )),
                            Some(reference),
                            super::resolved_input_type(self, template.template())?,
                        ))
                    })
                    .collect::<Result<Vec<_>, FactQueryError>>()?;

                let lexical = self
                    .symbol_graph()?
                    .symbol_for_key(owner.declared_owner())
                    .expect("anonymous requirement retains its lexical declaration");

                inputs.extend(generic_template_inputs(
                    self,
                    crate::compilation::binder::visible_generic_parameters(
                        self.symbol_graph()?,
                        lexical,
                    ),
                )?);

                self.checked_source_template(
                    key,
                    CheckedTemplateKind::CallableContract,
                    expression,
                    inputs,
                    self.semantic_value_store()?
                        .empty_dependency_contract_template()?,
                    cancellation,
                )
            }
            ConstantCallTarget::Default(_, _) | ConstantCallTarget::Predicate(_) => {
                panic!("declaration expressions have no independent caller requirements")
            }
        }
    }
}
