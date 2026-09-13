use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};
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
    pub(super) projections:
        BTreeMap<(BoundExpressionId, bray_symbols::DependencyProjection), BoundExpressionId>,
    pub(super) aliases: BTreeMap<BoundExpressionId, BoundExpressionId>,
    expressions: BTreeMap<BoundExpressionId, Vec<BoundExpressionId>>,
}

impl ValueInputs {
    pub(crate) fn new(unit: &BoundUnit, selections: &CheckedSemanticSelections) -> Self {
        let mut expressions = BTreeMap::<_, Vec<_>>::new();
        let mut targets = BTreeMap::new();

        for (id, expression) in unit.tree().expressions() {
            targets.insert(expression.origin().source_anchor().syntax(), id);

            for block in expression.child_blocks() {
                if let Some(block) = unit.tree().block(block) {
                    targets.insert(block.origin().source_anchor().syntax(), id);
                }
            }

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
            expressions,
            independent: Default::default(),
            projections: BTreeMap::new(),
            aliases: BTreeMap::new(),
        };

        result.collect_projections(unit, selections);

        result
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

    pub(crate) fn operands(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = BoundExpressionId> + '_ {
        self.expressions
            .get(&expression)
            .into_iter()
            .flatten()
            .copied()
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

        let data = request
            .semantic_values()
            .type_data(ty)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

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
                .generic_substitution_data(substitution)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

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
