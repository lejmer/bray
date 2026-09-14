use super::encoding::StructuralValueEncoder;
use crate::fact::FactQueryError;
use bray_symbols::{ConstantProjection, ConstantProjectionKind};

impl StructuralValueEncoder<'_, '_> {
    pub(super) fn constant_projection(
        &mut self,
        projection: ConstantProjection,
    ) -> Result<(), FactQueryError> {
        self.constant_term(projection.subject())?;

        match projection.kind() {
            ConstantProjectionKind::TupleElement(ordinal) => {
                self.tag(0);
                self.ordinal(ordinal);
            }
            ConstantProjectionKind::ArrayElement(index) => {
                self.tag(1);
                self.constant_term(index)?;
            }
            ConstantProjectionKind::ArraySlice { lower, upper } => {
                self.tag(5);

                for bound in [lower, upper] {
                    self.tag(u8::from(bound.is_some()));

                    if let Some(bound) = bound {
                        self.constant_term(bound)?;
                    }
                }
            }
            ConstantProjectionKind::ProductField(field) => {
                self.tag(2);
                self.symbol(field.into())?;
            }
            ConstantProjectionKind::UnionPayloadField(field) => {
                self.tag(3);
                self.symbol(field.into())?;
            }
            ConstantProjectionKind::NullableValue => self.tag(4),
        }

        Ok(())
    }
}
