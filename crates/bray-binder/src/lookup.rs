mod binding;
mod category;
mod diagnostic;
mod implementation;
mod path;
mod pattern;
#[cfg(test)]
mod test_support;

pub(crate) use binding::{combine_name_lookups, lookup_surface_name, lookup_unqualified_name};
pub(crate) use category::{ResolvedName, ResolvedTypeName, ResolvedValueName, classify_type};
pub(crate) use diagnostic::{NameReference, lookup_diagnostic};
pub use implementation::{
    BoundImplementationUsing, bind_implementation_using, bind_named_trait_implementation_path,
};
pub use path::{
    ImportedPathRoot, NameAccess, bind_owner_surface_path, bind_surface_path_with_re_exports,
};
pub(crate) use path::{
    PathBindingContext, bind_module_path, bind_owner_path, bind_source_path,
    lookup_surface_name_with_imports, visible_imported_path_root,
};
