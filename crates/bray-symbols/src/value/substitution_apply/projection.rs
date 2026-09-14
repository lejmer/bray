use super::super::{
    ConstantProjection, ConstantProjectionKind, GenericSubstitutionData, SemanticValueStore,
    SemanticValueStoreError,
};

impl SemanticValueStore {
    pub(super) fn substitute_constant_projection(
        &self,
        projection: ConstantProjection,
        substitution: &GenericSubstitutionData,
    ) -> Result<ConstantProjection, SemanticValueStoreError> {
        let kind = match projection.kind() {
            ConstantProjectionKind::ArrayElement(index) => ConstantProjectionKind::ArrayElement(
                self.substitute_constant_term_data(index, substitution)?,
            ),
            ConstantProjectionKind::ArraySlice { lower, upper } => {
                ConstantProjectionKind::ArraySlice {
                    lower: lower
                        .map(|bound| self.substitute_constant_term_data(bound, substitution))
                        .transpose()?,
                    upper: upper
                        .map(|bound| self.substitute_constant_term_data(bound, substitution))
                        .transpose()?,
                }
            }
            kind => kind,
        };

        Ok(ConstantProjection::new(
            self.substitute_constant_term_data(projection.subject(), substitution)?,
            kind,
        ))
    }
}
