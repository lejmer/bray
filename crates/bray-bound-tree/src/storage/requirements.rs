use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::AnyLocalSymbolId;

use super::{
    CheckedStorageFactsBuildError, StorageAccessId, StorageAccessOccurrence, StorageParameter,
    SurfaceStorageSymbol,
};
use crate::{
    AnyBoundNodeId, BoundExpression, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnitId, BoundUnitKind, BoundUnitView,
};

pub(super) fn validate_required_occurrences(
    view: BoundUnitView<'_>,
    root: AnyBoundNodeId,
    unit: BoundUnitId,
    kind: BoundUnitKind,
    occurrences: &BTreeMap<StorageAccessOccurrence, StorageAccessId>,
) -> Result<(), CheckedStorageFactsBuildError> {
    if view.unit() != unit || view.kind() != kind || root.unit() != unit {
        return Err(CheckedStorageFactsBuildError::UnitViewMismatch);
    }

    let mut pending = vec![root];
    let mut visited = BTreeSet::new();

    while let Some(node) = pending.pop() {
        if !visited.insert(node) {
            continue;
        }

        match node {
            AnyBoundNodeId::Expression(id) => {
                let expression = view
                    .expression(id)
                    .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node })?;

                validate_expression_occurrences(id, expression, occurrences)?;
                pending.extend(expression.child_expressions().map(AnyBoundNodeId::from));
                pending.extend(expression.child_patterns().map(AnyBoundNodeId::from));
                pending.extend(expression.child_blocks().map(AnyBoundNodeId::from));
            }
            AnyBoundNodeId::Pattern(id) => {
                let pattern = view
                    .pattern(id)
                    .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node })?;

                for binding in pattern.bindings() {
                    require(occurrences, StorageAccessOccurrence::Binding(*binding))?;
                }

                pending.extend(pattern.children().iter().copied().map(AnyBoundNodeId::from));
            }
            AnyBoundNodeId::Block(id) => {
                let block = view
                    .block(id)
                    .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node })?;

                for item in block.items() {
                    pending.extend(item.expression().map(AnyBoundNodeId::from));
                    pending.extend(item.pattern().map(AnyBoundNodeId::from));
                }
            }
            AnyBoundNodeId::CallableBody(id) => {
                let body = view
                    .callable_body(id)
                    .ok_or(CheckedStorageFactsBuildError::MissingBoundNode { node })?;

                pending.extend(body.block_id().map(AnyBoundNodeId::from));
            }
        }
    }

    Ok(())
}

fn validate_expression_occurrences(
    id: crate::BoundExpressionId,
    expression: &BoundExpression,
    occurrences: &BTreeMap<StorageAccessOccurrence, StorageAccessId>,
) -> Result<(), CheckedStorageFactsBuildError> {
    match expression {
        BoundExpression::Name(name) if target_requires_storage(name.target()) => {
            require(occurrences, StorageAccessOccurrence::Read(id))?;
        }
        BoundExpression::Assignment(assignment) => {
            if let Some(destination) = assignment.operands().first().copied() {
                require(occurrences, StorageAccessOccurrence::Write(destination))?;
                require(occurrences, StorageAccessOccurrence::Assignment(id))?;
            }
        }
        BoundExpression::MemberAccess(_) => {
            require(occurrences, StorageAccessOccurrence::Projection(id))?;
        }
        BoundExpression::Structured(expression)
            if matches!(
                expression.kind(),
                BoundStructuredExpressionKind::ElementIndex
                    | BoundStructuredExpressionKind::SliceIndex
                    | BoundStructuredExpressionKind::NullablePropagation
            ) =>
        {
            require(occurrences, StorageAccessOccurrence::Projection(id))?;
        }
        _ => {}
    }

    Ok(())
}

fn target_requires_storage(target: BoundReferenceTarget) -> bool {
    match target {
        BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(_)) => true,
        BoundReferenceTarget::Local(local) => StorageParameter::from_local_symbol(local).is_some(),
        BoundReferenceTarget::Surface(surface) => {
            StorageParameter::from_surface_symbol(surface).is_some()
                || SurfaceStorageSymbol::from_symbol(surface).is_some()
        }
    }
}

fn require(
    occurrences: &BTreeMap<StorageAccessOccurrence, StorageAccessId>,
    occurrence: StorageAccessOccurrence,
) -> Result<(), CheckedStorageFactsBuildError> {
    if occurrences.contains_key(&occurrence) {
        Ok(())
    } else {
        Err(CheckedStorageFactsBuildError::MissingRequiredOccurrence { occurrence })
    }
}
