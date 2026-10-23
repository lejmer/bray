use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    CheckedExpressionTypes, CheckedPatterns, CheckedSemanticSelections, SemanticSelection,
};
use bray_symbols::{
    AnyLocalSymbolId, DependencyRequirement, DependencyRequirementKind, DependencySubject,
};
use std::collections::BTreeSet;

pub(crate) struct Transfer<'a, 'state, C: CheckerRequestContext + ?Sized> {
    pub(super) request: CheckerUnitView<'a, C>,
    pub(super) types: &'a CheckedExpressionTypes,
    pub(super) selections: &'a CheckedSemanticSelections,
    pub(super) patterns: &'a CheckedPatterns,
    pub(super) inputs: &'a crate::dependency::ValueInputs,
    pub(super) state: &'state mut super::state::State,
}

impl<'a, 'state, C: CheckerRequestContext + ?Sized> Transfer<'a, 'state, C> {
    pub(crate) fn new(
        request: CheckerUnitView<'a, C>,
        types: &'a CheckedExpressionTypes,
        selections: &'a CheckedSemanticSelections,
        patterns: &'a CheckedPatterns,
        inputs: &'a crate::dependency::ValueInputs,
        state: &'state mut super::state::State,
    ) -> Self {
        Self {
            request,
            types,
            selections,
            patterns,
            inputs,
            state,
        }
    }

    pub(crate) fn operation(
        &mut self,
        operation: crate::analysis::AnalysisOperationKind,
        yielded: Option<BoundExpressionId>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        use crate::analysis::{AnalysisCallPhase, AnalysisOperationKind};

        if matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt,
                ..
            } | AnalysisOperationKind::Result {
                phase: crate::analysis::AnalysisResultPhase::Begin,
                ..
            } | AnalysisOperationKind::ScopeExit { .. }
                | AnalysisOperationKind::Suspension { .. }
        ) {
            return Ok(());
        }

        match operation.node() {
            bray_bound_tree::AnyBoundNodeId::Expression(id) => {
                let expression = self
                    .request
                    .view()
                    .expression(id)
                    .expect("flow expression is bound");

                let values = self.expression_values(id, expression)?;

                super::state::assign(&mut self.state.values, id, values);

                let sources = if self.inputs.source_inputs.contains(&id) {
                    self.expression_sources(id, expression)
                } else {
                    BTreeSet::new()
                };

                super::state::assign(&mut self.state.sources, id, sources);

                if let Some(target) = yielded
                    && let BoundExpression::ControlTransfer(transfer) = expression
                    && let Some(operand) = transfer.operand()
                {
                    // The selected branch supplies the enclosing expression's result at this edge.
                    let values = self.state.values.get(&operand).cloned().unwrap_or_default();

                    let sources = self
                        .state
                        .sources
                        .get(&operand)
                        .cloned()
                        .unwrap_or_default();

                    super::state::assign(&mut self.state.values, target, values);
                    super::state::assign(&mut self.state.sources, target, sources);
                }

                let returned = match expression {
                    BoundExpression::ControlTransfer(transfer)
                        if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Return =>
                    {
                        transfer.operand()
                    }
                    _ if matches!(self.request.root(), crate::CheckerUnitRoot::Expression(root) if root == id) => {
                        Some(id)
                    }
                    _ => None,
                };

                if let Some(value) = returned {
                    self.state
                        .returned
                        .extend(self.state.values.get(&value).into_iter().flatten().cloned());
                }

                if self.inputs.returned_errors.contains(&id) {
                    let (value, projection) = self.inputs.error_exits[&id];

                    let requirements = self.projected_values(value, &[projection]);

                    self.state.returned.extend(requirements);
                }
            }
            bray_bound_tree::AnyBoundNodeId::Pattern(pattern) => {
                if let Some(source) = self.inputs.pattern_subjects.get(&pattern).copied() {
                    if let Some(SemanticSelection::ScopedUse(scoped)) =
                        self.selections.expression(source)
                    {
                        let values = self.scoped_values(scoped)?;

                        self.bind_pattern_values(pattern, None, BTreeSet::new(), values)?;
                    } else {
                        let values = self.state.values.get(&source).cloned().unwrap_or_default();

                        self.bind_pattern(pattern, source, values)?;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

impl<C: CheckerRequestContext + ?Sized> Transfer<'_, '_, C> {
    fn expression_values(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundExpression,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        if self.inputs.is_independent(id) {
            return Ok(BTreeSet::new());
        }

        let mut requirements = if let Some(initializer) = self.inputs.projected_initializer(id) {
            self.state
                .values
                .get(&initializer)
                .cloned()
                .unwrap_or_default()
        } else {
            match expression {
                BoundExpression::Literal(_)
                | BoundExpression::Unary(_)
                | BoundExpression::Binary(_)
                | BoundExpression::AnonymousCallable(_) => BTreeSet::new(),
                BoundExpression::Name(name) => match name.target() {
                    BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)) => {
                        self.state.locals.get(&binding).cloned().unwrap_or_default()
                    }
                    _ => self
                        .expression_sources(id, expression)
                        .into_iter()
                        .map(|subject| {
                            DependencyRequirement::direct(
                                super::sources::source_subject(&subject, []),
                                DependencyRequirementKind::ValueDependencies,
                            )
                        })
                        .collect(),
                },
                BoundExpression::PatternReference(reference) => self
                    .state
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
                        self.state
                            .call_definitions
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
                    let operand = value.operands().first().copied().unwrap_or_else(|| {
                        panic!("expression_values requires structured value operand, id: {id:?}")
                    });

                    let reborrowed = self.reborrowed_receiver(operand);

                    let kind = if reborrowed.is_some() {
                        DependencyRequirementKind::ValueDependencies
                    } else {
                        DependencyRequirementKind::StorageAlive
                    };

                    let mut sources = self.expression_sources(id, expression);

                    if sources.is_empty()
                        && let Some(receiver) = reborrowed
                    {
                        return Ok(self.projected_values(receiver, &[]));
                    }

                    if sources.is_empty() && self.borrows_evaluation_storage(operand) {
                        sources.insert((
                            DependencySubject::root(
                                bray_symbols::DependencySubjectRoot::EvaluationStorage,
                            ),
                            true,
                        ));
                    }

                    sources
                        .into_iter()
                        .map(|source| {
                            DependencyRequirement::direct(
                                super::sources::source_subject(&source, []),
                                kind,
                            )
                        })
                        .collect()
                }
                _ => self
                    .inputs
                    .operands(id)
                    .flat_map(|child| self.state.values.get(&child).into_iter().flatten().cloned())
                    .collect(),
            }
        };

        for (value, path) in self.inputs.projected_operands(id) {
            requirements.extend(self.projected_values(value, &path));
        }

        Ok(requirements)
    }

    fn member_values(
        &self,
        id: BoundExpressionId,
        expression: &BoundExpression,
        receiver: BoundExpressionId,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let sources = self.expression_sources(id, expression);

        if sources.is_empty() {
            return Ok(self
                .state
                .values
                .get(&receiver)
                .cloned()
                .unwrap_or_default());
        }

        Ok(sources
            .into_iter()
            .map(|subject| {
                DependencyRequirement::direct(
                    super::sources::source_subject(&subject, []),
                    DependencyRequirementKind::ValueDependencies,
                )
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
