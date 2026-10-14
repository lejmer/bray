mod borrows;
mod check;
mod carriers;
mod completion;
mod proof;
mod transfers;

pub(in crate::analysis) use check::TrustedChecks;

pub(super) use borrows::project_place;

pub(in crate::analysis::guarantee) use carriers::result_projection as static_carrier;
