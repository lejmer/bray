mod core;
mod imported;
mod member;
mod prefix;
mod route;

pub(super) use core::classify_pattern_target;
pub use core::{NameAccess, bind_owner_surface_path, bind_surface_path_with_re_exports};
pub(crate) use core::{
    PathBindingContext, bind_module_path, bind_owner_path, bind_source_path,
    visible_imported_path_root,
};
pub use imported::ImportedPathRoot;
pub(crate) use prefix::lookup_surface_name_with_imports;
pub(super) use prefix::token_reference;
