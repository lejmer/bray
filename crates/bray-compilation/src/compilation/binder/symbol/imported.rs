use bray_binder::{BinderFactError, BinderFactResult};
use bray_diagnostics::DiagnosticResult;
use bray_package_interface::{
    ImportedImplementationFact, ImportedSemanticFact, InterfaceSemanticFactKind,
};
use bray_symbols::{
    CallableContractTemplate, CallableSignatureTemplate, GenericDeclarationTemplate,
    ImportedSymbolFactAddress, UnevaluatedDefaultTemplate,
};

use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::ImportedSemanticFactKey;

pub(super) fn imported_callable_signature(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<CallableSignatureTemplate>> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::CallableSignature,
    )?;

    let [ImportedSemanticFact::CallableSignature(fact)] = result.value().as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    // Candidate-facing facts retain shallow Arc-backed templates and diagnostics.
    Ok(DiagnosticResult::new(
        fact.signature().clone(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_generic_declaration(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<GenericDeclarationTemplate>> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::GenericDeclaration,
    )?;

    let [ImportedSemanticFact::GenericDeclaration(fact)] = result.value().as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    // Candidate-facing facts retain shallow Arc-backed templates and diagnostics.
    Ok(DiagnosticResult::new(
        fact.declaration().clone(),
        result.diagnostics().clone(),
    ))
}

pub(super) fn imported_callable_parameter_default(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<UnevaluatedDefaultTemplate>> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::CallableParameterDefault,
    )?;

    let [ImportedSemanticFact::CallableParameterDefault(fact)] = result.value().as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    // Imported fact results share immutable diagnostic storage.
    Ok(DiagnosticResult::new(
        fact.default(),
        result.diagnostics().clone(),
    ))
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
