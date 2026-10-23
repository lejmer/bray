use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{
    BoundCallableTarget, BoundExecutionSite, CheckedSemanticSelections, ConversionTarget,
    SelectedArgument, SelectedConstructionInput, SelectedConversion, SelectedOperation,
    SemanticSelection, StorageFlow,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_lowering::DeclarationPreconditions;
use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, CallablePredicateContractsQuery,
    CallableSignatureQuery, SymbolQueryRequest,
};

use crate::compilation::Compilation;
use crate::compilation::binder::binding_query_error;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn lowering_declaration_preconditions(
        &self,
        selections: &CheckedSemanticSelections,
        flow: &StorageFlow,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<BTreeMap<CallableDefinitionId, DeclarationPreconditions>>,
        FactQueryError,
    > {
        let context = self.binding_context(cancellation)?;
        let mut preconditions = BTreeMap::new();
        let mut visited = BTreeSet::new();
        let mut diagnostics = DiagnosticBag::new();
        let mut invocations = Vec::new();

        for entry in selections.entries() {
            invocations.clear();

            selected_invocations(
                entry.expression().into(),
                entry.selection(),
                &mut invocations,
            );

            for (site, callable) in &invocations {
                if flow.preconditions_proven(*site)
                    && selections.invoked_callable(*site) == Some(*callable)
                {
                    continue;
                }

                if !visited.insert(callable.definition()) {
                    continue;
                }

                let declaration = callable.definition();

                let checked = context
                    .resolve_symbol_query(
                        SymbolQueryRequest::<CallablePredicateContractsQuery>::new(
                            declaration.callable_symbol(),
                        ),
                    )
                    .map_err(binding_query_error)?;

                diagnostics.add_range(checked.diagnostics().clone());

                let clauses = checked
                    .value()
                    .iter()
                    .filter(|clause| {
                        clause.kind() == bray_symbols::CallableContractClauseKind::Requires
                    })
                    .filter(|clause| {
                        clause
                            .predicate()
                            .is_some_and(|predicate| predicate.requires_runtime_check())
                    })
                    .map(|clause| clause.ordinal())
                    .collect::<Vec<_>>();

                if clauses.is_empty() {
                    continue;
                }

                let signature = context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                        declaration.callable_symbol(),
                    ))
                    .map_err(binding_query_error)?;

                diagnostics.add_range(signature.diagnostics().clone());

                preconditions.insert(
                    declaration,
                    DeclarationPreconditions::new(
                        clauses,
                        signature.value().receiver().map(|receiver| receiver.mode()),
                    ),
                );
            }
        }

        Ok(DiagnosticResult::new(preconditions, diagnostics))
    }
}

fn selected_invocations(
    site: BoundExecutionSite,
    selection: &SemanticSelection,
    result: &mut Vec<(BoundExecutionSite, CallableInstanceData)>,
) {
    match selection {
        SemanticSelection::Call(call) => {
            if let BoundCallableTarget::Declaration(callable) = call.target() {
                result.push((site, callable));
            }

            for argument in call.arguments() {
                if let SelectedArgument::Explicit { conversion, .. } = argument {
                    conversion_invocations(site, conversion, result);
                }
            }
        }
        SemanticSelection::Operation(operation) => {
            if let SelectedOperation::Conversion(conversion) = operation {
                conversion_invocations(site, conversion, result);
            } else if let Some(callable) = operation.invoked_callable() {
                result.push((site, callable));
            }

            if let SelectedOperation::Construction(construction) = operation {
                for input in construction.inputs() {
                    if let SelectedConstructionInput::Explicit { conversion, .. } = input {
                        conversion_invocations(site, conversion, result);
                    }
                }
            }
        }
        SemanticSelection::ScopedUse(scoped) => {
            let expression = site
                .expression()
                .expect("selected scope has its source expression");

            result.push((
                BoundExecutionSite::ScopedEnter(expression),
                scoped.enter().0,
            ));

            result.push((BoundExecutionSite::ScopedExit(expression), scoped.exit().0));
        }
        SemanticSelection::Reference(_)
        | SemanticSelection::CallableReference(_)
        | SemanticSelection::StaticReference(_)
        | SemanticSelection::Predicate(_)
        | SemanticSelection::Iteration(_)
        | SemanticSelection::Propagation(_) => {}
    }
}

fn conversion_invocations(
    site: BoundExecutionSite,
    conversion: &SelectedConversion,
    result: &mut Vec<(BoundExecutionSite, CallableInstanceData)>,
) {
    if let Some(callable) = conversion.target().invoked_callable() {
        result.push((site, callable));
    }

    if let ConversionTarget::Composite(children) = conversion.target() {
        for child in children.iter() {
            conversion_invocations(site, child, result);
        }
    }
}
