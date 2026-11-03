use super::super::super::ConstantReferenceResolution;
use super::super::super::call::{ConstantCallRequest, ConstantCallResolution};
use super::super::super::diagnostic::{ConstantDiagnostic, ConstantLimitKind};
use super::super::support::{TemplateEvaluationFailure, constant_definition, recovery_value};
use crate::CheckerRequestContext;
use bray_bound_tree::CheckedTemplateNodeId;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ConstantInstanceKey, ConstantValueId,
    ConstantValueKind, GenericSubstitutionId, ImplementationInstanceData, ImplementationSymbolId,
    TypeId,
};

use super::control::TemplateEvaluator;

impl<C> TemplateEvaluator<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_declaration(
        &mut self,
        declaration: &bray_symbols::SymbolKey,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let Some(symbol) = self.resolver.symbol(declaration) else {
            panic!("constant declaration {declaration:?} must resolve to a symbol");
        };

        if let AnySymbolId::UnionVariant(variant) = symbol {
            return Ok(self.intern_value(ty, ConstantValueKind::union(variant, [])));
        }

        if CallableDefinitionId::try_new(symbol).is_some()
            || matches!(symbol, AnySymbolId::Static(_))
        {
            return Err(TemplateEvaluationFailure::invalid_expression(
                bray_diagnostics::DiagnosticExpressionCategory::NameReference,
            ));
        }

        let Some(definition) = constant_definition(symbol) else {
            panic!("constant symbol {symbol:?} must have a constant definition");
        };

        let instance = ConstantInstanceKey::new(
            definition,
            self.substitution
                .expect("declaration nodes retain their generic environment"),
            self.selected_implementation,
        );

        let result = self
            .resolver
            .resolve_constant(instance, self.budget.remaining_limits(self.limits))
            .map_err(|error| self.record_query_failure(error))?;

        self.diagnostics = self.diagnostics.merged(result.diagnostics());

        let value = match result.value() {
            ConstantReferenceResolution::Value(value) => *value,
            ConstantReferenceResolution::Evaluated(result) => {
                self.budget
                    .try_charge_usage(result.usage())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                result.value()
            }
            ConstantReferenceResolution::Term(term) => self.evaluate_term(*term, ty)?,
            ConstantReferenceResolution::Cycle { definition } => {
                return Err(TemplateEvaluationFailure::Diagnostic(
                    ConstantDiagnostic::Cycle {
                        definition: *definition,
                    },
                ));
            }
            ConstantReferenceResolution::Invalid => {
                panic!(
                    "constant declaration {declaration:?} must have a valid reference resolution"
                );
            }
        };

        let data = self.context.semantic_values().constant_value_data(value);

        if data.ty() != ty {
            panic!(
                "constant declaration {declaration:?} must produce type {ty:?}, actual: {data:?}"
            );
        }

        Ok(value)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "template calls retain their callable identity, substitution and staged argument ordinals"
    )]
    pub(super) fn evaluate_call(
        &mut self,
        owner: CheckedTemplateNodeId,
        state_inputs: Option<&[u32]>,
        callable: &bray_symbols::SymbolKey,
        substitution: GenericSubstitutionId,
        arguments: &[CheckedTemplateNodeId],
        ordinals: &[u32],
        dispatch: Option<&bray_bound_tree::CheckedTemplateDispatch>,
        saved_witnesses: &[(bray_symbols::SymbolKey, GenericSubstitutionId)],
        result_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let evaluated = self.evaluate_nodes(arguments)?;
        let mut ordered = ordinals.iter().copied().zip(evaluated).collect::<Vec<_>>();

        ordered.sort_by_key(|(ordinal, _)| *ordinal);

        let arguments = ordered
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>();

        let symbol = self
            .resolver
            .symbol(callable)
            .unwrap_or_else(|| panic!("constant invocation retains its declaration: {callable:?}"));

        let values = self.context.semantic_values();

        let substitution = self.substitute_arguments(substitution);

        let implementation = match dispatch {
            None => None,
            Some(bray_bound_tree::CheckedTemplateDispatch::Implementation(
                declaration,
                substitution,
            )) => Some((declaration, substitution)),
            Some(
                bray_bound_tree::CheckedTemplateDispatch::Constraint { .. }
                | bray_bound_tree::CheckedTemplateDispatch::TraitDefault { .. },
            ) => None,
        };

        let implementation = implementation.map(|(declaration, substitution)| {
            self.instantiate_implementation(declaration, *substitution)
        });

        let witnesses = saved_witnesses
            .iter()
            .map(|(declaration, substitution)| {
                self.instantiate_implementation(declaration, *substitution)
            })
            .chain(self.implementation_witnesses.iter().copied())
            .collect::<Vec<_>>();

        let limits = self.budget.remaining_limits(self.limits);

        let Some(limits) = limits.nested_call() else {
            return Err(TemplateEvaluationFailure::Diagnostic(
                ConstantDiagnostic::limit(ConstantLimitKind::EvaluationSteps, 1, 0),
            ));
        };

        let requirement = self.instantiate_dispatch_requirement(dispatch)?;

        if let Some(provider) = bray_symbols::DefaultValueProvider::try_from_symbol(symbol) {
            let implementation = match (implementation, requirement) {
                (Some(witness), _) => Some(witness),
                (None, Some(requirement)) => {
                    let selection = self
                        .context
                        .implementation_selection(requirement)
                        .map_err(|error| self.record_query_failure(error))?;

                    self.diagnostics
                        .extend(selection.diagnostics().iter().cloned());

                    if self.diagnostics.has_errors() {
                        return Err(TemplateEvaluationFailure::Diagnosed);
                    }

                    let bray_symbols::ImplementationSelection::Selected(witness) =
                        selection.value()
                    else {
                        panic!(
                            "admitted default retains its concrete dispatch witness: {requirement:?}"
                        );
                    };

                    Some(*witness)
                }
                (None, None) => None,
            };

            let request = ConstantCallRequest::new(
                crate::ConstantCallTarget::Default(provider, substitution),
                implementation,
                arguments,
                result_type,
                limits,
            )
            .with_implementation_witnesses(witnesses);

            return self.resolve_call(&request, result_type);
        }

        let callable = CallableDefinitionId::try_new(symbol)
            .expect("checked non-default constant invocation retains its callable");

        let mut callable = CallableInstanceData::new(callable, substitution);
        let mut implementation = implementation;

        if let Some(requirement) = requirement {
            let abstract_callable = values
                .intern_callable_instance(callable)
                .expect("checked abstract invocation retains valid arguments");

            let selected = self
                .context
                .witness_callable(abstract_callable, requirement)
                .map_err(|error| self.record_query_failure(error))?
                .expect("admitted concrete call retains its selected implementation");

            callable = selected.0;
            implementation = Some(selected.2);
        }

        let request = ConstantCallRequest::new(
            crate::ConstantCallTarget::Declaration(callable),
            implementation,
            arguments,
            result_type,
            limits,
        );

        let request = request
            .with_implementation_witnesses(witnesses)
            .with_observation(state_inputs.is_some());

        self.resolve_call_outputs(
            &request,
            result_type,
            Some((owner, state_inputs.unwrap_or(&[]))),
        )
    }

    fn instantiate_dispatch_requirement(
        &mut self,
        dispatch: Option<&bray_bound_tree::CheckedTemplateDispatch>,
    ) -> Result<Option<bray_symbols::ImplementationRequirementKey>, TemplateEvaluationFailure> {
        let route = dispatch
            .map(|dispatch| {
                dispatch.trait_dispatch::<bray_symbols::SemanticValueStoreError>(
                    self.context.semantic_values(),
                    |key| {
                        Ok(self
                            .resolver
                            .symbol(key)
                            .expect("checked dispatch owner resolves"))
                    },
                )
            })
            .transpose()
            .expect("checked dispatch retains valid semantic values")
            .flatten();

        let Some(route) = route else {
            return Ok(None);
        };

        let requirement = self
            .context
            .dispatch_requirement(route)
            .map_err(|error| self.record_query_failure(error))?;

        let subject = self.instantiate_type(requirement.subject())?;
        let values = self.context.semantic_values();
        let application = values.trait_application_data(requirement.trait_application());
        let substitution = self.substitute_arguments(application.substitution());

        let application = values
            .intern_trait_application(bray_symbols::TraitApplicationData::new(
                application.definition(),
                substitution,
            ))
            .expect("checked trait arguments retain their owner");

        Ok(Some(bray_symbols::ImplementationRequirementKey::new(
            subject,
            application,
        )))
    }

    fn instantiate_implementation(
        &self,
        declaration: &bray_symbols::SymbolKey,
        substitution: GenericSubstitutionId,
    ) -> bray_symbols::ImplementationInstanceId {
        let definition = self
            .resolver
            .symbol(declaration)
            .and_then(ImplementationSymbolId::try_from_any)
            .unwrap_or_else(|| panic!("checked implementation resolves: {declaration:?}"));

        let substitution = self.substitute_arguments(substitution);

        self.context
            .semantic_values()
            .intern_implementation_instance(ImplementationInstanceData::new(
                definition,
                substitution,
            ))
            .expect("checked implementation retains valid arguments")
    }

    pub(in crate::constant::template_evaluation) fn resolve_call(
        &mut self,
        request: &ConstantCallRequest,
        result_type: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        self.resolve_call_outputs(request, result_type, None)
    }

    pub(super) fn resolve_call_outputs(
        &mut self,
        request: &ConstantCallRequest,
        result_type: TypeId,
        outputs: Option<(CheckedTemplateNodeId, &[u32])>,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let resolution = self
            .resolver
            .resolve(request)
            .map_err(|error| self.record_query_failure(error))?;

        match resolution {
            ConstantCallResolution::Evaluated(result) => {
                self.budget
                    .try_charge_usage(result.value().usage())
                    .map_err(TemplateEvaluationFailure::Diagnostic)?;

                self.diagnostics = self.diagnostics.merged(result.diagnostics());

                if let Some((owner, ordinals)) = outputs {
                    let mut states = vec![None; request.arguments().len()];

                    for ordinal in ordinals {
                        states[usize::try_from(*ordinal).expect("checked state ordinal fits host")] =
                            Some(result.value().states().iter().find_map(|(input, state)|
                                (input == ordinal).then_some(*state))
                                .expect("checked observational call retains every mutable formal result"));
                    }

                    self.outputs.insert(owner, states);
                }

                Ok(result.value().value())
            }
            ConstantCallResolution::Cycle => Err(TemplateEvaluationFailure::Diagnostic(
                ConstantDiagnostic::Cycle { definition: None },
            )),
            ConstantCallResolution::Ineligible(diagnostics) => {
                if diagnostics.has_errors() {
                    self.diagnostics = self.diagnostics.merged(&diagnostics);

                    return Ok(recovery_value(self.context.semantic_values(), result_type));
                }

                // A valid const callable can receive a static address whose contents are
                // unavailable during evaluation. The declaration alone does not supply that data.
                Err(TemplateEvaluationFailure::invalid_expression(
                    bray_diagnostics::DiagnosticExpressionCategory::Call,
                ))
            }
        }
    }
}
