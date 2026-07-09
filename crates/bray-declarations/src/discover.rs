//! Source-unit declaration discovery.

mod children;
mod names;
mod source_unit;
mod surface;
mod syntax;

pub use source_unit::discover_source_unit_declarations;
