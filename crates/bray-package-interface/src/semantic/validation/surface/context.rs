use crate::{
    InterfaceSectionTag, InterfaceSemanticRecordKind, InterfaceValidationContext,
    InterfaceValidationError,
};

pub(super) fn semantic_record_error(
    error: InterfaceValidationError,
    kind: InterfaceSemanticRecordKind,
    index: usize,
) -> InterfaceValidationError {
    match error {
        InterfaceValidationError::Malformed {
            context:
                InterfaceValidationContext::Section(InterfaceSectionTag::SemanticRecordDirectory),
            cause,
        } => InterfaceValidationError::Malformed {
            context: InterfaceValidationContext::SemanticRecord {
                kind,
                index: super::super::saturating_u64(index),
            },
            cause,
        },
        error => error,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        InterfaceMalformedCause, InterfaceSemanticRecordKind, InterfaceValidationContext,
        InterfaceValidationError, InterfaceValidationField,
    };

    #[test]
    fn semantic_validation_failure_retains_record_kind_and_index() {
        let error = crate::semantic::codec::invalid_value(InterfaceValidationField::Reference);

        assert_eq!(
            super::semantic_record_error(error, InterfaceSemanticRecordKind::CallableSignature, 7,),
            InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::SemanticRecord {
                    kind: InterfaceSemanticRecordKind::CallableSignature,
                    index: 7,
                },
                cause: InterfaceMalformedCause::InvalidValue {
                    field: InterfaceValidationField::Reference,
                },
            }
        );
    }
}
