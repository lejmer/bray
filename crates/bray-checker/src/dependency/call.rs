mod contract;
mod instantiation;
mod operation;
mod references;

pub(super) use contract::selected_iteration_contract;
pub(crate) use contract::{
    cleanup_report_contract, selected_call_contracts, selected_scoped_contracts,
};

pub(crate) use references::prepare_reference;

pub(crate) use operation::{
    OperationInput, operation_inputs, operation_result_dependencies, selected_operation_contracts,
};
