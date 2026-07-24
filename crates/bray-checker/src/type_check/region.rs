use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundBlockId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind,
};
use bray_declarations::SyntaxAnchor;

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::inference::{InferenceTypeId, TypeInferenceContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResultRegionKind {
    SingleYield,
    ArrayGenerator,
    GeneralGenerator,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ResultRegion {
    owner: BoundExpressionId,
    variable: InferenceTypeId,
    kind: ResultRegionKind,
}

impl ResultRegion {
    pub(super) const fn owner(self) -> BoundExpressionId {
        self.owner
    }

    pub(super) const fn variable(self) -> InferenceTypeId {
        self.variable
    }

    pub(super) const fn kind(self) -> ResultRegionKind {
        self.kind
    }
}

#[derive(Debug)]
pub(super) struct ExpressionTypeRegions {
    block_owners: BTreeMap<BoundBlockId, BoundExpressionId>,
    break_variables: BTreeMap<SyntaxAnchor, InferenceTypeId>,
    results: BTreeMap<SyntaxAnchor, ResultRegion>,
}

impl ExpressionTypeRegions {
    pub(super) fn block_owner(&self, block: BoundBlockId) -> Option<BoundExpressionId> {
        self.block_owners.get(&block).copied()
    }

    pub(super) fn break_variable(&self, target: SyntaxAnchor) -> Option<InferenceTypeId> {
        self.break_variables.get(&target).copied()
    }

    pub(super) fn result(&self, target: SyntaxAnchor) -> Option<ResultRegion> {
        self.results.get(&target).copied()
    }
}

pub(super) fn initialize_expression_type_regions<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    blocks: &[BoundBlockId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    block_owners: BTreeMap<BoundBlockId, BoundExpressionId>,
    inference: &mut TypeInferenceContext,
) -> Result<ExpressionTypeRegions, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut results = BTreeMap::new();

    for block in blocks {
        let Some(bound) = request.view().block(*block) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode {
                node: AnyBoundNodeId::from(*block),
            });
        };

        let Some(owner) = block_owners.get(block).copied() else {
            continue;
        };

        let Some(variable) = block_variables.get(block).copied() else {
            continue;
        };

        results.insert(
            bound.origin().source_anchor().syntax(),
            ResultRegion {
                owner,
                variable,
                kind: ResultRegionKind::SingleYield,
            },
        );
    }

    for expression in expressions {
        let Some(BoundExpression::Structured(bound)) = request.view().expression(*expression)
        else {
            continue;
        };

        let kind = match bound.kind() {
            BoundStructuredExpressionKind::ArrayGenerator => ResultRegionKind::ArrayGenerator,
            BoundStructuredExpressionKind::GeneralGenerator => ResultRegionKind::GeneralGenerator,
            _ => continue,
        };

        let Some(variable) = inference.fresh(bound.is_recovered()) else {
            return Err(CheckerInfrastructureError::ExpressionTypeCapacityExceeded);
        };

        results.insert(
            bound.origin().source_anchor().syntax(),
            ResultRegion {
                owner: *expression,
                variable,
                kind,
            },
        );
    }

    let break_variables = collect_break_variables(request, expressions, variables)?;

    Ok(ExpressionTypeRegions {
        block_owners,
        break_variables,
        results,
    })
}

fn collect_break_variables<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
) -> Result<BTreeMap<SyntaxAnchor, InferenceTypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut regions = BTreeMap::new();

    for expression in expressions {
        let Some(variable) = variables.get(expression).copied() else {
            continue;
        };

        let Some(bound) = request.view().expression(*expression) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode {
                node: (*expression).into(),
            });
        };

        let target = match bound {
            BoundExpression::Structured(bound)
                if matches!(
                    bound.kind(),
                    BoundStructuredExpressionKind::Loop | BoundStructuredExpressionKind::While
                ) =>
            {
                Some(bound.origin().source_anchor().syntax())
            }
            BoundExpression::For(bound) => Some(bound.origin().source_anchor().syntax()),
            BoundExpression::Generator(bound) => Some(bound.region()),
            _ => None,
        };

        if let Some(target) = target {
            regions.insert(target, variable);
        }
    }

    Ok(regions)
}
