use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirEdge, MirOperand, MirOperationKind, MirPatternPredicate,
    MirPlace, MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::SymbolOrdinal;

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_outcome::CleanupOutcome;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn retain_finalizer_error(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        result: MirOperand,
        outcome: &CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let invalid = |cause| self.mir_error(source, cause);
        let ty = builder.operand_type(&result).map_err(invalid)?;
        let unit = self.context.representation_type(RepresentationRole::Unit)?;

        if ty == unit {
            return Ok(block);
        }

        let symbols = self.context.compiler_known_symbols();

        let [success, error] = symbols
            .representation_type_arguments(
                self.context.semantic_values(),
                RepresentationRole::Result,
                ty,
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .ok_or(SyntheticLoweringError::UnresolvedType(ty))?;

        if success != unit {
            return Err(SyntheticLoweringError::UnsupportedType(ty).into());
        }

        let representation = symbols.result_representation().ok_or(
            SyntheticLoweringError::MissingRepresentation {
                role: RepresentationRole::Result,
                argument: None,
            },
        )?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .map_err(invalid)?;

        let result_place = MirPlace::new(storage, [], ty);

        // The branch reads the tag before transferring the same owned result's error payload.
        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: result_place.clone(),
                value: result,
            },
        )?;

        let kind = builder.block_kind(block).map_err(invalid)?;
        let failed = builder.push_block(source.clone(), kind).map_err(invalid)?;
        let finished = builder.push_block(source.clone(), kind).map_err(invalid)?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::PatternBranch {
                    subject: MirOperand::Copy(result_place.clone()),
                    predicate: MirPatternPredicate::ActiveUnionVariant(
                        representation.error_variant(),
                    ),
                    matched: MirEdge::new(failed, []),
                    unmatched: MirEdge::new(finished, []),
                },
            )
            .map_err(invalid)?;

        let payload = result_place.project(
            MirProjectionKind::ActiveUnionPayloadElement {
                variant: representation.error_variant(),
                ordinal: SymbolOrdinal::new(0),
            },
            error,
        );

        let runtime_abi = builder.target().runtime_abi();
        let quiescence = self.cleanup_outcome(builder, failed, source)?;

        let failed = self.resolve_lifecycle_action(
            builder,
            failed,
            source,
            MirOperationKind::Abandon {
                action: bray_ir::MirAbandonmentAction::Quiesce,
                place: payload.clone(),
            },
            &quiescence,
        )?;

        // Retain the original Error after its runs settle, before forwarding their failures.
        self.push_lifecycle_operation(
            builder,
            failed,
            source,
            MirOperationKind::Async(MirAsyncOperation::TransferCleanupIncident {
                incident: MirOperand::Move(payload),
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::CleanupIncidentTransfer,
                    runtime_abi,
                ),
            }),
        )?;

        let failed = outcome.check(builder, failed, source).map_err(invalid)?;

        let failed = quiescence
            .retain_into(builder, failed, source, outcome)
            .map_err(invalid)?;

        builder
            .set_terminator(
                failed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(invalid)?;

        Ok(finished)
    }
}
