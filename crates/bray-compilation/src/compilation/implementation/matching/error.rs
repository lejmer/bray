use bray_symbols::ImplementationSymbolId;

use super::ImplementationMatchError;
use crate::compilation::SemanticQueryFailure;
use crate::fact::FactQueryError;

pub(in crate::compilation) fn implementation_match_query_error(
    implementation: ImplementationSymbolId,
    error: ImplementationMatchError,
) -> FactQueryError {
    match error {
        ImplementationMatchError::SemanticValue(error) => FactQueryError::SemanticValueStore(error),
        cause @ ImplementationMatchError::InvalidSubstitution(_) => {
            SemanticQueryFailure::ImplementationMatch {
                implementation: Some(implementation),
                cause,
            }
            .into()
        }
    }
}
