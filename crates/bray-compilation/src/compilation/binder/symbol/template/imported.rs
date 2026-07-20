use bray_binder::{BinderFactError, BinderFactResult};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::{ImportedSemanticFact, InterfaceSemanticFactKind};
use bray_symbols::{
    CallableContractTemplate, GenericConstraintTemplate, ImportedSymbolFactAddress,
};

use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::ImportedSemanticFactKey;

pub(super) fn imported_constraints(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<(Vec<GenericConstraintTemplate>, DiagnosticBag)> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::GenericConstraint,
    )?;

    let mut constraints = Vec::new();

    for fact in result.value().iter() {
        let ImportedSemanticFact::GenericConstraint(fact) = fact else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        constraints.push(GenericConstraintTemplate::Resolved(fact.constraint()));
    }

    // Imported fact results share immutable diagnostic storage.
    Ok((constraints, result.diagnostics().clone()))
}

pub(super) fn imported_callable_contract(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<CallableContractTemplate>> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::CallableContracts,
    )?;

    let [ImportedSemanticFact::CallableContracts(fact)] = result.value().as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    // The candidate-facing result shares the imported contract and diagnostics.
    Ok(DiagnosticResult::new(
        CallableContractTemplate::Resolved(fact.contract().clone()),
        result.diagnostics().clone(),
    ))
}

fn imported_facts(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
    kind: InterfaceSemanticFactKind,
) -> BinderFactResult<std::sync::Arc<DiagnosticResult<std::sync::Arc<[ImportedSemanticFact]>>>> {
    context
        .compilation
        .imported_semantic_fact_result_with_cancellation(
            ImportedSemanticFactKey::new(address.interface(), address.symbol(), kind),
            context.cancellation,
        )
        .map_err(|error| match error {
            crate::fact::FactQueryError::Cancelled => BinderFactError::Cancelled,
            _ => BinderFactError::DependencyUnavailable,
        })
}
