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
        DiagnosticResult<Vec<(ExecutionCondition, ExecutionCondition, bool)>>,
        FactQueryError,
    > {
        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;
        let symbols = self.available_compiler_known_symbols();

        let mut pending = requirements
            .iter()
            .flat_map(ExecutionCondition::conjuncts)
            .map(|condition| (condition, condition))
            .collect::<Vec<_>>();

        let mut witnesses = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        while let Some((requirement, condition)) = pending.pop() {
            let bray_checker::ExecutionConditionKind::Trusted(condition) = condition.kind() else {
                if let bray_checker::ExecutionConditionKind::Operation(_, operands) = condition.kind() {
                    pending.extend(operands.iter().map(|condition| (requirement, condition)));
                }

                continue;
            };

            let bray_checker::ExecutionConditionKind::Predicate(predicate, substitution, arguments) =
                (condition.as_ref()).kind()
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
                            std::slice::from_ref(&ExecutionCondition::new(bray_checker::ExecutionConditionKind::Trusted(condition.clone()))),
                            cancellation,
                            &mut diagnostics,
                        )?
                    }
                    _ => true,
                };

                if owns_authority {
                    let containing_value = self.trusted_subject_needs_owner(
                        ty?, cancellation, &mut diagnostics,
                    )?;

                    witnesses.push((requirement.clone(), argument.clone(), containing_value));
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
    ) -> Result<
        (
            Vec<(ExecutionCondition, ExecutionCondition)>,
            Vec<(ExecutionCondition, ExecutionCondition, bool)>,
        ),
        FactQueryError,
    > {
        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;

        let mut pending = conditions
            .iter()
            .flat_map(ExecutionCondition::conjuncts)
            .map(|condition| (condition, condition))
            .collect::<Vec<_>>();

        let storage_predicates = self.copyable_storage_predicates();

        let mut subjects = Vec::new();
        let mut addresses = Vec::new();

        while let Some((guarantee, condition)) = pending.pop() {
            match condition.kind() {
                bray_checker::ExecutionConditionKind::Trusted(condition) => {
                    let bray_checker::ExecutionConditionKind::Predicate(predicate, substitution, arguments) =
                        (condition.as_ref()).kind()
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
                                values, template, checked.value(),
                            ).expect("checked predicate parameter template resolves before trusted analysis");

                            let ty = values.substitute_type(ty, *substitution)?;

                            if matches!(values.type_data(ty).as_ref(),
                                bray_symbols::TypeData::Named { definition, .. }
                                if matches!(crate::compilation::foreign::compiler_known_representation(self, *definition),
                                    Some(bray_compiler_known::RepresentationRole::RawPointer
                                        | bray_compiler_known::RepresentationRole::FunctionPointer
                                        | bray_compiler_known::RepresentationRole::DevicePointer)))
                            {
                                let owning = !storage_predicates.contains(predicate);

                                addresses.push((guarantee.clone(), argument.clone(), owning));
                            }

                            let subject = if argument.observes_completion_result() {
                                    let subject = argument.witness_carrier(self.trusted_subject_needs_owner(
                                        ty, cancellation, diagnostics,
                                    )?);

                                    // A named container carries its field guarantees. A standalone
                                    // address retains its copyable storage facts, including in tuples.
                                    if subject == argument.witness_carrier(false) && !self.trusted_result_is_witness(
                                        values.unborrowed_type(ty),
                                        std::slice::from_ref(guarantee),
                                        cancellation,
                                        diagnostics,
                                    )? {
                                        continue;
                                    }

                                    subject
                            } else {
                                argument.witness_carrier(self.trusted_subject_needs_owner(
                                    ty, cancellation, diagnostics,
                                )?)
                            };

                            subjects.push((guarantee.clone(), subject));
                        }
                    }
                }
                bray_checker::ExecutionConditionKind::Operation(_, operands) => {
                    pending.extend(operands.iter().map(|operand| (guarantee, operand)))
                }
                _ => {}
            }
        }

        Ok((subjects, addresses))
    }

    fn trusted_subject_needs_owner(
        &self,
        ty: bray_symbols::TypeId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        let values = self.semantic_value_store()?;
        let ty = values.unborrowed_type(ty);
        let data = values.type_data(ty);

        let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(false);
        };

        if let Some(role) =
            crate::compilation::foreign::compiler_known_representation(self, *definition)
        {
            return Ok(role.numeric_kind().is_some() || matches!(role,
                bray_compiler_known::RepresentationRole::ScalarBool
                    | bray_compiler_known::RepresentationRole::ScalarChar
                    | bray_compiler_known::RepresentationRole::RawPointer
                    | bray_compiler_known::RepresentationRole::FunctionPointer
                    | bray_compiler_known::RepresentationRole::DevicePointer));
        }

        let representation =
            self.declared_type_representation_with_cancellation(*definition, cancellation)?;

        diagnostics.add_range(representation.diagnostics().iter().cloned());

        Ok(matches!(representation.value().storage(),
            bray_symbols::DeclaredStorageShape::Structure(fields) if fields.is_empty()))
    }

    fn copyable_storage_predicates(
        &self,
    ) -> std::collections::BTreeSet<bray_symbols::PredicateDefinitionSymbolId> {
        let provider = self.available_compiler_known_symbols().provider();

        [
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
        .collect::<std::collections::BTreeSet<_>>()
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
                        | bray_compiler_known::RepresentationRole::FunctionPointer
                        | bray_compiler_known::RepresentationRole::DevicePointer)))
        {
            return Ok(true);
        }

        let storage_predicates = self.copyable_storage_predicates();

        // Addresses and range extents describe checked storage without owning its authority.
        // Their guarantees retain source dependencies. Custom predicates still create witnesses.
        let context = self.binding_context(cancellation)?;
        let mut pending = result_guarantees.collect::<Vec<_>>();

        while let Some(condition) = pending.pop() {
            match condition.kind() {
                bray_checker::ExecutionConditionKind::Trusted(condition) => {
                    if !condition.observes_completion_result() {
                        continue;
                    }

                    let bray_checker::ExecutionConditionKind::Predicate(predicate, _, _) = (condition.as_ref()).kind() else {
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
                bray_checker::ExecutionConditionKind::Operation(_, operands) => pending.extend(operands.iter()),
                bray_checker::ExecutionConditionKind::Unknown => return Ok(true),
                _ => {}
            }
        }

        Ok(false)
    }
}
