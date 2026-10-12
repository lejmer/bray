mod borrows;
mod check;
mod completion;
mod proof;
mod transfers;

pub(in crate::analysis) use check::TrustedChecks;

pub(super) use borrows::project_place;
