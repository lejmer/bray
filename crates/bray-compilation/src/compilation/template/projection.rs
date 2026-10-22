use bray_binder::BindingQueryContext;
use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateOperation, StorageProjection};
use bray_symbols::{
    ConstantBinaryOperation, ConstantTermData, ConstantValueData, ConstantValueKind, TypeId,
};

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn integer_node(
        &mut self,
        ty: TypeId,
        value: u64,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let value =
            self.context
                .semantic_values()
                .intern_constant_value(ConstantValueData::new(
                    ty,
                    ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(value)),
                ))?;

        let term = self
            .context
            .semantic_values()
            .intern_constant_term(ConstantTermData::Value(value))?;

        Ok(self.push_node(
            CheckedTemplateOperation::Constant {
                term,
                usage: Default::default(),
            },
            ty,
        ))
    }

    pub(super) fn value_projection(
        &mut self,
        subject: CheckedTemplateNodeId,
        projection: StorageProjection,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        Ok(match projection {
            StorageProjection::ProductField(field) => CheckedTemplateOperation::Project {
                subject,
                member: self.symbol_key(field.into())?,
            },
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                CheckedTemplateOperation::Project {
                    subject,
                    member: self.symbol_key(field.into())?,
                }
            }
            StorageProjection::TupleElement(index) => {
                CheckedTemplateOperation::TupleElement { subject, index }
            }
            StorageProjection::NullableValue => CheckedTemplateOperation::NullableValue { subject },
            StorageProjection::Element(index) => CheckedTemplateOperation::Index {
                call: None,
                subject,
                index: self.expression(index)?,
            },
            projection @ (StorageProjection::ElementFromStart(index)
            | StorageProjection::ElementFromEnd(index)) => {
                let ty = self
                    .context
                    .compilation()
                    .target_property_type(bray_target::TargetPropertyKind::ScalarUsize)?;

                let mut index = self.integer_node(ty, u64::from(index.raw()))?;

                if matches!(projection, StorageProjection::ElementFromEnd(_)) {
                    let source_type = self.nodes
                        [usize::try_from(subject.raw()).expect("checked subject fits host")]
                    .ty();

                    let source_type = self.context.semantic_values().unborrowed_type(source_type);
                    let data = self.context.semantic_values().type_data(source_type);

                    let bray_symbols::TypeData::Array { length, .. } = data.as_ref() else {
                        panic!("checked finite array projection retains its length");
                    };

                    let length = self.push_node(
                        CheckedTemplateOperation::Constant {
                            term: *length,
                            usage: Default::default(),
                        },
                        ty,
                    );

                    let remaining = self.push_node(
                        CheckedTemplateOperation::Binary {
                            operation: ConstantBinaryOperation::Subtract,
                            left: length,
                            right: index,
                        },
                        ty,
                    );

                    let one = self.integer_node(ty, 1)?;

                    index = self.push_node(
                        CheckedTemplateOperation::Binary {
                            operation: ConstantBinaryOperation::Subtract,
                            left: remaining,
                            right: one,
                        },
                        ty,
                    );
                }

                CheckedTemplateOperation::Index {
                    call: None,
                    subject,
                    index,
                }
            }
            StorageProjection::OwnedTarget
            | StorageProjection::UninitPayload
            | StorageProjection::SliceRange { .. } => {
                panic!("admitted value projection stays within its inline state");
            }
        })
    }
}
