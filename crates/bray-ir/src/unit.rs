mod builder;
mod error;
mod kind;
pub(crate) mod local_id_remap;
mod model;
mod splice;
pub(crate) mod reconstruction;
mod validation;

pub use builder::MirUnitBuilder;
pub use error::MirCapacityError;
pub use kind::MirUnitKind;
pub use model::{
    MirExecutableTemplateId, MirGeneratedLifecycleKey, MirGeneratedLifecycleRole,
    MirImportedExecutableKey, MirUnit, MirUnitKey,
};
pub use splice::inline_scalar_call;
pub use reconstruction::{
    MirReconstructionMappings, reconstruct_reachable, reconstruct_with_edits,
};
