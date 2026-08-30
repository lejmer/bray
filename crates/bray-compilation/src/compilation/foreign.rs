pub(in crate::compilation) mod diagnostic;
mod directive;
mod layout;
pub(in crate::compilation) mod platform;
mod query;
pub(in crate::compilation) mod runtime;
mod source_role;
mod static_storage;
mod validation;

pub(in crate::compilation) use source_role::has_source_role;
pub(in crate::compilation) use validation::{
    AbiField, abi_type_matches, compiler_known_representation, target_abi_value_from_type,
};
