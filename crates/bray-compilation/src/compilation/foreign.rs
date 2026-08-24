pub(in crate::compilation) mod diagnostic;
mod directive;
mod layout;
pub(in crate::compilation) mod platform;
mod query;
mod static_storage;
mod validation;

pub(in crate::compilation) use validation::{
    compiler_known_representation, target_abi_value_from_type,
};
