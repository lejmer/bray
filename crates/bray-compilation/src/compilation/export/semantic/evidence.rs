use bray_symbols::{AnySymbolId, CallableContractEvidence, CallableDefinitionId};

use crate::compilation::Compilation;
use crate::compilation::unit::ProofDependency;

use super::super::PackageInterfaceExportError;
use super::context::SemanticExporter;
use super::templates::incomplete;

pub(super) fn export_callable_evidence(
    compilation: &Compilation,
    symbol: AnySymbolId,
    export: &mut SemanticExporter<'_>,
) -> Result<
    Vec<CallableContractEvidence<bray_package_interface::InterfaceCallableEvidenceTarget>>,
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

    let inputs = compilation
        .source_proof_inputs(&key, cancellation)
        .map_err(super::super::fact_query_export_error)?;

    if proven.result().diagnostics().has_errors() || inputs.diagnostics().has_errors() {
        return Err(incomplete(symbol));
    }

    let mut evidence = Vec::new();

    for (required, dependencies) in inputs.value() {
        let Some(obligation) = required.contract() else {
            continue;
        };

        if !proven.result().value().contains(required) {
            continue;
        }

        let dependencies = dependencies
            .iter()
            .map(|dependency| {
                let ProofDependency::Selected(target, required) = dependency else {
                    return Err(incomplete(symbol));
                };

                let obligation = required.contract().ok_or_else(|| incomplete(symbol))?;

                let callable = export.callable_instance_id(target.callable())?;

                let dispatch = target
                    .dispatch()
                    .map(|requirement| {
                        Ok((
                            export.type_id(requirement.subject())?,
                            export.trait_application_id(requirement.trait_application())?,
                        ))
                    })
                    .transpose()?;

                Ok((
                    bray_package_interface::InterfaceCallableEvidenceTarget::new(
                        callable, dispatch,
                    ),
                    obligation,
                ))
            })
            .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

        evidence.push(CallableContractEvidence::new(obligation, dependencies));
    }

    Ok(evidence)
}
