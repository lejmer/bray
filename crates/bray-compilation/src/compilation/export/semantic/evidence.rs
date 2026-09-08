use bray_bound_tree::CallableProofResult;
use bray_symbols::{AnySymbolId, CallableContractEvidence, CallableDefinitionId};

use crate::compilation::Compilation;

use super::super::PackageInterfaceExportError;
use super::context::SemanticExporter;
use super::templates::incomplete;

pub(super) fn export_callable_evidence(
    compilation: &Compilation,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
) -> Result<
    Vec<CallableContractEvidence<bray_package_interface::InterfaceSymbolReference>>,
    PackageInterfaceExportError,
> {
    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
        return Ok(Vec::new());
    };

    let Some(key) = compilation
        .callable_body_key(definition)
        .map_err(super::super::fact_query_export_error)?
    else {
        let assertions = compilation
            .foreign_callable_assertions(
                definition.callable_symbol(),
                &compilation.state.cancellation,
            )
            .map_err(super::super::fact_query_export_error)?;

        if assertions.diagnostics().has_errors() {
            return Err(incomplete(symbol));
        }

        return Ok(assertions
            .value()
            .iter()
            .copied()
            .map(CallableContractEvidence::foreign_assertion)
            .collect());
    };

    let cancellation = &compilation.state.cancellation;

    // Each cached query retains its Arc-backed key independently of this export request.
    let proven = compilation
        .callable_proofs_with_cancellation(key.clone(), cancellation)
        .map_err(super::super::fact_query_export_error)?;

    let body = compilation
        .body_semantics_with_cancellation(key.clone(), cancellation)
        .map_err(super::super::fact_query_export_error)?;

    let expressions = compilation
        .expression_semantics_with_cancellation(key, cancellation)
        .map_err(super::super::fact_query_export_error)?;

    if proven.result().diagnostics().has_errors()
        || body.result().diagnostics().has_errors()
        || expressions.result().diagnostics().has_errors()
    {
        return Err(incomplete(symbol));
    }

    let mut evidence = Vec::new();

    for checked in body.result().value().execution_proofs() {
        let CallableProofResult::Candidate(candidate) = checked else {
            continue;
        };

        let Some(obligation) = candidate.obligation().contract() else {
            continue;
        };

        if !proven.result().value().contains(&candidate.obligation()) {
            continue;
        }

        let dependencies = candidate
            .dependencies()
            .iter()
            .map(|dependency| {
                let target = dependency
                    .target()
                    .definition(expressions.result().value().selections())
                    .ok_or_else(|| incomplete(symbol))?;

                let obligation = dependency
                    .obligation()
                    .contract()
                    .ok_or_else(|| incomplete(symbol))?;

                Ok((export.symbol_reference(target.symbol())?, obligation))
            })
            .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

        evidence.push(CallableContractEvidence::new(obligation, dependencies));
    }

    Ok(evidence)
}
