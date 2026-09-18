use bray_bound_tree::{
    BoundExpressionId, SemanticSelection, StorageIdentityId,
};
use bray_ir::{MirBlockId, MirOperand, MirOperationKind, MirStoreKind};
use bray_symbols::{StaticReferenceSelection, StaticSymbolId};

use super::super::LoweringError;
use super::super::lowerer::Lowerer;
use super::access::RootInitialization;
use crate::LoweringInput;

pub(super) fn static_reference(
    input: &LoweringInput<'_>,
    expression: BoundExpressionId,
    declaration: StaticSymbolId,
) -> StaticReferenceSelection {
    let mut pending = vec![expression];

    while let Some(candidate) = pending.pop() {
        if let Some(reference) = input
            .semantic_selections()
            .entries()
            .iter()
            .find_map(|entry| match entry.selection() {
                SemanticSelection::StaticReference(reference)
                    if entry.expression() == candidate
                        && reference.template().declaration() == declaration =>
                {
                    Some(reference.clone())
                }
                _ => None,
            })
        {
            return reference;
        }

        let node = input
            .unit()
            .view()
            .expression(candidate)
            .unwrap_or_else(|| {
                panic!("lowering contract violation: MissingExpression {candidate:?}")
            });

        let child_start = pending.len();

        pending.extend(node.child_expressions());
        pending[child_start..].reverse();
    }

    panic!("lowering contract violation: MissingSemanticSelection {expression:?}")
}
impl Lowerer<'_> {
    pub(super) fn initialize_access_storage(
        &mut self,
        identity: StorageIdentityId,
        current: MirBlockId,
        initial_value: Option<(BoundExpressionId, MirOperand)>,
        static_reference: Option<&StaticReferenceSelection>,
    ) -> Result<RootInitialization, LoweringError> {
        let root_type = self.storage_identity_type(identity);

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

        let place =
            self.place_for_identity_with_static(identity, root_type, origin, static_reference)?;

        if let Some((owner, value)) = initial_value {
            if !value.reads_from(&place) {
                self.push_operation(
                    current,
                    self.expression_source(owner),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Initialize,
                        destination: place.clone(),
                        value,
                    },
                    None,
                )?;
            }

            self.initialized_temporaries.insert(identity);
        }

        Ok(RootInitialization::Continuing {
            block: current,
            place,
        })
    }
}
