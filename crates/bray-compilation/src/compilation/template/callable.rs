use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{
    BoundReferenceTarget, BoundUnitKey, BoundUnitRoot, CheckedTemplate, CheckedTemplateInputKind,
    CheckedTemplateKind,
};
use bray_symbols::{CallableDefinitionId, CallableSignatureQuery, SymbolQueryRequest};

use crate::compilation::{Compilation, binder::binding_query_error};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn constant_predicate_template(
        &self,
        predicate: bray_symbols::PredicateDefinitionSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<bray_diagnostics::DiagnosticResult<Option<CheckedTemplate>>, FactQueryError> {
        if let Some(key) = self.predicate_definition_key(predicate)? {
            let template = self.constant_body_template(key, cancellation)?;

            return Ok(template.result().as_ref().clone());
        }

        let context = self.binding_context(cancellation)?;

        let address = context
            .imported_semantic_address(predicate.into_any())
            .map_err(binding_query_error)?
            .expect("selected imported predicate retains its semantic address");

        let template = crate::compilation::binder::imported_declaration_template(
            &context,
            address,
            CheckedTemplateKind::PredicateDefinition,
        )
        .map_err(binding_query_error)?;

        Ok(template.map(|template| template.map(|template| template.template().clone())))
    }

    pub(in crate::compilation) fn constant_body_template(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<Option<CheckedTemplate>>>, FactQueryError> {
        self.unit_query(
            &self.state.constant_body_templates,
            CompilationFactKey::ConstantBodyTemplate(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                if key.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault {
                    let inputs = self.default_template_inputs(&key, cancellation)?;
                    let context = self.binding_context(cancellation)?;

                    let checked = crate::compilation::binder::checked_source_expression(
                        &context,
                        key.clone(),
                    )
                    .map_err(binding_query_error)?;

                    let template = self.checked_source_template(
                        key.clone(),
                        CheckedTemplateKind::RuntimeDefault,
                        key.source().syntax(),
                        inputs,
                        checked.dependency_contract,
                        cancellation,
                    )?;

                    return Ok((template, Box::new([])));
                }

                let certification =
                    self.certified_execution_with_cancellation(key.clone(), cancellation)?;

                let eligibility = certification.result().diagnostics();

                if eligibility.has_errors() {
                    return Ok((
                        bray_diagnostics::DiagnosticResult::new(None, eligibility.clone()),
                        Box::new([]),
                    ));
                }

                if !self.unit_requires_constant_evaluation(&key, cancellation)? {
                    let mut diagnostics = eligibility.clone();

                    for obligation in [
                        bray_checker::ExecutionObligation::Observation,
                        bray_checker::ExecutionObligation::Termination,
                    ] {
                        let (failure, _, _) = self.certify_execution_obligation(
                            &key,
                            obligation,
                            &mut diagnostics,
                            false,
                            cancellation,
                        )?;

                        if failure.is_some() || diagnostics.has_errors() {
                            return Ok((
                                bray_diagnostics::DiagnosticResult::new(None, diagnostics),
                                Box::new([]),
                            ));
                        }
                    }
                }

                let inputs = self.constant_body_inputs(&key, cancellation)?;

                let (kind, dependency) = if key.kind()
                    == bray_bound_tree::BoundUnitKind::PredicateDefinition
                {
                    let owner = self
                        .symbol_graph()?
                        .symbol_for_key(key.declared_owner())
                        .expect("predicate unit retains its owner");

                    let predicate = bray_symbols::PredicateDefinitionSymbolId::try_from_any(owner)
                        .expect("predicate unit has a predicate declaration");

                    let definition =
                        self.predicate_definition_with_cancellation(predicate, cancellation)?;

                    if definition.diagnostics().has_errors() {
                        return Ok((
                            bray_diagnostics::DiagnosticResult::new(
                                None,
                                definition.diagnostics().clone(),
                            ),
                            Box::new([]),
                        ));
                    }

                    let bray_symbols::PredicateDefinitionState::Defined(definition) =
                        definition.value()
                    else {
                        panic!("constant predicate invocation retains its checked definition");
                    };

                    (
                        CheckedTemplateKind::PredicateDefinition,
                        definition.semantic().dependency_contract(),
                    )
                } else {
                    (
                        CheckedTemplateKind::ConstantCallableBody,
                        self.semantic_value_store()?
                            .empty_dependency_contract_template()?,
                    )
                };

                let template = self.checked_source_template(
                    key.clone(),
                    kind,
                    key.source().syntax(),
                    inputs,
                    dependency,
                    cancellation,
                )?;

                Ok((template, Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn constant_body_inputs(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Vec<super::SourceTemplateInput>, FactQueryError> {
        let symbols = self.symbol_graph()?;

        let owner = symbols
            .symbol_for_key(key.declared_owner())
            .expect("const body retains its lexical owner");

        let parameters = crate::compilation::binder::visible_generic_parameters(symbols, owner);

        if key.kind() == bray_bound_tree::BoundUnitKind::PredicateDefinition {
            let predicate = bray_symbols::PredicateDefinitionSymbolId::try_from_any(owner)
                .expect("predicate body retains its declaration");

            let context = self.binding_context(cancellation)?;

            let signature = context
                .resolve_symbol_query(SymbolQueryRequest::<
                    bray_symbols::PredicateSignatureTemplateQuery,
                >::new(predicate))
                .map_err(binding_query_error)?;

            return super::predicate_template_inputs(self, signature.value(), parameters);
        }

        if key.kind() != bray_bound_tree::BoundUnitKind::AnonymousCallable {
            let definition = CallableDefinitionId::try_new(owner)
                .expect("declared const body retains its callable identity");

            let context = self.binding_context(cancellation)?;

            let signature = context
                .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                    definition.callable_symbol(),
                ))
                .map_err(binding_query_error)?;

            return super::callable_template_inputs(self, signature.value(), parameters);
        }

        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let declared =
            self.declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

        let template = declared
            .result()
            .value()
            .callable_type()
            .expect("anonymous const body retains its callable type");

        let ty = super::resolved_input_type(self, template)?;
        let data = self.semantic_value_store()?.type_data(ty);

        let bray_symbols::TypeData::Callable(callable_type) = data.as_ref() else {
            panic!("anonymous const body has a callable type");
        };

        let BoundUnitRoot::AnonymousCallable { callable, .. } = bound.result().value().root()
        else {
            panic!("anonymous const body retains its callable root");
        };

        let callable = bound
            .result()
            .value()
            .local_symbols()
            .anonymous_callable(callable)
            .expect("anonymous const body retains its parameters");

        let mut inputs = callable
            .parameters()
            .iter()
            .zip(callable_type.parameters())
            .enumerate()
            .map(|(ordinal, (parameter, ty))| {
                super::SourceTemplateInput::new(
                    CheckedTemplateInputKind::Parameter(bray_symbols::SymbolOrdinal::new(
                        u32::try_from(ordinal).expect("parameter ordinal fits u32"),
                    )),
                    Some(BoundReferenceTarget::Local((*parameter).into())),
                    ty.ty(),
                )
            })
            .collect::<Vec<_>>();

        inputs.extend(super::generic_template_inputs(self, parameters)?);

        Ok(inputs)
    }
}
