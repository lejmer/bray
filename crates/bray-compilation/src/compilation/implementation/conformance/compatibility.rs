mod check;
mod types;

pub(super) use check::{
    CompatibilityContext, fulfillment_is_compatible, subject_lifecycle_is_compatible,
};
