use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    CheckedExpressionTypes, CheckedPatterns, CheckedSemanticSelections, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, DependencyRequirement, DependencyRequirementKind};
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
        completed: Option<&BTreeSet<BoundExpressionId>>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        use crate::analysis::{AnalysisCallPhase, AnalysisOperationKind};

        if matches!(operation, AnalysisOperationKind::Bound(_))
            && operation
                .occurrence()
                .expression()
                .and_then(|expression| self.request.view().expression(expression))
                .is_some_and(bray_bound_tree::BoundExpression::has_result_region)
        {
            return Ok(());
        }

        if matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input | AnalysisCallPhase::Attempt,
                ..
            } | AnalysisOperationKind::Result {
                phase: crate::analysis::AnalysisResultPhase::Begin,
                ..
            } | AnalysisOperationKind::ScopeExit { .. }
                | AnalysisOperationKind::Suspension { .. }
        ) {
            return Ok(());
        }

        if let crate::analysis::AnalysisOperationKind::Call {
            invocation: input @ bray_bound_tree::BoundExecutionSite::CallInput { .. },
            phase: AnalysisCallPhase::Completion,
        } = operation
        {
            let values = self.input_values(input)?;

            super::state::assign(&mut self.state.values, input, values);

            return Ok(());
        }

        if let AnalysisOperationKind::Call {
            invocation:
                invocation @ (bray_bound_tree::BoundExecutionSite::Iterate(_)
                | bray_bound_tree::BoundExecutionSite::Next(_)
                | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_)),
            phase: AnalysisCallPhase::Completion,
        } = operation
        {
            let values = self.implicit_values(invocation)?;

            super::state::assign(&mut self.state.values, invocation, values);

            return Ok(());
        }

        match operation.node() {
            bray_bound_tree::AnyBoundNodeId::Expression(id) => {
                let expression = self
                    .request
                    .view()
                    .expression(id)
                    .expect("flow expression is bound");

                let values = self.expression_values(id, expression, completed)?;

                super::state::assign(&mut self.state.values, id.into(), values);

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
                    let values = self
                        .state
                        .values
                        .get(&(operand).into())
                        .cloned()
                        .unwrap_or_default();

                    let sources = self
                        .state
                        .sources
                        .get(&operand)
                        .cloned()
                        .unwrap_or_default();

                    super::state::assign(&mut self.state.values, target.into(), values);
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
                    self.state.returned.extend(
                        self.state
                            .values
                            .get(&(value).into())
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                }

                if self.inputs.returned_errors.contains(&id) {
                    let (value, projection) = self.inputs.error_exits[&id];

                    let requirements = self.projected_values(value, &[projection]);

                    self.state.returned.extend(requirements);
                }
            }
            bray_bound_tree::AnyBoundNodeId::Pattern(pattern) => {
                if let Some(source) = self.inputs.pattern_subjects.get(&pattern).copied() {
                    let values = self.state.values.get(&source).cloned().unwrap_or_default();

                    match source {
                        bray_bound_tree::BoundExecutionSite::Node(
                            bray_bound_tree::AnyBoundNodeId::Expression(expression),
                        ) => self.bind_pattern(pattern, expression, values)?,
                        bray_bound_tree::BoundExecutionSite::Next(_)
                        | bray_bound_tree::BoundExecutionSite::ScopedEnter(_) => {
                            self.bind_pattern_values(pattern, None, BTreeSet::new(), values)?
                        }
                        _ => panic!("binding pattern has an evaluated subject: {source:?}"),
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
        completed: Option<&BTreeSet<BoundExpressionId>>,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        if self.inputs.is_independent(id) {
            return Ok(BTreeSet::new());
        }

        let mut call_result = false;

        let mut requirements = if let Some(initializer) = self.inputs.projected_initializer(id) {
            self.state
                .values
                .get(&(initializer).into())
                .cloned()
                .unwrap_or_default()
        } else if let Some(SemanticSelection::Operation(operation)) = self.selections.expression(id)
            && let Some(template) =
                crate::dependency::call::operation_result_dependencies(self.request, id, operation)?
        {
            call_result = true;

            self.operation_values(id, operation, template.template.requirements())?
        } else {
            match expression {
                BoundExpression::Unary(_) | BoundExpression::Binary(_) => BTreeSet::new(),
                BoundExpression::Literal(_) | BoundExpression::AnonymousCallable(_) => {
                    BTreeSet::new()
                }
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
                BoundExpression::MemberAccess(member) => self.member_values(id, member.receiver()),
                BoundExpression::TraitQualifiedMember(member) => {
                    self.member_values(id, member.receiver())
                }
                BoundExpression::Await(_)
                    if completed
                        .or_else(|| self.inputs.completion_sources(id))
                        .is_some() =>
                {
                    let mut requirements = completed
                        .or_else(|| self.inputs.completion_sources(id))
                        .into_iter()
                        .flatten()
                        .filter_map(|source| self.state.completions.get(source))
                        .flatten()
                        .cloned()
                        .collect::<BTreeSet<_>>();

                    // Opaque values retain their own captures, rather than every
                    // capture of the joined value's unrelated known producers.
                    if completed.is_none_or(BTreeSet::is_empty) {
                        requirements.extend(
                            self.inputs
                                .operands(id)
                                .flat_map(|source| self.state.value_requirements(source).cloned()),
                        );
                    }

                    requirements
                }
                BoundExpression::Call(_)
                    if matches!(
                        self.selections.expression(id),
                        Some(SemanticSelection::Call(_))
                    ) =>
                {
                    call_result = true;

                    self.call_values(id)?
                }
                BoundExpression::Structured(value)
                    if value.kind() == BoundStructuredExpressionKind::With =>
                {
                    let mut values = self
                        .state
                        .values
                        .get(&(id).into())
                        .cloned()
                        .unwrap_or_default();

                    if let Some(SemanticSelection::ScopedUse(scoped)) =
                        self.selections.expression(id)
                    {
                        if scoped.enter().1.result() != scoped.capability_type() {
                            values.extend(self.scoped_values(scoped)?);
                        }

                        if crate::representation::type_representation(
                            self.request,
                            scoped.exit().1.result(),
                        ) == Some(bray_compiler_known::RepresentationRole::Result)
                        {
                            values.extend(self.exit_values(scoped)?);
                        }
                    }

                    values
                }
                BoundExpression::Structured(value)
                    if value.kind() == BoundStructuredExpressionKind::Catch =>
                {
                    self.state
                        .values
                        .get(&(id).into())
                        .cloned()
                        .unwrap_or_default()
                }
                BoundExpression::Structured(value)
                    if value.kind() == BoundStructuredExpressionKind::Borrow =>
                {
                    let operand = value.operands().first().copied().unwrap_or_else(|| {
                        panic!("expression_values requires structured value operand, id: {id:?}")
                    });

                    self.borrowed_values(operand, &[])
                }
                _ => self
                    .inputs
                    .operands(id)
                    .flat_map(|child| {
                        self.state
                            .values
                            .get(&(child).into())
                            .into_iter()
                            .flatten()
                            .cloned()
                    })
                    .collect(),
            }
        };

        if let Some(SemanticSelection::Operation(bray_bound_tree::SelectedOperation::TraitView {
            requirement,
            ..
        })) = self.selections.expression(id)
        {
            requirements.insert(DependencyRequirement::direct(
                bray_symbols::DependencySubject::root(
                    bray_symbols::DependencySubjectRoot::ImplementationRequirement(*requirement),
                ),
                DependencyRequirementKind::ValueDependencies,
            ));
        }

        for (value, path) in self.inputs.projected_operands(id) {
            requirements.extend(self.projected_values(value, &path));
        }

        if call_result
            && requirements
                .iter()
                .any(|requirement| !matches!(requirement, DependencyRequirement::Direct { .. }))
        {
            self.state
                .call_definitions
                .entry(id.ordinal())
                .or_default()
                .extend(requirements);

            return Ok(BTreeSet::from([DependencyRequirement::variable(
                0,
                bray_symbols::SymbolOrdinal::new(id.ordinal()),
            )]));
        }

        Ok(requirements)
    }

    fn member_values(
        &self,
        id: BoundExpressionId,
        receiver: BoundExpressionId,
    ) -> BTreeSet<DependencyRequirement> {
        let Some(projection) = crate::dependency::assignment::member_projection(
            self.request.unit(),
            self.selections,
            id,
        ) else {
            return self
                .state
                .values
                .get(&receiver.into())
                .cloned()
                .unwrap_or_default();
        };

        // Physical sources select components. Opaque referents retain their complete
        // carried contract, rather than projecting fields onto a lifetime anchor.
        self.projected_values(receiver, &[projection])
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
