use bray_bound_tree::{
    BoundExpressionId, SemanticSelection, StorageAccessId, StorageIdentityId,
};
use bray_ir::{MirBlockId, MirOperand, MirOperationKind, MirStoreKind};
use bray_symbols::{StaticReferenceSelection, StaticSymbolId};

use super::super::LoweringError;
use super::super::lowerer::Lowerer;
use super::access::RootInitialization;
use crate::LoweringInput;

pub(super) fn static_reference(
    input: &LoweringInput<'_>,
    access: StorageAccessId,
    expression: BoundExpressionId,
    declaration: StaticSymbolId,
) -> Result<StaticReferenceSelection, LoweringError> {
    let source = input
        .storage_plan()
        .access(access)
        .ok_or(LoweringError::MissingStorageAccessRecord(access))?
        .source();

    input
        .semantic_selections()
        .entries()
        .iter()
        .filter(|entry| {
            entry.expression() == expression
                || input
                    .unit()
                    .view()
                    .expression(entry.expression())
                    .is_some_and(|candidate| {
                        let candidate = candidate.origin().source_anchor();

                        candidate.source_version() == source.source_version()
                            && candidate.syntax().source_id() == source.syntax().source_id()
                            && source
                                .syntax()
                                .full_range()
                                .contains_range(candidate.syntax().full_range())
                    })
        })
        .find_map(|entry| match entry.selection() {
            SemanticSelection::StaticReference(reference)
                if reference.template().declaration() == declaration =>
            {
                // The MIR storage and access table own this immutable semantic selection.
                Some(reference.clone())
            }
            _ => None,
        })
        .ok_or(LoweringError::MissingSemanticSelection(expression))
}

impl Lowerer<'_> {
    pub(super) fn initialize_access_storage(
        &mut self,
        identity: StorageIdentityId,
        current: MirBlockId,
        initial_value: Option<(BoundExpressionId, MirOperand)>,
        static_reference: Option<&StaticReferenceSelection>,
    ) -> Result<RootInitialization, LoweringError> {
        let root_type = self.storage_identity_type(identity)?;

        let origin = initial_value.as_ref().map_or_else(
            || bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source()),
            |(owner, _)| {
                self.input
                    .unit()
                    .view()
                    .expression(*owner)
                    .map(|expression| expression.origin())
                    .unwrap_or_else(|| {
                        bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source())
                    })
            },
        );

        let place = self.place_for_identity_with_static(
            identity,
            root_type,
            origin,
            static_reference,
        )?;

        if let Some((owner, value)) = initial_value
            && !value.reads_from(&place)
        {
            self.builder.push_operation(
                current,
                self.expression_source(owner)?,
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: place.clone(),
                    value,
                },
                None,
            )?;
        }

        Ok(RootInitialization::Continuing {
            block: current,
            storage: place.storage(),
        })
    }
}
