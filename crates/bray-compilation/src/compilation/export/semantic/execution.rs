use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_diagnostics::DiagnosticResult;
use bray_package_interface::InterfaceCallableContract;
use bray_symbols::{
    AnySymbolId, CallableContractSet, CallableExecutionTarget, CallableSymbolId, SymbolQueryKind,
};

use super::super::PackageInterfaceExportError;
use super::context::SemanticExporter;
use crate::compilation::Compilation;
use crate::fact::{CompilationFactKey, SymbolQueryKey};

pub(super) fn callable_contracts(
    compilation: &Compilation,
    resolved: &BTreeMap<AnySymbolId, Arc<DiagnosticResult<CallableContractSet>>>,
    selected: &BTreeSet<AnySymbolId>,
    execution: &BTreeMap<AnySymbolId, bray_symbols::ResolvedCallableExecutionContract>,
    export: &mut SemanticExporter<'_>,
) -> Result<Vec<InterfaceCallableContract>, PackageInterfaceExportError> {
    let mut contracts = Vec::new();

    for symbol in selected.iter().copied() {
        let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
            continue;
        };

        let key = CompilationFactKey::from(SymbolQueryKey::new(
            callable.into_any(),
            SymbolQueryKind::CallableContracts,
        ));

        compilation
            .state
            .fact_runtime
            .check_request_cycle(&key)
            .map_err(super::super::fact_query_export_error)?;

        compilation
            .state
            .cancellation
            .check()
            .map_err(super::super::fact_query_export_error)?;

        let contract = resolved
            .get(&symbol)
            .ok_or_else(|| super::templates::incomplete(symbol))?;

        compilation
            .state
            .fact_runtime
            .record_completed_request(&key)
            .map_err(super::super::fact_query_export_error)?;

        if contract.diagnostics().has_errors() {
            return Err(super::templates::incomplete(symbol));
        }

        let mut serialized = export.callable_contract(symbol, contract.value())?;

        if let Some(evidence) = execution.get(&symbol) {
            let evidence = evidence.try_map(
                export,
                |export, term| export.constant_term_id(term),
                |export, target| match target {
                    CallableExecutionTarget::Callable(callable) => export
                        .callable_instance_id(callable)
                        .map(CallableExecutionTarget::Callable),
                    CallableExecutionTarget::Requirement {
                        callable,
                        requirement,
                    } => Ok(CallableExecutionTarget::Requirement {
                        callable: export.callable_instance_id(callable)?,
                        requirement: (
                            export.type_id(requirement.subject())?,
                            export.trait_application_id(requirement.trait_application())?,
                        ),
                    }),
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
