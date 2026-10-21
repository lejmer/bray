use bray_bound_tree::{CheckedMemoryOperationKind, CheckedTemplateNodeId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirBlockKind, MirEdge, MirMemoryOperation, MirOperand,
    MirOperationKind, MirPlace, MirProjectionKind, MirStorageKind, MirStoreKind, MirTerminatorKind,
};
use bray_symbols::{TypeData, TypeId};

use super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn array_transform(
        &mut self,
        block: MirBlockId,
        subject: CheckedTemplateNodeId,
        element: CheckedTemplateNodeId,
        converted: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (block, subject) = self.node(block, subject)?;
        let input = self.observed_place(block, subject)?;
        let source_type = self.context.semantic_values().type_data(input.ty());
        let target_type = self.context.semantic_values().type_data(ty);
        let (TypeData::Array { element: source_element, .. },
             TypeData::Array { element: target_element, .. }) =
            (source_type.as_ref(), target_type.as_ref())
        else {
            panic!("checked array transform retains its source and result array types");
        };
        let (source_element, target_element) = (*source_element, *target_element);
        let integer = self.context.representation_type(RepresentationRole::ScalarUsize)?;
        let length = self.value(block, MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
            CheckedMemoryOperationKind::SequenceLength,
            [MirOperand::Copy(input.clone())], [input.ty()], Some(integer),
        ))), integer)?;
        let output = self.builder.push_storage(self.source.clone(), MirStorageKind::Temporary, ty)
            .map_err(capacity)?;
        let output = MirPlace::new(output, [], ty);
        let header = self.builder.push_block(self.source.clone(), MirBlockKind::Ordinary).map_err(capacity)?;
        let body = self.builder.push_block(self.source.clone(), MirBlockKind::Ordinary).map_err(capacity)?;
        let completed = self.builder.push_block(self.source.clone(), MirBlockKind::Ordinary).map_err(capacity)?;
        let index = self.parameter(header, integer)?;
        let zero = self.integer(integer, 0)?;
        self.terminate(block, MirTerminatorKind::Goto(MirEdge::new(header, [zero])))?;
        let nonempty = self.machine(header, MirBinaryOperator::LessThan,
            MirOperand::Value(index), length, self.boolean)?;
        self.terminate(header, MirTerminatorKind::Branch {
            condition: nonempty,
            then_edge: MirEdge::new(body, []),
            else_edge: MirEdge::new(completed, []),
        })?;
        let retained = self.inserted.len();
        let source = input.project(MirProjectionKind::Index(MirOperand::Value(index)), source_element);
        self.values.insert(element, MirOperand::Move(source));
        self.inserted.push(element);
        let (body, value) = self.node(body, converted)?;
        let destination = output.clone().project(
            MirProjectionKind::Index(MirOperand::Value(index)), target_element);
        let operation = self.local_operation(body, MirOperationKind::Store {
            kind: MirStoreKind::Initialize, destination, value,
        })?;
        self.builder.push_operation(body, self.source.clone(), operation, None).map_err(capacity)?;
        self.rewind_values(retained);
        let one = self.integer(integer, 1)?;
        let next = self.machine(body, MirBinaryOperator::Add, MirOperand::Value(index), one, integer)?;
        self.terminate(body, MirTerminatorKind::Goto(MirEdge::new(header, [next])))?;
        Ok((completed, MirOperand::Move(output)))
    }
}
