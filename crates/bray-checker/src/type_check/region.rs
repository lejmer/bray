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

pub(super) type ResultRegions = BTreeMap<SyntaxAnchor, ResultRegion>;

pub(super) fn initialize_result_regions<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    blocks: &[BoundBlockId],
    block_variables: &BTreeMap<BoundBlockId, InferenceTypeId>,
    block_owners: &BTreeMap<BoundBlockId, BoundExpressionId>,
    inference: &mut TypeInferenceContext,
) -> Result<ResultRegions, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut regions = BTreeMap::new();

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

        regions.insert(
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

        regions.insert(
            bound.origin().source_anchor().syntax(),
            ResultRegion {
                owner: *expression,
                variable,
                kind,
            },
        );
    }

    Ok(regions)
}
