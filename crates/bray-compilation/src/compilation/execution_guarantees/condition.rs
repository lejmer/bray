use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{BoundReferenceTarget, BoundUnitKey};
use bray_checker::ExecutionCondition;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceSpan;

use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

pub(super) type DomainPostcondition = (ExecutionCondition, bray_symbols::PredicateDomain, SourceSpan);

impl Compilation {
    pub(in crate::compilation) fn execution_predicate_inputs(
        &self,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<
        std::collections::BTreeMap<
            bray_symbols::PredicateInstanceData,
            bray_symbols::PredicateSemanticSummary,
        >,
        FactQueryError,
    > {
        let selected = semantics
            .selections()
            .entries()
            .iter()
            .filter_map(|entry| match entry.selection() {
                bray_bound_tree::SemanticSelection::Call(call) => match call.target() {
                    bray_bound_tree::BoundCallableTarget::Predicate(predicate) => Some(predicate),
                    _ => None,
                },
                bray_bound_tree::SemanticSelection::Predicate(predicate) => {
                    Some(bray_symbols::PredicateInstanceData::new(
                        predicate.predicate(),
                        predicate.substitution(),
                    ))
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();

        let mut meanings = std::collections::BTreeMap::new();

        for predicate in selected {
            let definition =
                self.predicate_definition_with_cancellation(predicate.definition(), cancellation)?;

            diagnostics.add_range(definition.diagnostics().iter().cloned());

            if let bray_symbols::PredicateDefinitionState::Defined(definition) = definition.value()
            {
                let values = self.semantic_value_store()?;
                let semantic = definition.semantic();

                let condition = semantic
                    .condition()
                    .map(|condition| {
                        values.substitute_constant_term(condition, predicate.substitution())
                    })
                    .transpose()?;

                let dependency = values.substitute_dependency_contract(
                    semantic.dependency_contract(),
                    predicate.substitution(),
                )?;

                meanings.insert(
                    predicate,
                    bray_symbols::PredicateSemanticSummary::new(dependency)
                        .with_condition(condition, semantic.domain()),
                );
            }
        }

        Ok(meanings)
    }

    pub(in crate::compilation) fn execution_constant_inputs(
        &self,
        bound: &bray_bound_tree::BoundUnit,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<
        std::collections::BTreeMap<bray_checker::ExecutionPlace, ExecutionCondition>,
        FactQueryError,
    > {
        let mut constants = std::collections::BTreeMap::new();
        let mut visited = std::collections::BTreeSet::new();

        for (expression, node) in bound.tree().expressions() {
            let symbol = match node {
                bray_bound_tree::BoundExpression::Name(name) => match name.target() {
                    bray_bound_tree::BoundReferenceTarget::Surface(symbol) => symbol,
                    _ => continue,
                },
                bray_bound_tree::BoundExpression::MemberAccess(_) => {
                    let Some(bray_bound_tree::SemanticSelection::Operation(
                        bray_bound_tree::SelectedOperation::Member(member),
                    )) = semantics.selections().expression(expression)
                    else {
                        continue;
                    };

                    member.member()
                }
                _ => continue,
            };

            if !visited.insert(symbol) {
                continue;
            }

            let Some((_, resolution)) =
                self.resolve_surface_constant(symbol, cancellation, diagnostics)?
            else {
                continue;
            };

            let value = match resolution {
                bray_checker::ConstantReferenceResolution::Value(value) => value,
                bray_checker::ConstantReferenceResolution::Evaluated(value) => value.value(),
                _ => continue,
            };

            let condition = ExecutionCondition::from_constant(self.semantic_value_store()?, value);

            constants.insert(
                bray_bound_tree::BoundReferenceTarget::Surface(symbol).into(),
                condition,
            );
        }

        Ok(constants)
    }

    pub(super) fn execution_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<(ExecutionCondition, SourceSpan)>>, FactQueryError> {
        let (conditions, diagnostics) = self
            .predicate_condition_inputs(owner, anchors, cancellation)?
            .into_parts();

        Ok(DiagnosticResult::new(
            conditions
                .into_iter()
                .map(|(condition, _, source)| (condition, source))
                .collect(),
            diagnostics,
        ))
    }

    fn instance_execution_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        callable: Option<bray_symbols::CallableInstanceData>,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<(ExecutionCondition, SourceSpan)>>, FactQueryError> {
        let (conditions, diagnostics) = self
            .execution_condition_inputs(owner, anchors, cancellation)?
            .into_parts();

        let Some(callable) = callable else {
            return Ok(DiagnosticResult::new(conditions, diagnostics));
        };

        let values = self.semantic_value_store()?;

        let conditions = conditions
            .into_iter()
            .map(|(condition, source)| {
                let condition = bray_checker::substitute_execution_condition(
                    values,
                    &condition,
                    callable.substitution(),
                )?;

                Ok((condition, source))
            })
            .collect::<Result<_, FactQueryError>>()?;

        Ok(DiagnosticResult::new(conditions, diagnostics))
    }

    pub(super) fn clause_entry_conditions(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<ExecutionCondition>>, FactQueryError> {
        let mut diagnostics = DiagnosticBag::new();

        if key.declared_owner().source_declaration_id().is_none() {
            return Ok(DiagnosticResult::new(Vec::new(), diagnostics));
        }

        let anchor = if let Some(header) = key.callable_type_header() {
            header.syntax()
        } else {
            let symbols = self.symbol_graph()?;

            let symbol = symbols
                .symbol_for_key(key.declared_owner())
                .expect("source clause retains its declared symbol identity");

            symbols
                .declaration_syntax_anchor(symbol)
                .expect("source clause owner retains its declaration syntax")
        };

        let parent = self.execution_declaration(anchor)?;
        let anchor = key.source().syntax();

        let domain = parent.value().domains().iter().find(|domain| {
            domain.postconditions.contains(&anchor) || domain.guards.contains(&anchor)
        });

        let post = domain.is_some_and(|domain| domain.postconditions.contains(&anchor));

        if domain.is_none() && !parent.value().requirements().contains(&anchor) {
            // A nested callable owns its own entry context, independently of its lexical owner.
            return Ok(DiagnosticResult::new(Vec::new(), diagnostics));
        }

        let mut anchors = parent
            .value()
            .requirements()
            .iter()
            .copied()
            .filter(|requirement| {
                *requirement != anchor
                    && (post || requirement.full_range().start() < anchor.full_range().start())
            })
            .collect::<Vec<_>>();

        if let Some(domain) = domain {
            anchors.extend(
                domain
                    .guards
                    .iter()
                    .copied()
                    .take_while(|guard| *guard != anchor),
            );
        }

        if anchors.is_empty() {
            return Ok(DiagnosticResult::new(Vec::new(), diagnostics));
        }

        let conditions = self.predicate_condition_inputs(key, &anchors, cancellation)?;

        diagnostics.add_range(parent.diagnostics().iter().cloned());
        diagnostics.add_range(conditions.diagnostics().iter().cloned());

        let stable = if post {
            let stable = self.immutable_scalar_inputs(key, cancellation)?;

            diagnostics.add_range(stable.diagnostics().iter().cloned());

            stable.into_parts().0
        } else {
            Default::default()
        };

        let conditions = conditions
            .into_parts()
            .0
            .into_iter()
            .filter_map(|(condition, domain, _)| {
                // Borrowed storage and mutable parameter bindings can change before completion.
                // Ordinary predicates over immutable scalar values retain their entry meaning.
                (!post
                    || domain == bray_symbols::PredicateDomain::Value
                        && condition
                            .inputs()
                            .iter()
                            .all(|place| stable.contains(*place)))
                .then_some(condition)
            })
            .collect();

        Ok(DiagnosticResult::new(conditions, diagnostics))
    }

    fn immutable_scalar_inputs(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<std::collections::BTreeSet<bray_checker::ExecutionPlace>>,
        FactQueryError,
    > {
        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;

        let stable = |input, shape: &bray_symbols::CallableParameterData| {
            if shape.mode() != bray_symbols::CallableParameterMode::Immutable {
                return None;
            }

            let data = values.type_data(shape.ty());

            let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
                return None;
            };

            let role =
                crate::compilation::foreign::compiler_known_representation(self, *definition)?;

            (role.numeric_kind().is_some()
                || matches!(
                    role,
                    bray_compiler_known::RepresentationRole::ScalarBool
                        | bray_compiler_known::RepresentationRole::ScalarChar
                ))
            .then(|| bray_checker::ExecutionPlace::from(input))
        };

        if key.callable_type_header().is_some() {
            let header = crate::compilation::binder::callable_clause_header(&context, key)
                .map_err(crate::compilation::binder::binding_query_error)?;

            let inputs = self.execution_unit_inputs(key, cancellation)?;

            let result = inputs
                .into_iter()
                .zip(header.value().parameters())
                .filter_map(|(input, shape)| stable(input, shape))
                .collect();

            return Ok(DiagnosticResult::new(result, header.into_parts().1));
        }

        let symbol = context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .expect("source clause retains its declaration symbol");

        let Some(callable) = bray_symbols::CallableSymbolId::try_from_any(symbol) else {
            return Ok(DiagnosticResult::new(
                Default::default(),
                DiagnosticBag::new(),
            ));
        };

        let signature = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableSignatureQuery,
            >::new(callable))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [signature.value().callable_type()],
            cancellation,
        )?;

        let diagnostics =
            DiagnosticBag::merged_all([signature.diagnostics(), constants.diagnostics()]);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(Default::default(), diagnostics));
        }

        let ty = bray_checker::resolve_type_expression_template(
            values,
            signature.value().callable_type(),
            constants.value(),
        )
        .expect("checked signature has its demanded type constants");

        let data = values.type_data(ty);

        let bray_symbols::TypeData::Callable(callable_type) = data.as_ref() else {
            panic!("checked callable signature retains its callable type");
        };

        let result = signature
            .value()
            .parameters()
            .iter()
            .zip(callable_type.parameters())
            .filter_map(|(parameter, shape)| {
                stable(BoundReferenceTarget::Surface((*parameter).into()), shape)
            })
            .collect();

        Ok(DiagnosticResult::new(result, diagnostics))
    }

    pub(in crate::compilation) fn predicate_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<super::condition::DomainPostcondition>>, FactQueryError> {
        if owner.kind() == bray_bound_tree::BoundUnitKind::AnonymousCallable
            || owner.callable_type_header().is_some()
        {
            return self.callable_predicate_condition_inputs(owner, anchors, cancellation);
        }

        let mut conditions = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for anchor in anchors {
            let source = self.bound_source(*anchor)?;

            // A clause query owns the shared declaration identity independently of its caller.
            let key = BoundUnitKey::contract_clause(owner.declared_owner().clone(), source)
                .ok_or_else(|| {
                    SemanticQueryFailure::contract(
                        SemanticQueryContext::Source(anchor.source_id()),
                        SemanticQueryViolation::Missing(SemanticDataKind::Syntax),
                    )
                })?;

            let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

            let semantics =
                self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

            let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
            let body = self.body_semantics_with_cancellation(key.clone(), cancellation)?;

            let clause_diagnostics = DiagnosticBag::merged_all([
                bound.result().diagnostics(),
                semantics.result().diagnostics(),
                storage.result().diagnostics(),
                body.result().diagnostics(),
            ]);

            let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

            let constants = self.execution_constant_inputs(
                bound.result().value(),
                semantics.result().value(),
                cancellation,
                &mut diagnostics,
            )?;

            let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

            let predicates = self.execution_predicate_inputs(
                semantics.result().value(),
                cancellation,
                &mut diagnostics,
            )?;

            let normalized = bray_checker::predicate_conditions_with_inputs(
                bound.result().value(),
                semantics.result().value(),
                patterns.result().value(),
                self.semantic_value_store()?,
                &constants,
                Some((
                    storage.result().value(),
                    body.result().value().storage_flow(),
                )),
                Some(&predicates),
            )?;

            if clause_diagnostics.has_errors() {
                conditions.push((
                    ExecutionCondition::Unknown,
                    bray_symbols::PredicateDomain::Value,
                    span,
                ));
            } else {
                conditions.extend(
                    normalized
                        .into_iter()
                        .map(|(_, condition, domain)| (condition, domain, span)),
                );
            }

            diagnostics.add_range(clause_diagnostics);
        }

        Ok(DiagnosticResult::new(conditions, diagnostics))
    }
}

impl Compilation {
    pub(super) fn applicable_execution_obligation(
        &self,
        body: &BoundUnitKey,
        callable: Option<bray_symbols::CallableInstanceData>,
        declaration: &bray_checker::ExecutionDeclaration,
        required: bray_checker::ExecutionObligation,
        evidence: Option<&bray_checker::ExecutionCallEvidence>,
        cancellation: &CancellationToken,
    ) -> Result<Option<Option<SourceSpan>>, FactQueryError> {
        let bray_checker::ExecutionObligation::Property(property, selected) = required else {
            panic!("execution-domain selection requires a property obligation");
        };

        let requirements = self.instance_execution_condition_inputs(
            body,
            callable,
            declaration.requirements(),
            cancellation,
        )?;

        if requirements.diagnostics().has_errors() {
            return Ok(None);
        }

        let inputs = if callable.is_none() {
            self.execution_unit_inputs(body, cancellation)?
        } else {
            Vec::new()
        };

        let proves = |conditions: &[ExecutionCondition]| {
            evidence
                .unwrap_or(&bray_checker::ExecutionCallEvidence::default())
                .proves_trusted_with_inputs(conditions, &inputs)
        };

        let requirements = requirements
            .into_parts()
            .0
            .into_iter()
            .map(|(condition, _)| condition)
            .collect::<Vec<_>>();

        for domain in declaration.domains() {
            let Some(obligation) = domain
                .properties
                .iter()
                .find(|obligation| obligation.property == property)
            else {
                continue;
            };

            let clause = (!domain.guards.is_empty()).then_some(obligation.source.into());

            if selected.is_some() && selected != clause {
                continue;
            }

            let guards = self.instance_execution_condition_inputs(
                body,
                callable,
                &domain.guards,
                cancellation,
            )?;

            if guards.diagnostics().has_errors() {
                continue;
            }

            // Domains own their conjunction of shared immutable condition terms.
            let mut conditions = requirements.clone();

            conditions.extend(
                guards
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, _)| condition),
            );

            if proves(&conditions) {
                return Ok(Some(
                    (!domain.guards.is_empty()).then_some(obligation.source),
                ));
            }
        }

        if selected.is_none()
            && match callable {
                Some(callable) => self.is_constant_callable(callable, cancellation)?,
                None => self.unit_requires_constant_evaluation(body, cancellation)?,
            }
            && proves(&requirements)
        {
            return Ok(Some(None));
        }

        Ok(None)
    }
}

