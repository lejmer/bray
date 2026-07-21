mod context;
mod symbol;
mod value_type;
mod value_type_surface;

pub(super) use context::CompilationBinderFacts;
pub(in crate::compilation) use symbol::CompilationSymbolFacts;
pub(in crate::compilation) use value_type::bind_declared_value_type_templates;
