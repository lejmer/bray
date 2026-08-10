use bray_runtime_interface::ExecutableHostContract;

use crate::{LinkInput, LinkInputProvenance, LinkPlanBuildError};

pub(crate) fn validate_execution_inputs(
    executable_host: Option<&ExecutableHostContract>,
    inputs: &[LinkInput],
) -> Result<(), LinkPlanBuildError> {
    let selected_runtime = executable_host.and_then(ExecutableHostContract::runtime_artifact);

    let runtime_inputs: Vec<_> = inputs
        .iter()
        .filter_map(|input| {
            let LinkInputProvenance::Runtime(runtime) = input.provenance() else {
                return None;
            };

            Some(runtime)
        })
        .collect();

    match selected_runtime {
        Some(_) if runtime_inputs.is_empty() => Err(LinkPlanBuildError::MissingRuntimeComponent),
        None if runtime_inputs.is_empty() => Ok(()),
        None => Err(LinkPlanBuildError::UnexpectedRuntimeComponent),
        Some(selected) if runtime_inputs.iter().any(|runtime| *runtime != selected) => {
            Err(LinkPlanBuildError::RuntimeArtifactMismatch)
        }
        Some(_) => Ok(()),
    }
}
