use bray_ir::{
    MirBlockId, MirEdge, MirOperand, MirOperationKind, MirPatternPredicate, MirPlace,
    MirProjectionKind, MirSourceAnchor, MirTerminatorKind, MirValueId,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::LoweringError;
use super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn guard_cleanup_payloads(
        &mut self,
        mut block: MirBlockId,
        continuation: MirBlockId,
        source: &MirSourceAnchor,
        place: &MirPlace,
        mut value: Option<(MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        for (index, projection) in place.projections().iter().enumerate() {
            let predicate = match projection.kind() {
                MirProjectionKind::NullableValue => MirPatternPredicate::NullablePresent,
                MirProjectionKind::ActiveUnionPayloadField { variant, .. }
                | MirProjectionKind::ActiveUnionPayloadElement { variant, .. } => {
                    MirPatternPredicate::ActiveUnionVariant(*variant)
                }
                _ => continue,
            };

            let matched = self.builder.push_block(
                Self::retained_source(source),
                self.builder.block_kind(block),
            )?;

            let forwarded = value.map(|(value, ty)| (MirOperand::Value(value), ty));
            let matched_value = self.cleanup_parameter(matched, source, forwarded.as_ref())?;

            let subject = MirPlace::new(
                place.storage(),
                place.projections().iter().take(index).cloned(),
                projection.source_type(),
            );

            let subject = if matches!(predicate, MirPatternPredicate::ActiveUnionVariant(_)) {
                let ty = self.input.semantic_values().intern_type(TypeData::Borrow {
                    kind: BorrowKind::Shared,
                    target: subject.ty(),
                })?;

                let commit = self.push_operation(
                    block,
                    Self::retained_source(source),
                    MirOperationKind::Borrow {
                        kind: BorrowKind::Shared,
                        place: subject,
                    },
                    Some(ty),
                )?;

                MirOperand::Value(
                    commit
                        .result()
                        .expect("value-producing MIR operation must publish a result"),
                )
            } else {
                MirOperand::Copy(subject)
            };

            self.set_terminator(
                block,
                Self::retained_source(source),
                MirTerminatorKind::PatternBranch {
                    subject,
                    predicate,
                    matched: MirEdge::new(
                        matched,
                        value.map(|(value, _)| MirOperand::Value(value)),
                    ),
                    unmatched: MirEdge::new(
                        continuation,
                        value.map(|(value, _)| MirOperand::Value(value)),
                    ),
                },
            )?;

            block = matched;
            value = matched_value.zip(value.map(|(_, ty)| ty));
        }

        Ok((block, value))
    }
}