impl Compilation {
    pub(super) fn execution_call_contracts(
        &self,
        bound: &bray_bound_tree::BoundUnit,
        semantics: &bray_bound_tree::CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<
            std::collections::BTreeMap<
                bray_bound_tree::BoundExpressionId,
                Vec<bray_checker::ExecutionCompletionContract>,
            >,
        >,
        FactQueryError,
    > {
        let mut calls = std::collections::BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        for (expression, _) in bound.tree().expressions() {
            let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                semantics.selections().expression(expression)
            else {
                continue;
            };

            let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target() else {
                continue;
            };

            let contracts = self.callable_completion_contracts(callable, cancellation)?;

            diagnostics.add_range(contracts.diagnostics().iter().cloned());

            let contracts = contracts.into_parts().0;

            calls.insert(expression, contracts);
        }

        Ok(DiagnosticResult::new(calls, diagnostics))
    }

    pub(super) fn callable_completion_contracts(
        &self,
        callable: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<bray_checker::ExecutionCompletionContract>>, FactQueryError>
    {
        let mut diagnostics = DiagnosticBag::new();

        let Some(body) = self.callable_body_key(callable.definition())? else {
            if self
                .symbol_graph()?
                .declaration_syntax_anchor(callable.definition().symbol())
                .is_some()
            {
                return Ok(DiagnosticResult::new(Vec::new(), diagnostics));
            }

            let contract =
                self.imported_execution_contract(callable, &mut diagnostics, cancellation)?;

            let mut completion = Vec::new();

            for domain in &*contract.domains {
                let entry = self.imported_execution_conditions(
                    callable,
                    domain.entry.iter().copied(),
                    cancellation,
                )?;

                let mut postconditions = Vec::new();

                for (ordinal, term) in &*domain.postconditions {
                    let conditions =
                        self.imported_execution_conditions(callable, [*term], cancellation)?;

                    postconditions.extend(conditions.into_iter().map(|condition| {
                        (
                            condition,
                            bray_checker::ExecutionClauseId::Imported(*ordinal),
                        )
                    }));
                }

                completion.push(bray_checker::ExecutionCompletionContract {
                    entry,
                    postconditions,
                });
            }

            return Ok(DiagnosticResult::new(completion, diagnostics));
        };

        let declaration = self.execution_unit_declaration(&body)?;
        let mut contracts = Vec::new();

        for domain in declaration
            .value()
            .domains()
            .iter()
            .filter(|domain| !domain.postconditions.is_empty())
        {
            let anchors = declaration
                .value()
                .requirements()
                .iter()
                .chain(&domain.guards)
                .copied()
                .collect::<Vec<_>>();

            let entry = self.instance_execution_condition_inputs(
                &body,
                Some(callable),
                &anchors,
                cancellation,
            )?;

            let posts = self.instance_execution_condition_inputs(
                &body,
                Some(callable),
                &domain.postconditions,
                cancellation,
            )?;

            diagnostics.add_range(entry.diagnostics().iter().cloned());
            diagnostics.add_range(posts.diagnostics().iter().cloned());

            contracts.push(bray_checker::ExecutionCompletionContract {
                entry: entry
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, _)| condition)
                    .collect(),
                postconditions: posts
                    .into_parts()
                    .0
                    .into_iter()
                    .map(|(condition, source)| (condition, source.into()))
                    .collect(),
            });
        }

        Ok(DiagnosticResult::new(contracts, diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn completion_observation_publishes_its_checked_result_meaning() {
        for (body, valid) in [
            (
                "if value.is_complete() { return; } value.open = false; value.pending = false;",
                true,
            ),
            (
                "if value.is_complete() { value.open = true; return; } value.open = false; value.pending = false;",
                false,
            ),
            (
                "if !value.is_complete() { value.open = false; value.pending = false; }",
                true,
            ),
            (
                "let done = value.is_complete(); value.open = true; if done { return; } value.open = false; value.pending = false;",
                false,
            ),
        ] {
            let source = format!(
                r#"
                module app;

                struct Resource
                {{
                    mut open: bool;
                    mut pending: bool;
                    predicate complete(value: &Resource) = !value.open && !value.pending;

                    func is_complete() -> bool
                        executes(pure, total)
                        ensures(result == Resource.complete(value = &self))
                    {{
                        return !self.open && !self.pending;
                    }}
                }}

                func close(pos value: &mut Resource)
                    ensures(Resource.complete(value = &value))
                    when(Resource.complete(value = &value))
                    {{
                        executes(pure, total)
                    }}
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn completion_after_opaque_execution_requires_a_terminal_state() {
        for (completion, valid) in [
            ("value.open = false;", false),
            ("value = { open = false, pending = false };", true),
        ] {
            let source = format!(
                r#"
                trusted module app;
                struct Resource {{ mut open: bool; mut pending: bool; }}
                extern trusted func opaque();
                trusted func close(pos value: &mut Resource)
                    ensures(!value.open && !value.pending)
                {{
                    value.pending = false;
                    opaque();
                    {completion}
                }}
            "#
            );

            let compilation = compilation(&source);

            let checked = compilation
                .execution_properties(crate::test_support::source_function_body_key(
                    &compilation,
                    "close",
                ))
                .expect("fixture callable must resolve its execution properties");

            let diagnostics = checked.diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn predicate_calls_require_contract_expression_context() {
        use bray_diagnostics::{DiagnosticArgValue, DiagnosticSelectionRejectionReason};

        for declaration in [
            "func root() -> bool { return ready(value = true); }",
            "func root() { let action = lambda() -> bool { return ready(value = true); }; }",
            "func root(value: bool = ready(value = true)) {}",
            "struct Owner { predicate complete(value: bool) = value; } func root() -> bool { return Owner.complete(value = true); }",
        ] {
            let source = format!("module app; predicate ready(value: bool) = value; {declaration}");
            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(diagnostics.iter().flat_map(|diagnostic| diagnostic.args()).any(|argument| {
                matches!(argument.value(), DiagnosticArgValue::SelectionRejections(rejections)
                    if rejections.rejections().iter().any(|rejection| matches!(
                        rejection.reason(), DiagnosticSelectionRejectionReason::PredicateOutsideContract)))
            }), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn ordinary_predicate_meaning_tracks_completion_and_mutation() {
        for (body, valid) in [
            ("value.open = false;", true),
            ("value.open = true;", false),
            ("value.open = false; value.open = true;", false),
            ("if value.open { value.open = false; }", true),
        ] {
            let source = format!(
                r#"
                module app;

                struct Resource {{
                    mut open: bool;
                    predicate complete(value: &Resource) = !value.open;
                }}

                predicate finished(value: &Resource) = Resource.complete(value = value);

                func root(pos value: &mut Resource)
                    executes(total)
                    ensures(finished(value = &value))
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn ordinary_predicate_meaning_proves_guarded_completion() {
        let compilation = compilation(
            r#"
            module app;

            struct Resource {
                mut open: bool;
                predicate complete(value: &Resource) = !value.open;
            }

            func close(pos value: &mut Resource)
                ensures(Resource.complete(value = &value))
                when(Resource.complete(value = &value)) {
                    executes(pure, total)
                }
            {
                if !value.open {
                    return;
                }

                value.open = false;
            }
        "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn anonymous_total_proof_uses_its_own_field_requirements() {
        for (requirement, valid) in [("value.ready", true), ("!value.ready", false)] {
            let source = format!(
                r#"
                module app;
                struct Flags {{ ready: bool; }}
                func observe(pos value: &Flags) requires(value.ready) executes(total) {{}}
                func root(pos outer: Flags) {{
                    let action = lambda(pos value: &Flags)
                        requires({requirement}) executes(total)
                    {{ observe(value); }};
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn ordinary_predicate_meaning_proves_result_completion() {
        for body in [
            "value.open = false; return Ok(unit);",
            "if fail { return Error(false); } value.open = false; return Ok(unit);",
            "try status(fail); value.open = false; return Ok(unit);",
        ] {
            let source = format!(
                r#"
                module app;

                struct Resource {{
                    mut open: bool;
                    predicate complete(value: &Resource) = !value.open;
                }}

                func status(pos fail: bool) -> Result<unit, bool> executes(total) {{
                    if fail {{ return Error(false); }}
                    return Ok(unit);
                }}

                func close(pos value: &mut Resource, pos fail: bool) -> Result<unit, bool>
                    ensures(result matches Error(_) || Resource.complete(value = &value))
                    when(Resource.complete(value = &value)) {{
                        executes(pure, total)
                        ensures(result matches Ok(_))
                    }}
                {{
                    if !value.open {{ return Ok(unit); }}
                    {body}
                }}

                func wrapper(pos value: &mut Resource, pos fail: bool) -> Result<unit, bool>
                    ensures(result matches Error(_) || Resource.complete(value = &value))
                    when(Resource.complete(value = &value)) {{
                        executes(pure, total)
                        ensures(result matches Ok(_))
                    }}
                {{
                    return close(&mut value, fail);
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn successful_propagation_retains_the_borrowed_completion_condition() {
        for valid in [true, false] {
            let mutation = if valid { "" } else { "value.open = true;" };

            let source = format!(
                r#"
                module app;

                struct Resource
                {{
                    mut open: bool;
                }}

                func close(pos value: &mut Resource, pos fail: bool) -> Result<unit, bool>
                    executes(total)
                    ensures(result matches Error(_) || !value.open)
                {{
                    if fail {{ return Error(false); }}
                    value.open = false;
                    return Ok(unit);
                }}

                func wrapper(pos value: &mut Resource, pos fail: bool) -> Result<unit, bool>
                    executes(total)
                    ensures(result matches Error(_) || !value.open)
                {{
                    try close(&mut value, fail);
                    {mutation}
                    return Ok(unit);
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn propagation_publishes_only_its_selected_boundary_result() {
        for (body, valid) in [
            ("let value: bool? = none; value?; return true;", true),
            ("let value: bool? = true; value?; return true;", false),
            (
                "let value: bool? = none; let nested: bool? = { value?; yield true; }; return true;",
                false,
            ),
            (
                "let value: bool? = none; let nested: bool? = { value?; yield true; }; return nested;",
                true,
            ),
        ] {
            let source = format!(
                r#"
                module app;

                func root() -> bool?
                    executes(total)
                    ensures(result matches none)
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn anonymous_postconditions_check_mutated_owned_parameters() {
        for (modifier, value, valid, failure) in [
            (
                "mut",
                "true",
                true,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            ),
            (
                "mut",
                "false",
                false,
                DiagnosticKind::CheckingExecutionGuaranteeNotProven,
            ),
            (
                "",
                "true",
                false,
                DiagnosticKind::CheckingMissingMutationAuthority,
            ),
        ] {
            let source = format!(
                r#"
                module app;
                struct Flags {{ mut ready: bool; }}
                func root() {{
                    let action = lambda(pos {modifier} value: Flags) -> Flags
                        ensures(result.ready) executes(total)
                    {{
                        value.ready = {value};
                        return value;
                    }};
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(diagnostics, failure);
            }
        }
    }

    #[test]
    fn anonymous_owned_parameter_borrow_requires_mutable_mode() {
        for (modifier, valid) in [("mut", true), ("", false)] {
            let source = format!(
                r#"
                module app;
                struct Flags {{ mut ready: bool; }}
                func root() {{
                    let action = lambda(pos {modifier} value: Flags) {{
                        let borrowed: &mut Flags = &mut value;
                        borrowed.ready = true;
                    }};
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingMissingMutationAuthority,
                );
            }
        }
    }

    #[test]
    fn returned_owned_fields_use_the_latest_mutation() {
        for (body, valid) in [
            ("value.ready = true;", true),
            ("value.ready = false;", false),
            ("value.ready = true; value.ready = false;", false),
            ("value.ready = false; value.ready = true;", true),
            ("value.ready = true; change(&mut value);", false),
        ] {
            let source = format!(
                r#"
                module app;
                struct Flags {{ mut ready: bool; }}
                func change(pos value: &mut Flags) {{ value.ready = false; }}
                func root(pos mut value: Flags) -> Flags ensures(result.ready)
                {{ {body} return value; }}
            "#
            );

            let compilation = compilation(&source);

            let checked = compilation
                .execution_properties(crate::test_support::source_function_body_key(
                    &compilation,
                    "root",
                ))
                .expect("fixture callable must resolve its execution properties");

            assert_eq!(
                !checked.diagnostics().has_errors(),
                valid,
                "{source}: {checked:#?}"
            );
        }
    }

    #[test]
    fn projection_preserves_completion_until_mutation() {
        for (body, valid) in [
            (
                "let projected: &mut Flags = &mut value; _ = projected.ready;",
                true,
            ),
            ("observe(&mut value);", true),
            ("mutate(&mut value);", false),
            (
                "let projected = project(&mut value); _ = projected.ready;",
                true,
            ),
            (
                "let projected = project(&mut value); projected.ready = false;",
                false,
            ),
            (
                "let projected: &mut Flags = &mut value; projected.ready = false;",
                false,
            ),
        ] {
            let source = format!(
                r#"
                module app;

                struct Flags {{ mut ready: bool; }}

                func observe(pos value: &mut Flags) executes(pure, total) {{}}

                func mutate(pos value: &mut Flags) {{ value.ready = false; }}

                func project(pos value: &mut Flags) -> &mut Flags executes(pure, total)
                {{
                    return &mut value;
                }}

                func root(pos value: &mut Flags)
                    when(value.ready) {{ ensures(value.ready) }}
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn whole_value_replacement_installs_the_moved_field_evidence() {
        for (requirement, valid) in [("other.ready", true), ("!other.ready", false)] {
            let source = r#"
                module app;

                struct Flags
                {
                    ready: bool;
                }

                func root(pos mut value: Flags, other: Flags)
                    requires(REQUIREMENT)
                    when(value.ready)
                    {
                        ensures(value.ready)
                    }
                {
                    value = other;
                }
            "#
            .replace("REQUIREMENT", requirement);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn predicate_guards_compare_selected_identity_without_name_special_cases() {
        for (predicate, valid) in [("complete", true), ("other", false)] {
            let source = r#"
                module app;

                predicate complete(value: bool) = value;

                predicate other(value: bool) = !value;

                func root(pos flag: bool)
                    when(complete(flag))
                    {
                        executes(total)
                        ensures(PREDICATE(flag))
                    } {}
            "#
            .replace("PREDICATE", predicate);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn error_results_are_normal_exits_and_do_not_invent_completion_values() {
        for (promise, valid) in [("true", true), ("false", false)] {
            let source = r#"
                module app;

                func root() -> Result<unit, unit>
                    when(true)
                    {
                        executes(total)
                        ensures(PROMISE)
                    }
                {
                    return Error(unit);
                }
            "#
            .replace("PROMISE", promise);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn field_mutation_preserves_only_disjoint_entry_facts() {
        for (field, valid) in [("other", true), ("ready", false)] {
            let source = r#"
                module app;

                struct Flags
                {
                    mut ready: bool;
                    mut other: bool;
                }

                func update(pos mut value: Flags)
                    requires(value.ready)
                    when(true)
                    {
                        ensures(value.ready)
                    }
                {
                    value.FIELD = false;
                }
            "#
            .replace("FIELD", field);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn numeric_entry_guards_use_checked_values() {
        for (argument, valid) in [("4", true), ("0", false)] {
            let source = r#"
                module app;

                func guarded(pos value: i32)
                    when(value > 0)
                    {
                        executes(total)
                    }
                {
                    if value > 0
                    {
                        return;
                    }

                    panic("outside domain");
                }

                func caller()
                    executes(total)
                {
                    guarded(ARGUMENT);
                }
            "#
            .replace("ARGUMENT", argument);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn completion_evidence_composes_through_checked_operators() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return true;
            }

            func caller() -> bool
                when(true)
                {
                    ensures(!result)
                }
            {
                return !leaf();
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn completion_evidence_follows_the_returned_value_and_rejects_false_callee_promises() {
        for (returned, valid) in [("true", true), ("false", false)] {
            let source = r#"
                module app;

                func leaf() -> bool
                    when(true)
                    {
                        ensures(result)
                    }
                {
                    return RETURNED;
                }

                func caller() -> bool
                    when(true)
                    {
                        ensures(result)
                    }
                {
                    let first = leaf();
                    let moved = first;

                    return moved;
                }
            "#
            .replace("RETURNED", returned);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn nonreturning_recursion_cannot_supply_a_runtime_completion_witness() {
        let compilation = compilation(
            r#"
            module app;

            func first() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return second();
            }

            func second() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return first();
            }
        "#,
        );

        // Partial correctness applies only to normal outcomes. This cycle has none,
        // so its completion claims supply no result to a caller and prove no termination.
        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in ["first", "second"] {
            let dependencies =
                crate::compilation::result_dependencies::source_function_dependencies(
                    &compilation,
                    name,
                );

            assert!(!dependencies.normal_completion());
        }
    }

    #[test]
    fn mutation_invalidates_completion_evidence_at_the_changed_location() {
        let compilation = compilation(
            r#"
            module app;

            func leaf() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                return true;
            }

            func caller() -> bool
                when(true)
                {
                    ensures(result)
                }
            {
                let mut value = leaf();

                value = false;
                return value;
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }

    #[test]
    fn callers_require_proven_callee_entry_guards() {
        for (argument, valid) in [("true", true), ("flag", false), ("false", false)] {
            let source = r#"
                module app;

                func guarded(pos flag: bool)
                    when(flag)
                    {
                        executes(total)
                    }
                {
                    if flag
                    {
                        return;
                    }

                    panic("unguarded input");
                }

                func caller(pos flag: bool)
                    executes(total)
                {
                    guarded(ARGUMENT);
                }
            "#
            .replace("ARGUMENT", argument);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn flow_joins_keep_common_values_and_discard_conflicting_values() {
        for (alternative, valid) in [("true", true), ("false", false)] {
            let source = r#"
                module app;

                func joined(pos branch: bool) -> bool
                    when(true)
                    {
                        ensures(result)
                    }
                {
                    let mut value: bool = true;

                    if branch
                    {
                        value = true;
                    }
                    else
                    {
                        value = ALTERNATIVE;
                    }

                    return value;
                }
            "#
            .replace("ALTERNATIVE", alternative);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn nested_and_overlapping_groups_hold_independently_of_order() {
        for clauses in [
            r#"
                when(left)
                {
                    ensures(result)
                    when(right)
                    {
                        executes(pure, total)
                    }
                }
                when(right)
                {
                    ensures(result)
                }
            "#,
            r#"
                when(right)
                {
                    ensures(result)
                }
                when(left)
                {
                    when(right)
                    {
                        executes(total, pure)
                    }
                    ensures(result)
                }
            "#,
        ] {
            let source = r#"
                module app;
                func either(pos left: bool, pos right: bool) -> bool
                    CLAUSES
                {
                    return left || right;
                }
            "#
            .replace("CLAUSES", clauses);

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn conditional_execution_checks_only_the_promised_entry_domain() {
        let compilation = compilation(
            r#"
            module app;

            func guarded(pos flag: bool) -> bool
                when(flag)
                {
                    executes(pure, total)
                    ensures(result)
                }
            {
                if flag
                {
                    return true;
                }

                panic("outside promised domain");
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn guarded_postconditions_observe_exit_values_after_mutating_the_guard_input() {
        for (postcondition, valid) in [("!flag", true), ("flag", false)] {
            let source = r#"
                module app;

                func change(pos mut flag: bool)
                    when(flag)
                    {
                        executes(total)
                        ensures(POSTCONDITION)
                    }
                {
                    flag = false;
                }
            "#
            .replace("POSTCONDITION", postcondition);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn guarded_postconditions_without_execution_properties_are_checked() {
        let compilation = compilation(
            r#"
            module app;

            func invalid()
                when(true)
                {
                    ensures(false)
                } {}
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingExecutionGuaranteeNotProven,
        );
    }
}
