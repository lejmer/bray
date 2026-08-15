//! Language-defined properties supplied by one target profile.

mod abi;
mod atomic;
mod c;
mod kind;
mod model;
mod scalar;
mod value;

pub use abi::{TargetAbiSupport, TargetAbiScalars, TargetForeignAbiContract};
pub use atomic::{
    TargetAtomicSupport, TargetAtomicOperations, TargetAtomicRepresentation,
    TargetAtomicRepresentationSupport,
};
pub use c::{TargetCDataModel, TargetCScalarKind};
pub use kind::TargetPropertyKind;
pub use model::{
    TargetAddressSpaces, TargetAlignmentLimits, TargetProperties, TargetPlatformIdentity,
    TargetOperationSupport,
};
pub use scalar::{TargetScalarSupport, TargetScalarKind};
pub use value::TargetPropertyValue;
