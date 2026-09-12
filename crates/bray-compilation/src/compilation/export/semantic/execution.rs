use std::collections::BTreeSet;

use bray_binder::SymbolQueryProvider;
use bray_package_interface::InterfaceCallableContract;
use bray_symbols::{
    AnySymbolId, CallableContractsQuery, CallableExecutionTarget, CallableSymbolId,
    SymbolQueryRequest,
};

use super::super::PackageInterfaceExportError;
use super::context::SemanticExporter;
use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;

pub(super) fn callable_contracts(
    compilation: &Compilation,
    binder: &CompilationBindingContext<'_>,
    selected: &BTreeSet<AnySymbolId>,
    export: &mut SemanticExporter<'_>,
) -> Result<Vec<InterfaceCallableContract>, PackageInterfaceExportError> {
    let execution = compilation
        .export_execution_contracts(selected, &compilation.state.cancellation)
        .map_err(super::super::invalid_compilation_fact_error)?;

    if let Some(diagnostic) = execution
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.severity() == bray_diagnostics::SeverityKind::Error)
    {
        // The export failure owns the diagnostic after the preparation result is released.
        return Err(PackageInterfaceExportError::ExecutionEvidence(Box::new(
            diagnostic.clone(),
        )));
    }

    let mut contracts = Vec::new();

    for symbol in selected.iter().copied() {
        let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
            continue;
        };

        let contract = binder
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(callable))
            .map_err(super::super::binding_query_export_error)?;

        if contract.diagnostics().has_errors() {
            return Err(super::templates::incomplete(symbol));
        }

        let mut serialized = export.callable_contract(symbol, contract.value())?;

        if let Some(evidence) = execution.value().get(&symbol) {
            let evidence = evidence.try_map(
                export,
                |export, term| export.constant_term_id(term),
                |export, target| match target {
                    CallableExecutionTarget::Callable(callable) => export
                        .callable_instance_id(callable)
                        .map(CallableExecutionTarget::Callable),
                    CallableExecutionTarget::Indirect(ty) => {
                        export.type_id(ty).map(CallableExecutionTarget::Indirect)
                    }
                },
            )?;

            serialized = serialized.with_execution_contract(evidence);
        }

        contracts.push(serialized);
    }

    contracts.sort_by(|left, right| left.owner().cmp(right.owner()));

    Ok(contracts)
}
