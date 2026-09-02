use std::fmt::Display;

use bray_codegen::CodegenFailure;

pub(crate) fn resource_limit<T, U>(
    value: U,
    resource: &'static str,
) -> Result<T, CodegenFailure>
where
    T: TryFrom<U>,
    U: Copy + Display,
{
    T::try_from(value).map_err(|_| CodegenFailure::resource_limit(resource, value))
}

pub(crate) fn target_value<T, U>(
    value: U,
    constraint: &'static str,
) -> Result<T, CodegenFailure>
where
    T: TryFrom<U>,
    U: Copy + Display,
{
    T::try_from(value).map_err(|_| CodegenFailure::unsupported_target_value(constraint, value))
}
