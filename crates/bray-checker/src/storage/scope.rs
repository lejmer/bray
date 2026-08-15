use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundWalkControl,
    BoundWalkEvent, BoundWalkOutcome, StorageBinding, StorageBindingTarget, StorageIdentity,
    StorageIdentityId, StoragePlan, walk_bound_unit_view,
};

use crate::{CheckerFactError, CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(crate) struct StorageScopeOwners {
    nodes: BTreeMap<AnyBoundNodeId, BoundBlockId>,
    root: Option<BoundBlockId>,
}

impl StorageScopeOwners {
    pub(crate) fn collect<C>(request: CheckerUnitView<'_, C>) -> Result<Self, CheckerFactError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let mut nodes = BTreeMap::new();
        let mut scopes = Vec::new();
        let mut root = None;

        let outcome = walk_bound_unit_view(request.view(), request.unit().root(), |event| {
            match event {
                BoundWalkEvent::Enter(AnyBoundNodeId::Block(block)) => {
                    root.get_or_insert(block);
                    scopes.push(block);
                    nodes.insert(block.into(), block);
                }
                BoundWalkEvent::Enter(node) => {
                    if let Some(scope) = scopes.last().copied() {
                        nodes.insert(node, scope);
                    }
                }
                BoundWalkEvent::Exit(AnyBoundNodeId::Block(block)) => {
                    if scopes.pop() != Some(block) {
                        return BoundWalkControl::Stop;
                    }
                }
                BoundWalkEvent::Exit(_) => {}
            }

            BoundWalkControl::Continue
        });

        if outcome != BoundWalkOutcome::Completed || !scopes.is_empty() {
            return Err(CheckerFactError::Infrastructure(
                CheckerInfrastructureError::InvalidStorageFlow,
            ));
        }

        Ok(Self { nodes, root })
    }

    pub(crate) fn scope(&self, identity: Option<StorageIdentity>) -> Option<BoundBlockId> {
        identity.and_then(|identity| {
            identity
                .definition_node()
                .and_then(|node| self.nodes.get(&node).copied())
                .or(self.root)
        })
    }

    pub(crate) fn identity_scope(
        &self,
        storage: &bray_bound_tree::StoragePlan,
        identity: StorageIdentityId,
    ) -> Option<BoundBlockId> {
        self.scope(storage.identity(identity))
    }
}

pub(crate) fn local_initialization_bindings<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, Vec<StorageBinding>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut result = BTreeMap::new();

    for (_, block) in request.unit().tree().blocks() {
        for item in block.items() {
            let BoundBlockItem::LocalBinding(binding) = item else {
                continue;
            };

            let bindings = binding
                .bindings()
                .iter()
                .filter_map(|binding| storage.binding(StorageBindingTarget::Local(*binding)))
                .collect::<Vec<_>>();

            if !bindings.is_empty() {
                result.insert(binding.initializer(), bindings);
            }
        }
    }

    result
}

pub(crate) fn local_initialization_destinations<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, StorageIdentityId>
where
    C: CheckerRequestContext + ?Sized,
{
    local_initialization_bindings(request, storage)
        .into_iter()
        .filter_map(|(initializer, bindings)| match bindings.as_slice() {
            [StorageBinding::Identity(destination)] => Some((initializer, *destination)),
            [StorageBinding::Access(_)] | [] | [_, _, ..] => None,
        })
        .collect()
}
