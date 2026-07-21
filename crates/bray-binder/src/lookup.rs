mod binding;
mod category;
mod diagnostic;
mod implementation;
mod path;
#[cfg(test)]
mod test_support;

pub(crate) use binding::{combine_name_lookups, lookup_surface_name, lookup_unqualified_name};
pub(crate) use category::{ResolvedName, ResolvedTypeName, classify_type};
pub(crate) use diagnostic::{NameReference, lookup_diagnostic};
pub use implementation::bind_named_trait_implementation_path;
pub use path::NameAccess;
pub(crate) use path::{PathBindingContext, bind_module_path};
