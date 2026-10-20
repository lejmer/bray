use super::core::{PredicateLowerer, PredicateLoweringContext};
use bray_bound_tree::CheckedTemplateOperation;
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirImmediateValue, MirOperand, MirOperationKind,
    MirProjectionKind,
};
use bray_symbols::{TypeData, TypeId};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn custom_index(
        &mut self,
        mut block: MirBlockId,
        operation: &CheckedTemplateOperation,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (call, subject) = match operation {
            CheckedTemplateOperation::Index {
                call: Some(call),
                subject,
                ..
            }
            | CheckedTemplateOperation::Slice {
                call: Some(call),
                subject,
                ..
            } => (call, *subject),
            _ => unreachable!("custom access retains its selected index contract"),
        };

        let (continued, subject) = self.node(block, subject)?;

        block = continued;

        let place = self.observed_place(block, subject)?;

        let borrowed = self
            .context
            .semantic_values()
            .intern_type(TypeData::Borrow {
                kind: call.borrow_kind,
                target: place.ty(),
            })
            .map_err(crate::SyntheticLoweringError::SemanticValue)?;

        let receiver = self.value(
            block,
            MirOperationKind::Borrow {
                kind: call.borrow_kind,
                place,
            },
            borrowed,
        )?;

        let mut inputs = vec![receiver];

        match operation {
            CheckedTemplateOperation::Index { index, .. } => {
                let (continued, index) = self.node(block, *index)?;

                block = continued;
                inputs.push(index);
            }
            CheckedTemplateOperation::Slice { lower, upper, .. } => {
                let nullable = self.context.predicate_index_bound_type(call)?;

                let TypeData::Nullable(element) =
                    *self.context.semantic_values().type_data(nullable)
                else {
                    panic!("checked slice bounds are nullable values");
                };

                for bound in [lower, upper] {
                    let value = if let Some(bound) = bound {
                        let (continued, value) = self.node(block, *bound)?;

                        block = continued;

                        let value = self.convert(block, value, element)?;

                        self.value(
                            block,
                            MirOperationKind::Aggregate(MirAggregate::new(
                                MirAggregateKind::NullablePresent,
                                [value],
                            )),
                            nullable,
                        )?
                    } else {
                        MirOperand::Immediate {
                            value: MirImmediateValue::NullableAbsent,
                            ty: nullable,
                        }
                    };

                    inputs.push(value);
                }
            }
            _ => unreachable!("custom access dispatch retains an index or slice"),
        }

        let selected = self.context.predicate_index_call(call, inputs)?;

        let (block, value) = self.call(block, selected)?;

        let place = self.place(block, value)?;

        Ok((
            block,
            MirOperand::Copy(place.project(MirProjectionKind::Dereference, ty)),
        ))
    }
}
