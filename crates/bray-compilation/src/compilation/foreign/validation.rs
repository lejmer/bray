mod abi;
mod check;

pub(in crate::compilation) use abi::{
    compiler_known_representation, target_abi_value_from_type,
};
pub(super) use check::{
    callable_surface, validate_callable_surface, validate_platform_service_surface,
};
