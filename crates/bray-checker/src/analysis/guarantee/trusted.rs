mod borrows;
mod carriers;
mod check;
mod completion;
mod dependencies;
mod diagnostic;
mod observations;
mod owned;
mod proof;
mod transfers;

pub(in crate::analysis) use carriers::portable_path as portable_carrier_path;
pub(in crate::analysis) use check::{MemoryCallEvidence, TrustedChecks};
pub(in crate::analysis) use owned::OwnedReturnCondition;

pub(super) use borrows::project_place;

pub(in crate::analysis::guarantee) use carriers::result_projection as static_carrier;
