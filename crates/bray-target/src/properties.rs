//! Language-defined properties supplied by one target profile.

mod abi;
mod atomic;
mod c;
mod kind;
mod model;
mod scalar;
mod value;

pub use abi::{TargetAbiScalars, TargetAbiSupport, TargetForeignAbiContract};
pub use atomic::{
    TargetAtomicOperations, TargetAtomicRepresentation, TargetAtomicRepresentationSupport,
    TargetAtomicSupport,
};
pub use c::{TargetCDataModel, TargetCScalarKind};
pub use kind::TargetPropertyKind;
pub use model::{
    TargetAddressSpaces, TargetAlignmentLimits, TargetNativeSymbolSupport, TargetOperationSupport,
    TargetPlatformIdentity, TargetProperties,
};
pub use scalar::{TargetScalarKind, TargetScalarSupport};
pub use value::TargetPropertyValue;
