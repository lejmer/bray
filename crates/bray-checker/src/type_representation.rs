mod check;
mod directive;
mod model;

pub use check::check_declared_type_representation;
pub use model::{
    DeclaredStorageMember, DeclaredTypeDefinition, DeclaredUnionVariant, RepresentationIntegerType,
    TypeRepresentationContext,
};
