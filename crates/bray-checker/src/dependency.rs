mod assignment;
mod call;
mod check;
mod defaults;
mod implementation;
mod operation;
mod projection;
mod propagation;
mod result;
mod returned;
mod value;

pub(crate) use call::selected_call_contracts;
pub(crate) use check::check_dependency_contracts;
pub(crate) use implementation::implementation_dependency_source;
pub use result::infer_result_dependencies;
pub(crate) use returned::{call_result_template, opaque_result};
pub(crate) use value::ValueInputs;
