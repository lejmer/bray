use bray_binder::SymbolQueryProvider;
use bray_checker::ExecutionCondition;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn trusted_required_witnesses(
        &self,
        requirements: &[ExecutionCondition],
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<Vec<(ExecutionCondition, bray_checker::ExecutionPlace)>>,
        FactQueryError,
    > {
        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;
        let symbols = self.available_compiler_known_symbols();
        let mut pending = requirements.iter().collect::<Vec<_>>();
        let mut witnesses = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        while let Some(condition) = pending.pop() {
            let ExecutionCondition::Trusted(condition) = condition else {
                if let ExecutionCondition::Operation(_, operands) = condition {
                    pending.extend(operands.iter());
                }

                continue;
            };

            let ExecutionCondition::Predicate(predicate, substitution, arguments) =
                condition.as_ref()
            else {
                continue;
            };

            let signature = context
                .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                    bray_symbols::PredicateSignatureTemplateQuery,
                >::new(*predicate))
                .map_err(crate::compilation::binder::binding_query_error)?;

            diagnostics.add_range(signature.diagnostics().iter().cloned());

            if signature.diagnostics().has_errors() || !signature.value().is_trusted() {
                continue;
            }

            let templates = signature
                .value()
                .parameters()
                .iter()
                .map(|parameter| parameter.ty().clone())
                .collect::<Vec<_>>();

            let checked = self.checked_constant_terms_for_templates_with_cancellation(
                templates.iter(),
                cancellation,
            )?;

            diagnostics.add_range(checked.diagnostics().iter().cloned());

            if checked.diagnostics().has_errors() {
                continue;
            }

            assert_eq!(
                templates.len(),
                arguments.len(),
                "checked predicate retains its signature arity"
            );

            for (template, argument) in templates.iter().zip(arguments.iter()) {
                let ty = bray_checker::resolve_type_expression_template(
                    values,
                    template,
                    checked.value(),
                )
                .expect(
                    "checked predicate parameter template must resolve before trusted analysis",
                );

                let ty = values.substitute_type(ty, *substitution);

                let owns_authority = match values.type_data(ty?).as_ref() {
                    bray_symbols::TypeData::Named { definition, .. } => {
                        let role = match definition {
                            bray_symbols::NamedTypeSymbolId::Struct(symbol) => {
                                symbols.symbol_representation(*symbol)
                            }
                            bray_symbols::NamedTypeSymbolId::Union(symbol) => {
                                symbols.symbol_representation(*symbol)
                            }
                        };

                        !role.is_some_and(|role| {
                            role.numeric_kind().is_some()
                                || matches!(
                                    role,
                                    bray_compiler_known::RepresentationRole::ScalarBool
                                        | bray_compiler_known::RepresentationRole::ScalarChar
                                        | bray_compiler_known::RepresentationRole::Unit
                                        | bray_compiler_known::RepresentationRole::Never
                                )
                        }) && self.trusted_result_is_witness(
                            ty?,
                            std::slice::from_ref(&ExecutionCondition::Trusted(condition.clone())),
                            cancellation,
                            &mut diagnostics,
                        )?
                    }
                    _ => true,
                };

                if owns_authority {
                    witnesses.extend(argument.inputs().into_iter().map(|place| {
                        (
                            ExecutionCondition::Trusted(condition.clone()),
                            place.clone(),
                        )
                    }));
                }
            }
        }

        Ok(DiagnosticResult::new(witnesses, diagnostics))
    }

    pub(super) fn trusted_predicate_subjects(
        &self,
        conditions: &[ExecutionCondition],
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Vec<(ExecutionCondition, ExecutionCondition)>, FactQueryError> {
        let context = self.binding_context(cancellation)?;

        let mut pending = conditions
            .iter()
            .map(|condition| (condition, condition))
            .collect::<Vec<_>>();

        let mut subjects = Vec::new();

        while let Some((guarantee, condition)) = pending.pop() {
            match condition {
                ExecutionCondition::Trusted(condition) => {
                    let ExecutionCondition::Predicate(predicate, _, arguments) = condition.as_ref()
                    else {
                        continue;
                    };

                    let signature = context
                        .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                            bray_symbols::PredicateSignatureTemplateQuery,
                        >::new(*predicate))
                        .map_err(crate::compilation::binder::binding_query_error)?;

                    diagnostics.add_range(signature.diagnostics().iter().cloned());

                    if !signature.diagnostics().has_errors() && signature.value().is_trusted() {
                        subjects.extend(
                            arguments
                                .iter()
                                .map(|argument| (guarantee.clone(), argument.clone())),
                        );
                    }
                }
                ExecutionCondition::Operation(_, operands) => {
                    pending.extend(operands.iter().map(|operand| (guarantee, operand)))
                }
                _ => {}
            }
        }

        Ok(subjects)
    }

    pub(super) fn trusted_result_is_witness(
        &self,
        ty: bray_symbols::TypeId,
        guarantees: &[ExecutionCondition],
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        let mut result_guarantees = guarantees
            .iter()
            .filter(|condition| condition.observes_completion_result())
            .peekable();

        if result_guarantees.peek().is_none() {
            return Ok(false);
        }

        let values = self.semantic_value_store()?;

        if !matches!(values.type_data(ty).as_ref(), bray_symbols::TypeData::Named { definition, .. }
            if crate::compilation::foreign::compiler_known_representation(self, *definition).is_some_and(|role|
                role.numeric_kind().is_some()
                    || matches!(role, bray_compiler_known::RepresentationRole::RawPointer
                        | bray_compiler_known::RepresentationRole::DevicePointer)))
        {
            return Ok(true);
        }

        let provider = self.available_compiler_known_symbols().provider();

        let storage_predicates = [
            "ValidRead",
            "ValidWrite",
            "AlignedFor",
            "InitializedAs",
            "InitializedRangeAs",
            "NulTerminatedRead",
            "WideNulTerminatedRead",
            "NonOverlapping",
            "SharedAliasValid",
            "ExclusiveAliasValid",
            "EpochCurrent",
            "SynchronizedAccess",
            "MovementStable",
            "FinalizationPending",
            "OwnedAllocation",
            "CallableAddressValid",
            "DeviceValidRead",
            "DeviceValidWrite",
            "DeviceAlignedFor",
            "DeviceInitializedAs",
        ]
        .into_iter()
        .filter_map(|name| {
            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                .expect("closed memory predicate key must be valid");

            provider
                .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                .map(bray_symbols::PredicateDefinitionSymbolId::from)
        })
        .collect::<std::collections::BTreeSet<_>>();

        // Addresses and range extents describe checked storage without owning its authority.
        // Their guarantees retain source dependencies. Custom predicates still create witnesses.
        let context = self.binding_context(cancellation)?;
        let mut pending = result_guarantees.collect::<Vec<_>>();

        while let Some(condition) = pending.pop() {
            match condition {
                ExecutionCondition::Trusted(condition) => {
                    if !condition.observes_completion_result() {
                        continue;
                    }

                    let ExecutionCondition::Predicate(predicate, _, _) = condition.as_ref() else {
                        continue;
                    };

                    if storage_predicates.contains(predicate) {
                        continue;
                    }

                    let signature = context
                        .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                            bray_symbols::PredicateSignatureTemplateQuery,
                        >::new(*predicate))
                        .map_err(crate::compilation::binder::binding_query_error)?;

                    diagnostics.add_range(signature.diagnostics().iter().cloned());

                    if signature.diagnostics().has_errors() || signature.value().is_trusted() {
                        return Ok(true);
                    }
                }
                ExecutionCondition::Operation(_, operands) => pending.extend(operands.iter()),
                ExecutionCondition::Unknown => return Ok(true),
                _ => {}
            }
        }

        Ok(false)
    }
}
