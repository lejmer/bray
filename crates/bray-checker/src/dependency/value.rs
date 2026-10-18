use crate::{CheckerRequestContext, CheckerUnitView};
use bray_symbols::TypeData;
use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundControlTransferKind, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind,
    BoundUnit, CheckedSemanticSelections, SelectedOperation, SemanticSelection,
};

/// The operands whose values contribute to an expression's result, excluding evaluation inputs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ValueInputs {
    independent: std::collections::BTreeSet<BoundExpressionId>,
    pub(super) source_inputs: std::collections::BTreeSet<BoundExpressionId>,
    pub(super) pattern_subjects: BTreeMap<bray_bound_tree::BoundPatternId, BoundExpressionId>,
    pub(super) projections: BTreeMap<
        BoundExpressionId,
        BTreeMap<
            bray_symbols::DependencyProjection,
            (
                BoundExpressionId,
                Option<bray_symbols::DependencyProjection>,
            ),
        >,
    >,
    pub(super) initializers: BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(super) borrowed: BTreeMap<
        BoundExpressionId,
        Vec<(BoundExpressionId, Vec<bray_symbols::DependencyProjection>)>,
    >,
    pub(super) writes: super::assignment::AssignmentInputs,
    pub(super) projected: BTreeMap<
        BoundExpressionId,
        Vec<(BoundExpressionId, Vec<bray_symbols::DependencyProjection>)>,
    >,
    pub(super) error_exits:
        BTreeMap<BoundExpressionId, (BoundExpressionId, bray_symbols::DependencyProjection)>,
    pub(super) returned_errors: std::collections::BTreeSet<BoundExpressionId>,
    pub(super) expressions: BTreeMap<BoundExpressionId, Vec<BoundExpressionId>>,
    pub(super) projected_initializers: BTreeMap<BoundExpressionId, BoundExpressionId>,
}

impl ValueInputs {
    pub(crate) fn new(
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        patterns: &bray_bound_tree::CheckedPatterns,
    ) -> Self {
        let mut expressions = BTreeMap::<_, Vec<_>>::new();
        let targets = transfer_targets(unit);

        for (id, expression) in unit.tree().expressions() {
            let inputs = match expression {
                BoundExpression::Call(call)
                    if matches!(
                        selections.expression(id),
                        Some(SemanticSelection::Operation(
                            SelectedOperation::Construction(_)
                        ))
                    ) =>
                {
                    call.arguments()
                        .iter()
                        .map(bray_bound_tree::BoundArgument::expression)
                        .collect()
                }
                BoundExpression::Call(_)
                | BoundExpression::Unary(_)
                | BoundExpression::Binary(_)
                | BoundExpression::Assignment(_)
                | BoundExpression::Match(_)
                | BoundExpression::For(_)
                | BoundExpression::Block(_)
                | BoundExpression::AnonymousCallable(_) => Vec::new(),
                BoundExpression::Structured(value) => match value.kind() {
                    BoundStructuredExpressionKind::Conditional
                    | BoundStructuredExpressionKind::While
                    | BoundStructuredExpressionKind::Loop
                    | BoundStructuredExpressionKind::With
                    | BoundStructuredExpressionKind::Catch => Vec::new(),
                    BoundStructuredExpressionKind::ElementIndex
                    | BoundStructuredExpressionKind::SliceIndex
                    | BoundStructuredExpressionKind::RepeatedArray => {
                        value.operands().first().copied().into_iter().collect()
                    }
                    _ => value.operands().to_vec(),
                },
                _ => expression.child_expressions().collect(),
            };

            expressions.insert(id, inputs);
        }

        for (_, expression) in unit.tree().expressions() {
            let BoundExpression::ControlTransfer(transfer) = expression else {
                continue;
            };

            if matches!(
                transfer.kind(),
                BoundControlTransferKind::Yield | BoundControlTransferKind::Break
            ) && let Some(owner) = transfer.target().and_then(|target| targets.get(&target))
                && let Some(operand) = transfer.operand()
            {
                expressions.entry(*owner).or_default().push(operand);
            }
        }

        let mut result = Self {
            projected: BTreeMap::new(),
            error_exits: BTreeMap::new(),
            returned_errors: Default::default(),
            expressions,
            projected_initializers: BTreeMap::new(),
            pattern_subjects: crate::pattern::binding_subjects(unit).into_iter().collect(),
            independent: Default::default(),
            source_inputs: Default::default(),
            projections: BTreeMap::new(),
            initializers: BTreeMap::new(),
            borrowed: BTreeMap::new(),
            writes: Default::default(),
        };

        for entry in selections.entries() {
            if let SemanticSelection::ScopedUse(_) = entry.selection()
                && let Some(BoundExpression::Structured(value)) =
                    unit.tree().expression(entry.expression())
            {
                result.pattern_subjects.extend(
                    value
                        .patterns()
                        .iter()
                        .map(|pattern| (*pattern, entry.expression())),
                );
            }
        }

        result.collect_projections(unit, selections, patterns);

        result.writes = super::assignment::AssignmentInputs::new(unit, selections, &result);

        result
    }

    pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
        request: CheckerUnitView<'_, C>,
        types: &bray_bound_tree::CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
        patterns: &bray_bound_tree::CheckedPatterns,
        declaration: impl FnMut(
            bray_symbols::CallableSymbolId,
        ) -> Result<
            bray_symbols::DependencyContractTemplateId,
            crate::CheckerQueryError<C::UpstreamError>,
        >,
    ) -> Result<Self, crate::CheckerQueryError<C::UpstreamError>> {
        let mut inputs = Self::new(request.unit(), selections, patterns);

        inputs.collect_returned_borrows(request, types, selections, declaration)?;
        inputs.collect_projections(request.unit(), selections, patterns);

        inputs.writes =
            super::assignment::AssignmentInputs::new(request.unit(), selections, &inputs);

        inputs.filter_independent(request, types)?;
        inputs.collect_propagation(request, selections)?;
        inputs.collect_source_inputs(request.unit(), patterns);

        Ok(inputs)
    }

    fn collect_source_inputs(
        &mut self,
        unit: &BoundUnit,
        patterns: &bray_bound_tree::CheckedPatterns,
    ) {
        let mut pending = unit
            .tree()
            .expressions()
            .filter(|(expression, _)| !self.is_independent(*expression))
            .map(|(expression, _)| expression)
            .collect::<Vec<_>>();

        for (pattern, source) in &self.pattern_subjects {
            let mut nested = vec![*pattern];

            while let Some(pattern) = nested.pop() {
                let node = unit
                    .tree()
                    .pattern(pattern)
                    .expect("prepared pattern is bound");

                let checked = patterns
                    .pattern(pattern)
                    .expect("prepared pattern is checked");

                if checked.bindings(node).any(|binding| {
                    patterns.binding_type(binding).is_some_and(|ty| {
                        matches!(
                            ty.operation(),
                            bray_bound_tree::PatternOperation::Observe
                                | bray_bound_tree::PatternOperation::SharedBorrow
                                | bray_bound_tree::PatternOperation::MutableBorrow
                        )
                    })
                }) {
                    pending.push(*source);
                    break;
                }

                nested.extend(node.children());
            }
        }

        while let Some(expression) = pending.pop() {
            if self.source_inputs.insert(expression) {
                pending.extend(
                    unit.tree()
                        .expression(expression)
                        .expect("source dependency expression is bound")
                        .child_expressions(),
                );
            }
        }
    }

    pub(crate) fn constructed_fields(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<
        Item = (
            bray_symbols::DependencyProjection,
            BoundExpressionId,
            &[bray_symbols::DependencyProjection],
        ),
    > + '_ {
        self.projections
            .get(&expression)
            .into_iter()
            .flatten()
            .map(|(projection, (operand, suffix))| (*projection, *operand, suffix.as_slice()))
    }

    pub(crate) fn projected_operands(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<
        Item = (
            BoundExpressionId,
            std::borrow::Cow<'_, [bray_symbols::DependencyProjection]>,
        ),
    > {
        (!self.is_independent(expression))
            .then_some(expression)
            .into_iter()
            .flat_map(move |expression| {
                self.projected
                    .get(&expression)
                    .into_iter()
                    .flatten()
                    .map(|(value, path)| (*value, std::borrow::Cow::Borrowed(path.as_slice())))
                    .chain(self.writes.values(expression).map(|write| {
                        (
                            write.value,
                            std::borrow::Cow::Owned(write.source().collect()),
                        )
                    }))
            })
    }

    pub(crate) fn propagated_errors(
        &self,
    ) -> impl Iterator<
        Item = (
            BoundExpressionId,
            BoundExpressionId,
            bray_symbols::DependencyProjection,
        ),
    > {
        self.error_exits
            .iter()
            .map(|(exit, (value, projection))| (*exit, *value, *projection))
    }

    pub(crate) fn filter_independent<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        types: &bray_bound_tree::CheckedExpressionTypes,
    ) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
        for (expression, _) in request.unit().tree().expressions() {
            if let Some(ty) = types.expression(expression)
                && independent_value_type(request, ty.ty())?
            {
                self.independent.insert(expression);
                self.expressions.remove(&expression);
            }
        }

        Ok(())
    }

    pub(crate) fn is_independent(&self, expression: BoundExpressionId) -> bool {
        self.independent.contains(&expression)
    }

    pub(crate) fn projected_initializer(
        &self,
        expression: BoundExpressionId,
    ) -> Option<BoundExpressionId> {
        self.projected_initializers.get(&expression).copied()
    }

    pub(crate) fn operands(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = BoundExpressionId> + '_ {
        let operands = self
            .projected_initializers
            .get(&expression)
            .map(std::slice::from_ref)
            .unwrap_or_else(|| {
                self.expressions
                    .get(&expression)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
            });

        operands.iter().copied()
    }
}

