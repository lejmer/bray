use std::collections::BTreeMap;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundReferenceTarget, BoundUnitKey};
use bray_checker::{
    ExecutionObligation, execution_condition_is_implied, remap_execution_condition_inputs,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{AnySymbolId, CallableSignatureQuery, CallableSymbolId, SymbolQueryRequest};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

struct ConformanceExecutionDomain {
    entry: Vec<bray_checker::ExecutionCondition>,
    properties: Vec<bray_checker::ExecutionProperty>,
    postconditions: Vec<(
        bray_checker::ExecutionCondition,
        bray_checker::ExecutionClauseId,
    )>,
}

impl Compilation {
    pub(in crate::compilation) fn execution_contract_conformance(
        &self,
        requirement: AnySymbolId,
        fulfillment: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<bool>, FactQueryError> {
        let required = self.conformance_execution_domains(requirement, cancellation)?;
        let provided = self.conformance_execution_domains(fulfillment, cancellation)?;

        let inputs =
            self.execution_contract_input_mapping(fulfillment, requirement, cancellation)?;

        let mut diagnostics = DiagnosticBag::merged_all([
            required.diagnostics(),
            provided.diagnostics(),
            inputs.diagnostics(),
        ]);

        let mut valid = !diagnostics.has_errors();

        let identity = inputs
            .value()
            .values()
            .map(|input| (*input, *input))
            .collect();

        let graph = self.symbol_graph()?;

        let source = graph
            .declaration_syntax_anchor(fulfillment)
            .or_else(|| graph.declaration_syntax_anchor(requirement))
            .map(|anchor| bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range()));

        for domain in required.value() {
            let mut properties = std::collections::BTreeSet::new();
            let mut postconditions = Vec::new();

            for candidate in provided.value() {
                if candidate.entry.iter().all(|condition| {
                    execution_condition_is_implied(condition, &domain.entry, inputs.value())
                }) {
                    properties.extend(candidate.properties.iter().copied());

                    postconditions.extend(candidate.postconditions.iter().map(|(condition, _)| {
                        remap_execution_condition_inputs(condition, inputs.value())
                    }));
                }
            }

            let missing_properties = domain
                .properties
                .iter()
                .filter(|property| !properties.contains(property))
                .map(|property| ExecutionObligation::Property(*property, None));

            let missing_posts = domain
                .postconditions
                .iter()
                .filter(|(condition, _)| {
                    !execution_condition_is_implied(condition, &postconditions, &identity)
                })
                .map(|(_, clause)| ExecutionObligation::Postcondition(*clause));

            for obligation in missing_properties.chain(missing_posts) {
                valid = false;

                if let Some(source) = source {
                    diagnostics.add(super::proof::guarantee_diagnostic(
                        obligation, source, source, false,
                    ));
                }
            }
        }

        Ok(DiagnosticResult::new(valid, diagnostics))
    }

    fn conformance_execution_domains(
        &self,
        symbol: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<ConformanceExecutionDomain>>, FactQueryError> {
        let mut domains = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
            return Ok(DiagnosticResult::new(domains, diagnostics));
        };

        if let Some(owner) = self.execution_contract_owner(symbol)? {
            let declared = self.execution_declaration(owner.source().syntax())?;
            diagnostics.add_range(declared.diagnostics().iter().cloned());

            // Ordinary clauses without execution guarantees use the existing contract matcher.
            if declared.value().clauses().is_empty() {
                return Ok(DiagnosticResult::new(domains, diagnostics));
            }

            for domain in declared.value().domains() {
                let anchors = declared
                    .value()
                    .requirements()
                    .iter()
                    .chain(&domain.guards)
                    .copied()
                    .collect::<Vec<_>>();

                let entry = self.execution_condition_inputs(&owner, &anchors, cancellation)?;

                let posts =
                    self.execution_condition_inputs(&owner, &domain.postconditions, cancellation)?;

                diagnostics.add_range(entry.diagnostics().iter().cloned());
                diagnostics.add_range(posts.diagnostics().iter().cloned());

                domains.push(ConformanceExecutionDomain {
                    entry: entry
                        .into_parts()
                        .0
                        .into_iter()
                        .map(|(condition, _)| condition)
                        .collect(),
                    properties: domain
                        .properties
                        .iter()
                        .map(|property| property.property)
                        .collect(),
                    postconditions: posts
                        .into_parts()
                        .0
                        .into_iter()
                        .map(|(condition, source)| (condition, source.into()))
                        .collect(),
                });
            }
        } else {
            let context = self.binding_context(cancellation)?;

            let contract = context
                .resolve_symbol_query(
                    SymbolQueryRequest::<bray_symbols::CallableContractsQuery>::new(callable),
                )
                .map_err(crate::compilation::binder::binding_query_error)?;

            diagnostics.add_range(contract.diagnostics().iter().cloned());

            if self.symbol_graph()?.symbol_origin(symbol)
                == Some(bray_symbols::SymbolOrigin::CompilerKnown)
            {
                domains.push(ConformanceExecutionDomain {
                    entry: Vec::new(),
                    properties: contract
                        .value()
                        .invocation_behavior()
                        .execution_properties()
                        .to_vec(),
                    postconditions: Vec::new(),
                });
            }

            let inputs = self.execution_callable_inputs(symbol, cancellation)?;
            let values = self.semantic_value_store()?;

            for domain in &*contract.value().execution_contract().domains {
                let entry = domain
                    .entry
                    .iter()
                    .map(|term| {
                        bray_checker::execution_condition_from_term(values, *term, &inputs)
                            .map_err(FactQueryError::SemanticValueStore)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let postconditions = domain
                    .postconditions
                    .iter()
                    .map(|(ordinal, term)| {
                        bray_checker::execution_condition_from_term(values, *term, &inputs)
                            .map(|condition| {
                                (
                                    condition,
                                    bray_checker::ExecutionClauseId::Imported(*ordinal),
                                )
                            })
                            .map_err(FactQueryError::SemanticValueStore)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                domains.push(ConformanceExecutionDomain {
                    entry,
                    properties: domain.properties.to_vec(),
                    postconditions,
                });
            }
        }

        Ok(DiagnosticResult::new(domains, diagnostics))
    }
    pub(super) fn execution_contract_owner(
        &self,
        symbol: AnySymbolId,
    ) -> Result<Option<BoundUnitKey>, FactQueryError> {
        if CallableSymbolId::try_from_any(symbol).is_none() {
            return Ok(None);
        }

        let symbols = self.symbol_graph()?;

        let Some(anchor) = symbols.declaration_syntax_anchor(symbol) else {
            return Ok(None);
        };

        let Some(owner) = symbols.symbol_key(symbol) else {
            return Ok(None);
        };

        // The clause key retains the declaration's shared identity independently of the graph.
        Ok(BoundUnitKey::contract_clause(
            owner.clone(),
            self.bound_source(anchor)?,
        ))
    }

    fn execution_contract_input_mapping(
        &self,
        source: AnySymbolId,
        target: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<BTreeMap<BoundReferenceTarget, BoundReferenceTarget>>,
        FactQueryError,
    > {
        let mut mapping = BTreeMap::new();

        let (Some(source), Some(target)) = (
            CallableSymbolId::try_from_any(source),
            CallableSymbolId::try_from_any(target),
        ) else {
            return Ok(DiagnosticResult::new(mapping, DiagnosticBag::new()));
        };

        let context = self.binding_context(cancellation)?;

        let source = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(source))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let target = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(target))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let diagnostics = source.diagnostics().merged(target.diagnostics());

        for (source, target) in source
            .value()
            .parameters()
            .iter()
            .zip(target.value().parameters())
        {
            mapping.insert(
                BoundReferenceTarget::Surface((*source).into()),
                BoundReferenceTarget::Surface((*target).into()),
            );
        }

        if let (Some(source), Some(target)) = (source.value().receiver(), target.value().receiver())
        {
            mapping.insert(
                BoundReferenceTarget::Surface(source.parameter().into()),
                BoundReferenceTarget::Surface(target.parameter().into()),
            );
        }

        Ok(DiagnosticResult::new(mapping, diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use crate::test_support::compilation;

    #[test]
    fn storage_projections_require_checked_pure_total_bodies() {
        for (contract, shared, mutable, valid) in [
            (
                "executes(pure, total)",
                "return &storage.value;",
                "return &mut storage.value;",
                true,
            ),
            (
                "",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(pure)",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(total)",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(pure, total) requires(storage.value == 0)",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "when(storage.value == 0) { executes(pure, total) }",
                "return &storage.value;",
                "return &mut storage.value;",
                false,
            ),
            (
                "when(true) { executes(pure, total) }",
                "return &storage.value;",
                "return &mut storage.value;",
                true,
            ),
            (
                "executes(pure, total)",
                "panic(1);",
                "return &mut storage.value;",
                false,
            ),
            (
                "executes(pure, total)",
                "return &storage.value;",
                "loop {}",
                false,
            ),
            (
                "executes(pure, total)",
                "return &storage.value;",
                "storage.value = 1; return &mut storage.value;",
                false,
            ),
        ] {
            let source = format!(
                r#"
                module app;

                struct Policy {{ mut value: i32; }}

                impl Policy(Storage<i32>)
                {{
                    trusted static func create(pos value: i32) -> Self
                    {{ return Policy {{ value = value }}; }}

                    static func borrow(pos storage: &Self) -> &i32 {contract}
                    {{ {shared} }}

                    static func borrow_mut(pos storage: &mut Self) -> &mut i32 {contract}
                    {{ {mutable} }}

                    trusted static func destroy(pos storage: &mut Self) {{}}
                    trusted static func release(pos storage: Self) {{}}
                }}

                func root(pos value: &box[Policy] i32) -> i32 executes(pure, total)
                {{
                    return match value {{ case box(inner) {{ yield inner; }} }};
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
    fn concrete_witnesses_are_checked_and_open_termination_dependencies_are_rejected() {
        for (parameters, subject, constraint, valid) in [
            ("", "Value", "", true),
            ("<T>", "T", "with(T: Readable)", false),
        ] {
            let source = r#"
                module app;
                trait Readable { func read() -> bool executes(total); }
                struct Value {}
                impl Value(Readable)
                {
                    func read() -> bool executes(total) { return true; }
                }
                func rootPARAMETERS(pos value: &SUBJECT) -> bool
                    CONSTRAINT
                    executes(total)
                {
                    return value.read();
                }
            "#
            .replace("PARAMETERS", parameters)
            .replace("SUBJECT", subject)
            .replace("CONSTRAINT", constraint);

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
    fn lifecycle_fulfillments_and_trait_defaults_keep_execution_obligations() {
        for (provided, valid) in [("executes(total)", true), ("", false)] {
            let source = r#"
                module app;
                struct Value { destruct() PROVIDED {} }
                trait Disposable { destruct() executes(total); }
                impl Value(Disposable) {}
            "#
            .replace("PROVIDED", provided);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }

        for (body, valid) in [("return true;", true), ("loop {}", false)] {
            let source = r#"
                module app;
                trait Readable { func read() -> bool executes(total) { BODY } }
                struct Value {}
                impl Value(Readable) {}
            "#
            .replace("BODY", body);

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
    fn trait_guarded_postconditions_follow_completion_evidence() {
        for (post, body, valid) in [
            ("result", "true", true),
            ("!result", "false", false),
            ("result", "false", false),
            ("true", "true", false),
        ] {
            let source = r#"
                module app;
                trait Readable
                {
                    func read(pos flag: bool) -> bool
                        when(flag) { ensures(result) };
                }
                struct Value {}
                impl Value(Readable)
                {
                    func read(pos flag: bool) -> bool
                        when(flag) { ensures(POST) }
                    {
                        return BODY;
                    }
                }
            "#
            .replace("POST", post)
            .replace("BODY", body);

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
    fn trait_fulfillments_cannot_invent_execution_properties() {
        for (provided, valid) in [
            ("executes(pure, total)", true),
            ("executes(total)", false),
            ("", false),
        ] {
            let source = r#"
                module app;

                trait Readable
                {
                    func read() -> bool
                        executes(pure, total);
                }

                struct Value {}

                impl Value(Readable)
                {
                    func read() -> bool
                        PROVIDED
                    {
                        return true;
                    }
                }
            "#
            .replace("PROVIDED", provided);

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(diagnostics.is_empty(), valid, "{source}: {diagnostics:?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingExecutionGuaranteeNotProven,
                );
            }
        }
    }

    #[test]
    fn trait_guard_implication_maps_the_fulfillment_inputs() {
        for (guard, valid) in [("true", true), ("flag", true), ("!flag", false)] {
            let source = r#"
                module app;

                trait Readable
                {
                    func read(pos flag: bool) -> bool
                        when(flag)
                        {
                            executes(total)
                        };
                }

                struct Value {}

                impl Value(Readable)
                {
                    func read(pos flag: bool) -> bool
                        when(GUARD)
                        {
                            executes(total)
                        }
                    {
                        return flag;
                    }
                }
            "#
            .replace("GUARD", guard);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
}
