use bray_execution::ExecutableHostContract;

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

    match (selected_runtime, runtime_inputs.as_slice()) {
        (Some(_), []) => Err(LinkPlanBuildError::MissingRuntimeComponent),
        (None, []) => Ok(()),
        (None, [_first, ..]) => Err(LinkPlanBuildError::UnexpectedRuntimeComponent),
        (Some(selected), runtimes) if runtimes.iter().any(|runtime| *runtime != selected) => {
            Err(LinkPlanBuildError::RuntimeArtifactMismatch)
        }
        (Some(_), [_first, ..]) => Ok(()),
    }
}
