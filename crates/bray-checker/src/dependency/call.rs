mod contract;
mod input;
mod instantiation;
mod operation;
mod references;

pub(crate) use contract::{
    cleanup_report_contract, selected_call_contracts, selected_implicit_contracts,
};
pub(crate) use input::{selected_input_contracts, selected_input_template};

pub(crate) use references::prepare_reference;

pub(crate) use operation::{
    OperationInput, operation_inputs, operation_result_dependencies, selected_operation_contracts,
};
