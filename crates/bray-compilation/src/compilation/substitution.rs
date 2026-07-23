use bray_symbols::{AnySymbolId, GenericOwnerId, GenericSubstitutionData, GenericSubstitutionId};

use crate::fact::FactQueryError;

pub(super) fn empty_substitution(
    values: &bray_symbols::SemanticValueStore,
    definition: AnySymbolId,
) -> Result<GenericSubstitutionId, FactQueryError> {
    let owner = GenericOwnerId::try_new(definition).ok_or(FactQueryError::InfrastructureFailure)?;

    let substitution = GenericSubstitutionData::try_new(owner, [], [])
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    values
        .intern_generic_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}
