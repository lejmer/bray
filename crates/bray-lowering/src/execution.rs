use bray_compiler_known::ImplementationHook;
use bray_runtime_interface::ExecutionLaneRequirement;
use bray_symbols::{AvailableCompilerKnownSymbols, CallableExecutionRequirement};

pub(crate) fn execution_lane_requirements(
    known: &AvailableCompilerKnownSymbols,
    requirements: &[CallableExecutionRequirement],
) -> Vec<ExecutionLaneRequirement> {
    requirements
        .iter()
        .filter_map(
            |requirement| match known.symbol_implementation(requirement.declaration()) {
                Some(ImplementationHook::BlockingExecution) => {
                    Some(ExecutionLaneRequirement::Blocking)
                }
                Some(ImplementationHook::ComputeExecution) => {
                    Some(ExecutionLaneRequirement::Compute)
                }
                Some(ImplementationHook::MainThreadExecution) => {
                    Some(ExecutionLaneRequirement::MainThread)
                }
                _ => None,
            },
        )
        .collect()
}
