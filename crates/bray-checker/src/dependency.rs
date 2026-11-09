mod assignment;
mod call;
mod check;
mod cleanup;
mod completion;
mod defaults;
mod copying;
mod equations;
mod implementation;
mod operation;
mod parameters;
mod projection;
mod propagation;
mod result;
mod returned;
mod value;
mod witness;

pub(crate) use check::check_dependency_contracts;
pub(crate) use cleanup::check_synchronous_cleanup;
pub use implementation::generator_result_dependencies;
pub(crate) use implementation::implementation_dependency_source;
pub(crate) use result::{ReportDependencies, State as ResultState, Transfer as ResultTransfer};
pub(crate) use returned::{
    callee_result_template, invocation_result_template, opaque_result, parameter_root,
    resolved_result_template, result_input, result_inputs,
};
pub(crate) use value::{ValueInputs, transfer_targets};

pub(crate) use call::{
    OperationInput, cleanup_report_contract, operation_inputs, prepare_reference,
};
pub(crate) use witness::map_requirements;
