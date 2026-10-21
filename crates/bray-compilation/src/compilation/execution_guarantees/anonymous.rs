use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundReferenceTarget, BoundUnitKey, BoundUnitRoot};
use bray_checker::ExecutionCondition;
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceSpan;

use crate::compilation::{Compilation, binder};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn execution_unit_inputs(
        &self, owner: &BoundUnitKey, cancellation: &CancellationToken,
    ) -> Result<Vec<BoundReferenceTarget>, FactQueryError> {
        if owner.callable_type_header().is_some() {
            let bound = self.bound_unit_with_cancellation(owner.clone(), cancellation)?;
            let mut parameters = bound.result().value().local_symbols().bindings().iter()
                .filter_map(|binding| binding.key().ordinal().map(|ordinal| (ordinal, BoundReferenceTarget::Local(binding.id().into())))).collect::<Vec<_>>();
            parameters.sort_by_key(|(ordinal, _)| *ordinal);
            return Ok(parameters.into_iter().map(|(_, reference)| reference).collect());
        }
        if owner.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault {
            return Ok(self.default_template_inputs(owner, cancellation)?.into_iter().filter_map(|input| input.target).collect());
        }

        if owner.kind() != bray_bound_tree::BoundUnitKind::AnonymousCallable {
            let symbol = self.symbol_graph()?.symbol_for_key(owner.declared_owner()).expect("checked unit retains its declaration owner");
            return self.execution_callable_inputs(symbol, cancellation);
        }
        let bound = self.bound_unit_with_cancellation(owner.clone(), cancellation)?;
        let BoundUnitRoot::AnonymousCallable { callable, .. } = bound.result().value().root() else {
            panic!("anonymous contract owner retains its own callable root");
        };
        Ok(bound.result().value().local_symbols().anonymous_callable(callable)
            .expect("anonymous body owns its parameter identities").parameters().iter()
            .map(|parameter| BoundReferenceTarget::Local((*parameter).into())).collect())
    }

    pub(super) fn anonymous_predicate_condition_inputs(
        &self,
        owner: &BoundUnitKey,
        anchors: &[SyntaxAnchor],
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<(ExecutionCondition, bool, SourceSpan)>>, FactQueryError> {
        let declared =
            self.declared_value_type_templates_with_cancellation(owner.clone(), cancellation)?;

        let bound = self.bound_unit_with_cancellation(owner.clone(), cancellation)?;

        let template = declared
            .result()
            .value()
            .callable_type()
            .expect("anonymous unit owns its callable type");

        let references = self.execution_unit_inputs(owner, cancellation)?;

        let context = self.binding_context(cancellation)?;

        let lexical_owner = context
            .symbols()
            .symbol_for_key(owner.declared_owner())
            .expect("anonymous callable retains its lexical declaration");

        let syntax = self.execution_declaration_node(owner.source().syntax())?;
        let values = self.semantic_value_store()?;

        let mut diagnostics = DiagnosticBag::merged_all([
            declared.result().diagnostics(),
            bound.result().diagnostics(),
        ]);

        let mut conditions = Vec::new();

        for anchor in anchors {
            let checked = binder::checked_callable_type_contracts(
                &context,
                lexical_owner,
                syntax,
                template,
                Some(std::slice::from_ref(anchor)),
            )
            .map_err(binder::binding_query_error)?;

            let behavior = checked.value();

            let completion = behavior
                .deferred_execution()
                .unwrap_or(behavior.invocation());

            let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

            if checked.diagnostics().has_errors() {
                conditions.push((ExecutionCondition::Unknown, true, span));
            } else {
                for predicate in behavior
                    .invocation()
                    .predicate_requirements()
                    .iter()
                    .chain(completion.predicate_guarantees())
                {
                    let condition = predicate
                        .condition()
                        .map(|term| {
                            bray_checker::execution_condition_from_term(values, term, &references)
                        })
                        .transpose()?
                        .unwrap_or(ExecutionCondition::Unknown);

                    conditions.push((condition, predicate.is_trusted(), span));
                }
            }

            diagnostics.add_range(checked.into_parts().1);
        }

        Ok(DiagnosticResult::new(conditions, diagnostics))
    }
}
