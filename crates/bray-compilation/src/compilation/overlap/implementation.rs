use bray_symbols::{SemanticValueStore, SemanticValueStoreError};

use super::semantic::SemanticUnifier;
use crate::compilation::implementation::ImplementationHeader;

pub(in crate::compilation) fn implementation_headers_overlap(
    left: &ImplementationHeader,
    right: &ImplementationHeader,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    let mut unifier = SemanticUnifier::new(left.parameters(), right.parameters(), values);

    if !unifier.types_may_overlap(left.subject(), right.subject())? {
        return Ok(false);
    }

    unifier.trait_applications_may_overlap(left.trait_application(), right.trait_application())
}

pub(in crate::compilation) fn implementation_subjects_overlap(
    left: &ImplementationHeader,
    right: &ImplementationHeader,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    SemanticUnifier::new(left.parameters(), right.parameters(), values)
        .types_may_overlap(left.subject(), right.subject())
}
