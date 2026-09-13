use std::collections::BTreeMap;

use bray_symbols::SymbolKind;

use crate::{
    AnyBoundNodeId, BoundBlockId, BoundExpression, BoundExpressionId, BoundPatternId, BoundUnit,
    BoundUnitView, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, StorageIdentity,
    StorageIdentityId, StoragePlan, walk_bound_unit_view,
};

/// A malformed lexical storage-scope map.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageScopeBuildError {
    /// Bound traversal ended with a mismatched scope stack.
    UnbalancedScopes { open_scope: Option<BoundBlockId> },
    /// A referenced pattern is absent from the bound unit.
    MissingPattern { pattern: BoundPatternId },
}

/// Returns whether one identity transfers through its checked unit boundary.
pub fn storage_identity_transfers_at_unit_exit(
    storage: &StoragePlan,
    identity: StorageIdentityId,
) -> bool {
    match storage.identity(identity) {
        Some(StorageIdentity::Result(_)) => true,
        Some(
            StorageIdentity::Receiver(_)
            | StorageIdentity::LocalOwned(_)
            | StorageIdentity::Static(_)
            | StorageIdentity::Parameter(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::PredicateParameter(_)
            | StorageIdentity::PostconditionResult(_)
            | StorageIdentity::Temporary(_)
            | StorageIdentity::CustomIndexBorrow(_)
            | StorageIdentity::IterationCursor(_)
            | StorageIdentity::IterationElement(_)
            | StorageIdentity::Allocation(_)
            | StorageIdentity::CompilerCreated(_)
            | StorageIdentity::Alternative { .. }
            | StorageIdentity::Error(_),
        )
        | None => false,
    }
}

/// Returns whether the checked body owns the remaining represented parts of its destructor receiver.
pub fn storage_identity_is_destructor_receiver(
    unit: &BoundUnit,
    storage: &StoragePlan,
    identity: StorageIdentityId,
) -> bool {
    matches!(
        storage.identity(identity),
        Some(StorageIdentity::Receiver(_))
    ) && unit.key().kind() == crate::BoundUnitKind::CallableBody
        && unit.key().declared_owner().kind() == SymbolKind::Destructor
}

/// Lexical owner scopes derived from one complete bound unit.
pub struct StorageScopeOwners {
    nodes: BTreeMap<AnyBoundNodeId, BoundBlockId>,
    root: Option<BoundBlockId>,
    unit: crate::BoundUnitKind,
}

impl StorageScopeOwners {
    /// Derives lexical storage owners from the unit's bound-tree structure.
    pub fn collect(unit: &BoundUnit) -> Result<Self, StorageScopeBuildError> {
        let mut nodes = BTreeMap::new();
        let mut scopes = Vec::new();
        let mut root = None;

        let outcome = walk_bound_unit_view(unit.view(), unit.root(), |event| {
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
            return Err(StorageScopeBuildError::UnbalancedScopes {
                open_scope: scopes.last().copied(),
            });
        }

        for (_, expression) in unit.tree().expressions() {
            match expression {
                BoundExpression::Match(expression) => {
                    for arm in expression.arms() {
                        assign_pattern_scope(unit.view(), arm.pattern(), arm.body(), &mut nodes)?;
                    }
                }
                BoundExpression::For(expression) => assign_pattern_scope(
                    unit.view(),
                    expression.pattern(),
                    expression.body(),
                    &mut nodes,
                )?,
                BoundExpression::Generator(expression) => assign_pattern_scope(
                    unit.view(),
                    expression.pattern(),
                    expression.body(),
                    &mut nodes,
                )?,
                BoundExpression::Structured(expression) => {
                    if matches!(
                        expression.kind(),
                        crate::BoundStructuredExpressionKind::PatternTest
                            | crate::BoundStructuredExpressionKind::Condition
                    ) && let ([subject], [scope]) = (expression.operands(), expression.blocks())
                    {
                        assign_expression_scope(unit.view(), *subject, *scope, &mut nodes);
                    }

                    if expression.kind() == crate::BoundStructuredExpressionKind::Condition
                        && let ([condition], [scope]) = (expression.operands(), expression.blocks())
                    {
                        assign_condition_bindings(unit.view(), *condition, *scope, &mut nodes)?;
                    }
                }
                BoundExpression::Block(_)
                | BoundExpression::Literal(_)
                | BoundExpression::Name(_)
                | BoundExpression::PatternReference(_)
                | BoundExpression::UnresolvedReference(_)
                | BoundExpression::Unary(_)
                | BoundExpression::Binary(_)
                | BoundExpression::Assignment(_)
                | BoundExpression::Call(_)
                | BoundExpression::BoxConstruction(_)
                | BoundExpression::ErrorCall(_)
                | BoundExpression::Conversion(_)
                | BoundExpression::ErrorConversion(_)
                | BoundExpression::AnonymousCallable(_)
                | BoundExpression::Await(_)
                | BoundExpression::StructConstruction(_)
                | BoundExpression::MemberAccess(_)
                | BoundExpression::LeadingDotVariant(_)
                | BoundExpression::UnqualifiedVariant(_)
                | BoundExpression::TraitQualifiedMember(_)
                | BoundExpression::ControlTransfer(_)
                | BoundExpression::Error(_) => {}
            }
        }

        Ok(Self {
            nodes,
            root,
            unit: unit.key().kind(),
        })
    }

    /// Returns the lexical owner for one storage identity.
    pub fn scope(&self, identity: Option<StorageIdentity>) -> Option<BoundBlockId> {
        match identity {
            Some(StorageIdentity::Static(_)) | None => None,
            Some(identity) if identity.is_borrowed_provider_input(self.unit) => None,
            Some(identity) => identity
                .definition_node()
                .and_then(|node| self.nodes.get(&node).copied())
                .or(self.root),
        }
    }

    /// Returns the lexical owner for one identity in a storage plan.
    pub fn identity_scope(
        &self,
        storage: &StoragePlan,
        identity: StorageIdentityId,
    ) -> Option<BoundBlockId> {
        self.scope(storage.identity(identity))
    }
}

fn assign_expression_scope(
    view: BoundUnitView<'_>,
    expression: BoundExpressionId,
    scope: BoundBlockId,
    nodes: &mut BTreeMap<AnyBoundNodeId, BoundBlockId>,
) {
    let original = nodes.get(&expression.into()).copied();
    let mut pending = vec![expression];

    while let Some(expression) = pending.pop() {
        if nodes.get(&expression.into()).copied() != original {
            continue;
        }

        nodes.insert(expression.into(), scope);

        if let Some(expression) = view.expression(expression) {
            pending.extend(expression.child_expressions());
        }
    }
}

fn assign_condition_bindings(
    view: BoundUnitView<'_>,
    condition: BoundExpressionId,
    scope: BoundBlockId,
    nodes: &mut BTreeMap<AnyBoundNodeId, BoundBlockId>,
) -> Result<(), StorageScopeBuildError> {
    let mut pending = vec![condition];

    while let Some(condition) = pending.pop() {
        match view.expression(condition) {
            Some(BoundExpression::Binary(binary))
                if binary.operator() == crate::BoundOperator::LogicalAnd =>
            {
                pending.extend(binary.operands());
            }
            Some(BoundExpression::Structured(binding))
                if binding.kind() == crate::BoundStructuredExpressionKind::PatternBinding =>
            {
                for pattern in binding.patterns() {
                    assign_pattern_scope(view, *pattern, scope, nodes)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn assign_pattern_scope(
    view: BoundUnitView<'_>,
    pattern: BoundPatternId,
    scope: BoundBlockId,
    nodes: &mut BTreeMap<AnyBoundNodeId, BoundBlockId>,
) -> Result<(), StorageScopeBuildError> {
    let mut pending = vec![pattern];

    while let Some(pattern) = pending.pop() {
        let pattern_node = view
            .pattern(pattern)
            .ok_or(StorageScopeBuildError::MissingPattern { pattern })?;

        nodes.insert(pattern.into(), scope);
        pending.extend(pattern_node.children().iter().copied());
    }

    Ok(())
}
