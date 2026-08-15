//! Language-defined facts supplied by one target profile.

mod abi;
mod atomic;
mod c;
mod kind;
mod model;
mod scalar;
mod value;

pub use abi::{TargetAbiFacts, TargetAbiScalarFacts, TargetForeignAbiFacts};
pub use atomic::{
    TargetAtomicFacts, TargetAtomicOperationFacts, TargetAtomicRepresentation,
    TargetAtomicRepresentationFacts,
};
pub use c::{TargetCAbiFacts, TargetCScalarKind};
pub use kind::TargetFactKind;
pub use model::{
    TargetAddressSpaceFacts, TargetAlignmentFacts, TargetFacts, TargetIdentityFacts,
    TargetOperationFacts,
};
pub use scalar::{TargetScalarFacts, TargetScalarKind};
pub use value::TargetFactValue;
