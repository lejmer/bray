mod binding;
mod category;
mod diagnostic;
mod path;

pub(crate) use binding::{combine_name_lookups, lookup_surface_name, lookup_unqualified_name};
pub(crate) use category::{ResolvedName, ResolvedTypeName, classify_type};
pub(crate) use diagnostic::{NameReference, lookup_diagnostic};
pub(crate) use path::{NameAccess, PathBindingContext};
