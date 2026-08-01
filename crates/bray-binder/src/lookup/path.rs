mod core;
mod prefix;

pub use core::{NameAccess, bind_surface_path_with_re_exports};
pub(crate) use core::{PathBindingContext, bind_module_path, bind_source_path};
pub(super) use core::classify_pattern_target;
pub(super) use prefix::token_reference;
