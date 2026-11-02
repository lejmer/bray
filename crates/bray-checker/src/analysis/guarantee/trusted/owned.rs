use std::collections::{BTreeMap, BTreeSet};

use super::super::{flow::ExecutionFlowDomain, state::ExecutionState};
use crate::{CheckerRequestContext, ExecutionCondition};
use bray_symbols::{ConstantProjectionKind, DependencyOwnedTransfer};

/// The output shape under which a checked identity transfer is available.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::analysis) enum OwnedReturnCondition {
    Unconditional,
    ActivePayload,
    PresentValue,
}

/// Only ordinary result-shape facts are needed to compare normal ownership exits.
#[derive(Eq, PartialEq)]
pub(super) struct OwnedReturn {
    value: ExecutionCondition,
    assumptions: BTreeSet<(ExecutionCondition, bool)>,
    transfers: BTreeSet<DependencyOwnedTransfer>,
}

impl super::check::TrustedChecks {
    pub(super) fn collect_owned_return<C: CheckerRequestContext + ?Sized>(
        &mut self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        result: &ExecutionCondition,
    ) {
        if !domain
            .request
            .trusted_contracts()
            .is_some_and(|inputs| inputs.infer_result_witnesses)
        {
            return;
        }

        let returned = OwnedReturn {
            value: result.clone(),
            assumptions: state
                .assumptions
                .iter()
                .filter(|(condition, holds)| {
                    condition.nullable_absence(*holds).is_some()
                        || condition.nullable_absence(!*holds).is_some()
                        || matches!(condition, ExecutionCondition::VariantTest(_, _))
                })
                .cloned()
                .collect(),
            transfers: owned_transfers(domain, result),
        };

        if !self.owned_returns.contains(&returned) {
            self.owned_returns.push(returned);
        }
    }

    pub(in crate::analysis) fn owned_transfers<C: CheckerRequestContext + ?Sized>(
        &self,
        domain: &ExecutionFlowDomain<'_, '_, C>,
    ) -> BTreeMap<DependencyOwnedTransfer, OwnedReturnCondition> {
        let candidates = self
            .owned_returns
            .iter()
            .flat_map(|returned| returned.transfers.iter())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut retained = BTreeMap::new();

        for transfer in candidates {
            let mut conditional = false;
            let mut presence = false;

            let valid = self.owned_returns.iter().all(|returned| {
                let (active, leaf_absent) = activity(
                    domain,
                    &returned.value,
                    transfer.output(),
                    &returned.assumptions,
                );

                if returned.transfers.contains(&transfer) {
                    conditional |= active.is_none();

                    true
                } else if active == Some(false) {
                    conditional = true;
                    presence |= leaf_absent;

                    true
                } else {
                    false
                }
            });

            if valid {
                let condition = if presence {
                    OwnedReturnCondition::PresentValue
                } else if conditional {
                    OwnedReturnCondition::ActivePayload
                } else {
                    OwnedReturnCondition::Unconditional
                };

                retained.insert(transfer, condition);
            }
        }

        retained
    }
}

/// An inactive alternative owns no payload and imposes no input-identity requirement.
fn activity<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    result: &ExecutionCondition,
    path: &[ConstantProjectionKind],
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
) -> (Option<bool>, bool) {
    let mut value = result.clone();
    let mut active = Some(true);

    for projection in path {
        let known = match projection {
            ConstantProjectionKind::NullableValue => {
                if matches!(value, ExecutionCondition::Present(_)) {
                    Some(true)
                } else if matches!(&value, ExecutionCondition::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
                {
                    Some(false)
                } else {
                    assumptions.iter().find_map(|(condition, holds)| {
                        condition
                            .nullable_absence(*holds)
                            .map(|(subject, _)| (subject, false))
                            .or_else(|| {
                                condition
                                    .nullable_absence(!*holds)
                                    .map(|(subject, _)| (subject, true))
                            })
                            .and_then(|(subject, present)| {
                                (subject == &value || subject == value.owner_identity())
                                    .then_some(present)
                            })
                    })
                }
            }
            ConstantProjectionKind::UnionPayloadField(field) => value
                .clone()
                .matches_variant(domain.dependencies.projection_variant(*field))
                .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
            _ => Some(true),
        };

        if known == Some(false) {
            return (Some(false), false);
        }

        if known.is_none() {
            active = None;
        }

        value = value.component(*projection);
    }

    if matches!(&value, ExecutionCondition::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
    {
        (Some(false), true)
    } else {
        (active, false)
    }
}

/// Preserve exact owned inputs embedded in the checked normal result.
fn owned_transfers<C: crate::CheckerRequestContext + ?Sized>(
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    result: &ExecutionCondition,
) -> BTreeSet<bray_symbols::DependencyOwnedTransfer> {
    use bray_bound_tree::StorageProjection;
    use bray_symbols::{ConstantProjectionKind, DependencyOwnedTransfer};

    let mut transfers = BTreeSet::new();

    let Some(inputs) = domain.request.trusted_contracts() else {
        return transfers;
    };

    if !inputs.infer_result_witnesses {
        return transfers;
    }

    let mut pending = vec![(result, Vec::new())];

    while let Some((source, path)) = pending.pop() {
        if path.len() > ExecutionCondition::WORK_LIMIT {
            continue;
        }

        let mut input = source;
        let mut projections = Vec::new();

        while let ExecutionCondition::Projection(field, parent) = input {
            projections.push(*field);
            input = parent;
        }

        if let ExecutionCondition::Input(place) = input {
            let place = projections
                .into_iter()
                .rev()
                .fold(place.clone(), |place, field| place.component(field));

            if !place.is_callable_input()
                || domain
                    .borrowed_inputs
                    .iter()
                    .any(|borrow| borrow.contains(&place))
            {
                continue;
            }

            let input = ExecutionCondition::Input(place);

            if let Some(term) = super::carriers::portable(domain.request, &input) {
                transfers.insert(DependencyOwnedTransfer::new(term, path));
            }

            continue;
        }

        match source {
            ExecutionCondition::Present(value) => {
                let mut path = path;

                path.push(ConstantProjectionKind::NullableValue);
                pending.push((value, path));
            }
            ExecutionCondition::Constructed(_, fields) => {
                for (field, value) in fields.iter() {
                    let projection = match field {
                        StorageProjection::ProductField(field) => {
                            ConstantProjectionKind::ProductField(*field)
                        }
                        StorageProjection::ActiveUnionPayloadField { field, .. } => {
                            ConstantProjectionKind::UnionPayloadField(*field)
                        }
                        StorageProjection::TupleElement(ordinal) => {
                            ConstantProjectionKind::TupleElement(*ordinal)
                        }
                        StorageProjection::ElementFromStart(ordinal) => {
                            ConstantProjectionKind::ArrayElementOrdinal(*ordinal)
                        }
                        StorageProjection::NullableValue => ConstantProjectionKind::NullableValue,
                        _ => continue,
                    };

                    let mut path = path.clone();

                    path.push(projection);
                    pending.push((value, path));
                }
            }
            _ => {}
        }
    }

    transfers
}
