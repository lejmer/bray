mod builder;
mod error;
mod kind;
pub(crate) mod local_id_remap;
mod model;
pub(crate) mod reconstruction;
mod splice;
mod substitution;
mod validation;

pub use builder::MirUnitBuilder;
pub use error::MirCapacityError;
pub use kind::MirUnitKind;
pub use model::{
    MirExecutableTemplateId, MirGeneratedLifecycleKey, MirGeneratedLifecycleRole,
    MirImportedExecutableKey, MirUnit, MirUnitKey,
};
pub use reconstruction::{
    MirReconstructionMappings, reachable_blocks, reconstruct_reachable, reconstruct_with_edits,
};
pub use splice::{append_cleanup_continuation, inline_call};