pub(crate) fn independent_value_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: bray_symbols::TypeId,
) -> Result<bool, crate::CheckerQueryError<C::UpstreamError>> {
    let mut pending = vec![ty];
    let mut visited = std::collections::BTreeSet::new();

    while let Some(ty) = pending.pop() {
        if request.is_cancelled() {
            return Err(crate::CheckerQueryError::Cancelled);
        }

        if !visited.insert(ty) {
            continue;
        }

        let data = request.semantic_values().type_data(ty);

        let (definition, substitution) = match data.as_ref() {
            TypeData::Nullable(element) | TypeData::Array { element, .. } => {
                pending.push(*element);
                continue;
            }
            TypeData::Tuple(elements) => {
                pending.extend(elements.iter().copied());
                continue;
            }
            TypeData::Named {
                definition,
                substitution,
            } => (*definition, *substitution),
            _ => return Ok(false),
        };

        let symbols = request.available_compiler_known_symbols();

        let role = match definition {
            bray_symbols::NamedTypeSymbolId::Struct(id) => symbols.symbol_representation(id),
            bray_symbols::NamedTypeSymbolId::Union(id) => symbols.symbol_representation(id),
        };

        if role.is_some_and(|role| {
            role.numeric_kind().is_some()
                || matches!(
                    role,
                    bray_compiler_known::RepresentationRole::ScalarBool
                        | bray_compiler_known::RepresentationRole::ScalarChar
                        | bray_compiler_known::RepresentationRole::Unit
                        | bray_compiler_known::RepresentationRole::Never
                        | bray_compiler_known::RepresentationRole::Range
                )
        }) {
            continue;
        }

        if role == Some(bray_compiler_known::RepresentationRole::Result) {
            let arguments = request
                .semantic_values()
                .generic_substitution_data(substitution);

            pending.extend(arguments.bindings().iter().filter_map(
                |binding| match binding.argument() {
                    bray_symbols::GenericArgument::Type(ty) => Some(ty),
                    bray_symbols::GenericArgument::Constant(_) => None,
                },
            ));

            continue;
        }

        if role.is_some() {
            return Ok(false);
        }

        let representation = request.declared_type_representation(definition)?;

        if representation.value().is_recovered() || representation.diagnostics().has_errors() {
            return Ok(false);
        }

        for member in representation.value().storage().member_types() {
            let checked = crate::constant::checked_substituted_type(request, member, substitution)?;

            if checked.diagnostics().has_errors() {
                return Ok(false);
            }

            let Some(ty) = *checked.value() else {
                return Ok(false);
            };

            pending.push(ty);
        }
    }

    Ok(true)
}

pub(crate) fn transfer_targets(
    unit: &BoundUnit,
) -> BTreeMap<bray_declarations::SyntaxAnchor, BoundExpressionId> {
    let mut targets = BTreeMap::new();

    for (id, expression) in unit.tree().expressions() {
        targets.insert(expression.origin().source_anchor().syntax(), id);

        for block in expression.child_blocks() {
            if let Some(block) = unit.tree().block(block) {
                targets.insert(block.origin().source_anchor().syntax(), id);
            }
        }
    }

    targets
}
