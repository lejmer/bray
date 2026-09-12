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

impl Compilation {
    pub(in crate::compilation) fn execution_contract_conformance(
        &self,
        requirement: AnySymbolId,
        fulfillment: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<bool>, FactQueryError> {
        let mut diagnostics = DiagnosticBag::new();

        let Some(required_owner) = self.execution_contract_owner(requirement)? else {
            return Ok(DiagnosticResult::new(true, diagnostics));
        };

        let required = self.execution_declaration(required_owner.source().syntax())?;

        if required.value().clauses().is_empty() {
            return Ok(DiagnosticResult::new(true, diagnostics));
        }

        let Some(provided_owner) = self.execution_contract_owner(fulfillment)? else {
            return Ok(DiagnosticResult::new(false, diagnostics));
        };

        let provided = self.execution_declaration(provided_owner.source().syntax())?;

        let inputs =
            self.execution_contract_input_mapping(fulfillment, requirement, cancellation)?;

        diagnostics.add_range(required.diagnostics().iter().cloned());
        diagnostics.add_range(provided.diagnostics().iter().cloned());
        diagnostics.add_range(inputs.diagnostics().iter().cloned());

        let mut valid = !diagnostics.has_errors();

        for domain in required.value().domains() {
            let anchors = required
                .value()
                .requirements()
                .iter()
                .chain(&domain.guards)
                .copied()
                .collect::<Vec<_>>();

            let assumptions =
                self.execution_condition_inputs(&required_owner, &anchors, cancellation)?;

            diagnostics.add_range(assumptions.diagnostics().iter().cloned());

            let assumptions = assumptions
                .into_parts()
                .0
                .into_iter()
                .map(|(condition, _)| condition)
                .collect::<Vec<_>>();

            let mut available_properties = std::collections::BTreeSet::new();
            let mut available_postconditions = Vec::new();

            for source in provided.value().domains() {
                let anchors = provided
                    .value()
                    .requirements()
                    .iter()
                    .chain(&source.guards)
                    .copied()
                    .collect::<Vec<_>>();

                let conditions =
                    self.execution_condition_inputs(&provided_owner, &anchors, cancellation)?;

                diagnostics.add_range(conditions.diagnostics().iter().cloned());

                if !conditions.diagnostics().has_errors()
                    && conditions.value().iter().all(|(condition, _)| {
                        execution_condition_is_implied(condition, &assumptions, inputs.value())
                    })
                {
                    available_properties
                        .extend(source.properties.iter().map(|property| property.property));

                    let posts = self.execution_condition_inputs(
                        &provided_owner,
                        &source.postconditions,
                        cancellation,
                    )?;

                    diagnostics.add_range(posts.diagnostics().iter().cloned());

                    if !posts.diagnostics().has_errors() {
                        available_postconditions.extend(posts.value().iter().map(
                            |(condition, _)| {
                                remap_execution_condition_inputs(condition, inputs.value())
                            },
                        ));
                    }
                }
            }

            for property in &domain.properties {
                if !available_properties.contains(&property.property) {
                    valid = false;

                    diagnostics.add(super::proof::guarantee_diagnostic(
                        ExecutionObligation::Property(property.property, None),
                        property.source,
                        bray_source::SourceSpan::new(
                            provided_owner.source().syntax().source_id(),
                            provided_owner.source().syntax().full_range(),
                        ),
                        false,
                    ));
                }
            }

            if !domain.guards.is_empty() {
                let posts = self.execution_condition_inputs(
                    &required_owner,
                    &domain.postconditions,
                    cancellation,
                )?;

                diagnostics.add_range(posts.diagnostics().iter().cloned());

                let identity = inputs
                    .value()
                    .values()
                    .map(|input| (*input, *input))
                    .collect();

                for (condition, source) in posts.value() {
                    if !execution_condition_is_implied(
                        condition,
                        &available_postconditions,
                        &identity,
                    ) {
                        valid = false;

                        diagnostics.add(super::proof::guarantee_diagnostic(
                            ExecutionObligation::Postcondition(*source),
                            *source,
                            bray_source::SourceSpan::new(
                                provided_owner.source().syntax().source_id(),
                                provided_owner.source().syntax().full_range(),
                            ),
                            false,
                        ));
                    }
                }
            }
        }

        Ok(DiagnosticResult::new(
            valid && !diagnostics.has_errors(),
            diagnostics,
        ))
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
