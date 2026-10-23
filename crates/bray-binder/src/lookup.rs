mod access;
mod binding;
mod category;
mod diagnostic;
mod implementation;
mod path;
mod pattern;
#[cfg(test)]
mod test_support;

pub(crate) use access::can_access_symbol;
pub use access::check_member_access;
pub(crate) use binding::{combine_name_lookups, lookup_surface_name, lookup_unqualified_name};
pub(crate) use category::{ResolvedName, ResolvedTypeName, ResolvedValueName, classify_type};
pub(crate) use diagnostic::{NameReference, lookup_diagnostic};
pub use implementation::{
    BoundImplementationUsing, bind_implementation_using, bind_named_trait_implementation_path,
};
pub use path::{
    ImportedPathRoot, NameAccess, bind_owner_surface_path, bind_surface_path_with_member_lookup,
};
pub(crate) use path::{
    PathBindingContext, bind_module_path, bind_owner_path, bind_source_path,
    lookup_dependency_declared, lookup_surface_name_with_imports, module_using_access,
    visible_imported_path_root,
};
