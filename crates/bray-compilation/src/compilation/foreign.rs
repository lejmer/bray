pub(in crate::compilation) mod diagnostic;
mod directive;
mod layout;
mod platform;
mod query;
mod validation;

pub(in crate::compilation) use validation::{
    compiler_known_representation, target_abi_value_from_type,
};
