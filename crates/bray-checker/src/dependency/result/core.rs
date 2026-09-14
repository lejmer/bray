use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};
use bray_bound_tree::{
    BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundReferenceTarget, BoundStructuredExpressionKind, CheckedExpressionTypes, CheckedPatterns,
    CheckedSemanticSelections, SemanticSelection,
};
use bray_symbols::{
    AnyLocalSymbolId, CallableSymbolId, DependencyContractTemplateData,
    DependencyContractTemplateId, DependencyRequirement, DependencyRequirementKind,
    DependencySubject, LocalBindingSymbolId,
};
use std::collections::{BTreeMap, BTreeSet};

/// Infers the portable dependencies retained by normal returns using the supplied callee contracts.
/// `recursive_callees` identifies members of recursive call-graph components.
pub fn infer_result_dependencies<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    patterns: &CheckedPatterns,
    callees: &BTreeMap<CallableSymbolId, DependencyContractTemplateId>,
    recursive_callees: &BTreeSet<CallableSymbolId>,
) -> Result<DependencyContractTemplateId, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(error) = crate::unit::semantic_input_failure(
        request,
        [
            (
                crate::CheckerInputKind::ExpressionTypes,
                (types.unit(), types.kind()),
            ),
            (
                crate::CheckerInputKind::SemanticSelections,
                (selections.unit(), selections.kind()),
            ),
            (
                crate::CheckerInputKind::Patterns,
                (patterns.unit(), patterns.kind()),
            ),
        ],
    ) {
        return Err(error.into());
    }

    let mut inference = ResultInference {
        request,
        types,
        selections,
        patterns,
        callees,
        recursive_callees,
        include_evaluation_storage: false,
        call_definitions: BTreeMap::new(),
        // Nodes own snapshots so fixed-point propagation can extend destinations independently.
        values: BTreeMap::new(),
        locals: BTreeMap::new(),
        local_sources: BTreeMap::new(),
        sources: BTreeMap::new(),
        inputs: crate::dependency::ValueInputs::prepare(
            request,
            types,
            selections,
            patterns,
            |callable| {
                if recursive_callees.contains(&callable) {
                    return request
                        .semantic_values()
                        .empty_dependency_contract_template()
                        .map_err(|error| {
                            CheckerInfrastructureError::SemanticValueStore(error).into()
                        });
                }

                callees
                    .get(&callable)
                    .copied()
                    .ok_or_else(|| CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
            },
        )?,
    };

    loop {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        let mut changed = false;

        for (id, expression) in request.unit().tree().expressions() {
            let sources = inference.expression_sources(id, expression)?;
            let values = inference.expression_values(id, expression)?;

            changed |= extend(inference.sources.entry(id).or_default(), sources);
            changed |= extend(inference.values.entry(id).or_default(), values);
        }

        for (_, block) in request.unit().tree().blocks() {
            for item in block.items() {
                let BoundBlockItem::LocalBinding(binding) = item else {
                    continue;
                };

                let values = inference
                    .values
                    .get(&binding.initializer())
                    .cloned()
                    .unwrap_or_default();

                changed |=
                    inference.bind_pattern(binding.pattern(), binding.initializer(), values)?;
            }
        }

        for (_, expression) in request.unit().tree().expressions() {
            if let BoundExpression::Structured(value) = expression
                && value.kind() == BoundStructuredExpressionKind::PatternBinding
                && let Some(subject) = value.operands().first()
            {
                for pattern in value.patterns() {
                    changed |= inference.bind_pattern(*pattern, *subject, BTreeSet::new())?;
                }
            }

            let iteration = match expression {
                BoundExpression::For(value) => Some((value.pattern(), value.source())),
                BoundExpression::Generator(value) => Some((value.pattern(), value.source())),
                _ => None,
            };

            if let Some((pattern, source)) = iteration {
                let values = inference.values.get(&source).cloned().unwrap_or_default();
                changed |= inference.bind_pattern(pattern, source, values)?;
            }

            if let BoundExpression::Match(value) = expression {
                for arm in value.arms() {
                    changed |=
                        inference.bind_pattern(arm.pattern(), value.subject(), BTreeSet::new())?;
                }
            }
        }

        if !changed {
            if !inference.include_evaluation_storage {
                inference.include_evaluation_storage = true;
                continue;
            }

            break;
        }
    }

    let expression_result = match request.unit().root() {
        bray_bound_tree::BoundUnitRoot::Expression(expression) => Some(expression),
        _ => None,
    };

    let requirements = request
        .unit()
        .tree()
        .expressions()
        .filter_map(|(_, node)| {
            let BoundExpression::ControlTransfer(transfer) = node else {
                return None;
            };

            (transfer.kind() == BoundControlTransferKind::Return)
                .then(|| transfer.operand())
                .flatten()
        })
        .chain(expression_result)
        .flat_map(|operand| {
            inference
                .values
                .get(&operand)
                .into_iter()
                .flatten()
                .cloned()
        });

    let requirements = requirements.chain(
        inference
            .inputs
            .returned_errors()
            .flat_map(|(value, projection)| inference.projected_values(value, &[projection])),
    );

    let requirements = requirements.collect::<Vec<_>>();

    let requirements = if super::super::equations::has_variables(&requirements, false) {
        let maximum = inference
            .call_definitions
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0);

        let definitions = (0..=maximum).map(|ordinal| {
            inference
                .call_definitions
                .remove(&ordinal)
                .unwrap_or_default()
        });

        vec![DependencyRequirement::fixed_point(
            definitions,
            requirements,
        )]
    } else {
        requirements
    };

    request
        .semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new(requirements))
        .map_err(|error| CheckerInfrastructureError::SemanticValueStore(error).into())
}

