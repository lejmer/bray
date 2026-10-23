use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_ir::{
    MirBlockId, MirFieldReference, MirOperand, MirOperationKind, MirPlace, MirProjectionKind,
    MirStoreKind,
};
use bray_symbols::AnySymbolId;

use super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn store(
        &mut self,
        block: MirBlockId,
        kind: MirStoreKind,
        destination: MirPlace,
        value: MirOperand,
    ) -> Result<(), C::Error> {
        let operation = self.local_operation(
            block,
            MirOperationKind::Store {
                kind,
                destination,
                value,
            },
        )?;

        self.builder
            .push_operation(block, self.source.clone(), operation, None)
            .map_err(capacity)?;

        Ok(())
    }

    pub(super) fn materialized(
        &mut self,
        block: MirBlockId,
        node: CheckedTemplateNodeId,
        temporary: bray_bound_tree::CheckedTemplateTemporaryId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let index = usize::try_from(temporary.raw()).expect("checked temporary fits host");

        let canonical =
            self.temporary_nodes[index].expect("materialized temporary retains its read node");

        if canonical != node {
            return self.node(block, canonical);
        }

        let temporary_data = self.template.temporaries()[index];
        let initializer = temporary_data.initializer();
        let ty = self.context.predicate_type(temporary_data.ty())?;

        let (block, value) = self.node(block, initializer)?;

        let value = self.convert(block, value, ty)?;

        let place = if let Some(place) = self.temporary_homes.get(&temporary) {
            place.clone()
        } else {
            let storage = self
                .builder
                .push_storage(self.source.clone(), bray_ir::MirStorageKind::Temporary, ty)
                .map_err(capacity)?;

            let place = MirPlace::new(storage, [], ty);

            self.private_homes.insert(storage);
            self.temporary_homes.insert(temporary, place.clone());

            place
        };

        self.store(block, MirStoreKind::Initialize, place.clone(), value)?;

        Ok((block, MirOperand::Copy(place)))
    }

    pub(super) fn update(
        &mut self,
        block: MirBlockId,
        subject: CheckedTemplateNodeId,
        base: CheckedTemplateNodeId,
        component: CheckedTemplateNodeId,
        value: CheckedTemplateNodeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (block, _) = self.node(block, base)?;

        let (block, _) = self.node(block, component)?;

        let (block, value) = self.node(block, value)?;

        let (block, subject) = self.node(block, subject)?;

        let subject = self.private_owner(block, subject)?;

        let (block, destination) = self.component_place(block, subject.clone(), base, component)?;

        if value != MirOperand::Copy(destination.clone()) {
            self.store(block, MirStoreKind::Assign, destination, value)?;
        }

        Ok((block, MirOperand::Copy(subject)))
    }

    pub(super) fn private_owner(
        &mut self,
        block: MirBlockId,
        subject: MirOperand,
    ) -> Result<MirPlace, C::Error> {
        let subject = match subject {
            MirOperand::Copy(place) | MirOperand::Move(place)
                if place.projections().is_empty()
                    && self.private_homes.contains(&place.storage()) =>
            {
                place
            }
            subject => {
                // Copy a readonly source once, then retain this private home for later writes.
                let ty = self.builder.operand_type(&subject);

                let snapshot = self.value(
                    block,
                    MirOperationKind::Convert {
                        operand: subject,
                        conversion: bray_bound_tree::SelectedConversion::new(
                            ty,
                            ty,
                            bray_bound_tree::ConversionTarget::Identity,
                        ),
                    },
                    ty,
                )?;

                self.place(block, snapshot)?
            }
        };

        Ok(subject)
    }

    fn component_place(
        &mut self,
        block: MirBlockId,
        root: MirPlace,
        base: CheckedTemplateNodeId,
        component: CheckedTemplateNodeId,
    ) -> Result<(MirBlockId, MirPlace), C::Error> {
        if component == base {
            return Ok((block, root));
        }

        let node = &self.template.nodes()
            [usize::try_from(component.raw()).expect("checked component fits host")];

        let ty = self.context.predicate_type(node.ty())?;
        let operation = node.operation().clone();

        let parent = operation
            .inline_projection_subject()
            .expect("checked update has inline components");

        let (block, parent) = self.component_place(block, root, base, parent)?;

        let (block, projection) = match operation {
            CheckedTemplateOperation::Project { member, .. } => {
                let field = match self.context.predicate_symbol(&member)? {
                    AnySymbolId::StructField(field) => MirFieldReference::Struct(field),
                    AnySymbolId::UnionPayloadField(field) => MirFieldReference::UnionPayload(field),
                    symbol => panic!("checked update retains its field: {symbol:?}"),
                };

                (block, MirProjectionKind::Field(field))
            }
            CheckedTemplateOperation::TupleElement { index, .. } => {
                (block, MirProjectionKind::TupleField(index.raw()))
            }
            CheckedTemplateOperation::NullableValue { .. } => {
                (block, MirProjectionKind::NullableValue)
            }
            CheckedTemplateOperation::Index {
                index, call: None, ..
            } => {
                let (block, index) = self.node(block, index)?;

                (block, MirProjectionKind::Index(index))
            }
            _ => unreachable!("checked component has an inline projection"),
        };

        Ok((block, parent.project(projection, ty)))
    }
}
