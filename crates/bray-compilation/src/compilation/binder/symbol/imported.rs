use bray_binder::{BinderFactError, BinderFactResult};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::{
    ImportedImplementationFact, ImportedSemanticFact, InterfaceSemanticFactKind,
};
use bray_symbols::{
    CallableContractTemplate, GenericConstraintTemplate, GenericOwnerId, ImplementationSymbolId,
    ImportedSymbolFactAddress,
};

use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::ImportedSemanticFactKey;

pub(super) fn imported_constraints(
    context: &CompilationBinderFacts<'_>,
    owner: GenericOwnerId,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<(Vec<GenericConstraintTemplate>, DiagnosticBag)> {
    let kind = if ImplementationSymbolId::try_from_any(owner.symbol()).is_some() {
        InterfaceSemanticFactKind::Implementation
    } else {
        InterfaceSemanticFactKind::GenericConstraint
    };

    let result = imported_facts(context, address, kind)?;

    let constraints = match result.value().as_ref() {
        [ImportedSemanticFact::Implementation(implementation)] => implementation
            .constraints()
            .iter()
            .copied()
            .map(GenericConstraintTemplate::Resolved)
            .collect(),
        facts if kind == InterfaceSemanticFactKind::GenericConstraint => facts
            .iter()
            .map(|fact| {
                let ImportedSemanticFact::GenericConstraint(fact) = fact else {
                    return Err(BinderFactError::DependencyUnavailable);
                };

                Ok(GenericConstraintTemplate::Resolved(fact.constraint()))
            })
            .collect::<BinderFactResult<Vec<_>>>()?,
        _ => return Err(BinderFactError::DependencyUnavailable),
    };

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

pub(in crate::compilation) fn imported_implementation(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<ImportedImplementationFact>> {
    let result = imported_facts(context, address, InterfaceSemanticFactKind::Implementation)?;

    let [ImportedSemanticFact::Implementation(implementation)] = result.value().as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    // The adapter owns an Arc-backed header independently of the exact fact result.
    Ok(DiagnosticResult::new(
        implementation.clone(),
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