pub(super) struct ResultInference<'a, C: CheckerRequestContext + ?Sized> {
    pub(super) request: CheckerUnitView<'a, C>,
    pub(super) types: &'a CheckedExpressionTypes,
    pub(super) selections: &'a CheckedSemanticSelections,
    pub(super) patterns: &'a CheckedPatterns,
    pub(super) callees: &'a BTreeMap<CallableSymbolId, DependencyContractTemplateId>,
    pub(super) recursive_callees: &'a BTreeSet<CallableSymbolId>,
    pub(super) include_evaluation_storage: bool,
    call_definitions: BTreeMap<u32, BTreeSet<DependencyRequirement>>,
    pub(super) values: BTreeMap<BoundExpressionId, BTreeSet<DependencyRequirement>>,
    pub(super) locals: BTreeMap<LocalBindingSymbolId, BTreeSet<DependencyRequirement>>,
    pub(super) local_sources: BTreeMap<LocalBindingSymbolId, BTreeSet<DependencySubject>>,
    pub(super) sources: BTreeMap<BoundExpressionId, BTreeSet<DependencySubject>>,
    pub(super) inputs: crate::dependency::ValueInputs,
}

impl<C: CheckerRequestContext + ?Sized> ResultInference<'_, C> {
    fn expression_values(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundExpression,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let mut requirements = if let Some(initializer) = self.inputs.projected_initializer(id) {
            self.values.get(&initializer).cloned().unwrap_or_default()
        } else {
            match expression {
                BoundExpression::Literal(_)
                | BoundExpression::Unary(_)
                | BoundExpression::Binary(_)
                | BoundExpression::AnonymousCallable(_) => BTreeSet::new(),
                BoundExpression::Name(name) => match name.target() {
                    BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)) => {
                        self.locals.get(&binding).cloned().unwrap_or_default()
                    }
                    _ => self
                        .expression_sources(id, expression)?
                        .into_iter()
                        .map(|subject| {
                            DependencyRequirement::direct(
                                subject,
                                DependencyRequirementKind::ValueDependencies,
                            )
                        })
                        .collect(),
                },
                BoundExpression::PatternReference(reference) => self
                    .locals
                    .get(&reference.binding())
                    .cloned()
                    .unwrap_or_default(),
                BoundExpression::MemberAccess(member) => {
                    self.member_values(id, expression, member.receiver())?
                }
                BoundExpression::TraitQualifiedMember(member) => {
                    self.member_values(id, expression, member.receiver())?
                }
                BoundExpression::Call(_)
                    if matches!(
                        self.selections.expression(id),
                        Some(SemanticSelection::Call(_))
                    ) =>
                {
                    let values = self.call_values(id)?;

                    if values.iter().any(|requirement| {
                        !matches!(requirement, DependencyRequirement::Direct { .. })
                    }) {
                        self.call_definitions
                            .entry(id.ordinal())
                            .or_default()
                            .extend(values);

                        BTreeSet::from([DependencyRequirement::variable(
                            0,
                            bray_symbols::SymbolOrdinal::new(id.ordinal()),
                        )])
                    } else {
                        values
                    }
                }
                BoundExpression::Structured(value)
                    if value.kind() == BoundStructuredExpressionKind::Borrow =>
                {
                    let operand = value
                        .operands()
                        .first()
                        .copied()
                        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

                    let reborrowed = self.reborrowed_receiver(operand)?;

                    let kind = if reborrowed.is_some() {
                        DependencyRequirementKind::ValueDependencies
                    } else {
                        DependencyRequirementKind::StorageAlive
                    };

                    let mut sources = self.expression_sources(id, expression)?;

                    if sources.is_empty()
                        && let Some(receiver) = reborrowed
                    {
                        return Ok(self.projected_values(receiver, &[]));
                    }

                    if self.include_evaluation_storage
                        && sources.is_empty()
                        && self.borrows_evaluation_storage(operand)
                    {
                        sources.insert(DependencySubject::root(
                            bray_symbols::DependencySubjectRoot::EvaluationStorage,
                        ));
                    }

                    sources
                        .into_iter()
                        .map(|source| DependencyRequirement::direct(source, kind))
                        .collect()
                }
                _ => self
                    .inputs
                    .operands(id)
                    .flat_map(|child| self.values.get(&child).into_iter().flatten().cloned())
                    .collect(),
            }
        };

        if self.inputs.is_independent(id) {
            return Ok(BTreeSet::new());
        }

        for (value, path) in self.inputs.projected_operands(id) {
            requirements.extend(self.projected_values(value, path));
        }

        Ok(requirements)
    }

    fn member_values(
        &self,
        id: BoundExpressionId,
        expression: &BoundExpression,
        receiver: BoundExpressionId,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let sources = self.expression_sources(id, expression)?;

        if sources.is_empty() {
            return Ok(self.values.get(&receiver).cloned().unwrap_or_default());
        }

        Ok(sources
            .into_iter()
            .map(|subject| {
                DependencyRequirement::direct(subject, DependencyRequirementKind::ValueDependencies)
            })
            .collect())
    }
}

pub(super) fn extend<T: Ord>(
    destination: &mut BTreeSet<T>,
    values: impl IntoIterator<Item = T>,
) -> bool {
    let before = destination.len();

    destination.extend(values);

    destination.len() != before
}
