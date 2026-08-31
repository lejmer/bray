use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, DeclaredValueTypeConstraintKind, DeclaredValueTypeEvidence,
    DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{TypeData, TypeId};

use super::template::{TemplateResolution, resolve_type_template};
use crate::representation::representation_type;
use crate::type_check::{SessionProgress, intrinsic_representation_role};
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
    ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput,
};

pub(super) struct PreparedDeclaredTypes {
    pub(super) input: ExpressionTypeInput,
    pub(super) deferred: BTreeSet<BoundExpressionId>,
    pub(super) unsupported_callable_result: bool,
    pub(super) diagnostics: DiagnosticBag,
}

#[derive(Default)]
struct TypeTermComponents {
    parents: BTreeMap<DeclaredValueTypeTerm, DeclaredValueTypeTerm>,
    sizes: BTreeMap<DeclaredValueTypeTerm, usize>,
}

impl TypeTermComponents {
    fn insert(&mut self, term: DeclaredValueTypeTerm) {
        self.parents.entry(term).or_insert(term);
        self.sizes.entry(term).or_insert(1);
    }

    fn representative(&self, mut term: DeclaredValueTypeTerm) -> DeclaredValueTypeTerm {
        while self.parents[&term] != term {
            term = self.parents[&term];
        }

        term
    }

    fn union(&mut self, left: DeclaredValueTypeTerm, right: DeclaredValueTypeTerm) {
        self.insert(left);
        self.insert(right);

        let left = self.representative(left);
        let right = self.representative(right);

        if left == right {
            return;
        }

        let left_size = self.sizes[&left];
        let right_size = self.sizes[&right];

        let (parent, child) = if left_size > right_size || left_size == right_size && left < right {
            (left, right)
        } else {
            (right, left)
        };

        self.parents.insert(child, parent);
        self.sizes.insert(parent, left_size + right_size);
        self.sizes.remove(&child);
    }

    fn terms(&self) -> impl Iterator<Item = DeclaredValueTypeTerm> + '_ {
        self.parents.keys().copied()
    }
}

pub(super) fn prepare_declared_types<C>(
    request: CheckerUnitView<'_, C>,
    declared: &DeclaredValueTypeTemplates,
    supplemental: &[DeclaredValueTypeEvidence],
) -> Result<SessionProgress<PreparedDeclaredTypes>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut components = TypeTermComponents::default();
    let mut diagnostics = DiagnosticBag::new();

    for evidence in declared.evidence().iter().chain(supplemental) {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        components.insert(evidence.term());
    }

    for constraint in declared
        .constraints()
        .iter()
        .filter(|constraint| constraint.kind() != DeclaredValueTypeConstraintKind::Initializer)
    {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        components.union(constraint.left(), constraint.right());
    }

    let mut initializer_expectations = Vec::new();

    for constraint in declared
        .constraints()
        .iter()
        .filter(|constraint| constraint.kind() == DeclaredValueTypeConstraintKind::Initializer)
    {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        components.insert(constraint.left());
        components.insert(constraint.right());

        let (expression, target) = match (constraint.left(), constraint.right()) {
            (DeclaredValueTypeTerm::Expression(expression), target) => (expression, target),
            (target, DeclaredValueTypeTerm::Expression(expression)) => (expression, target),
            _ => {
                components.union(constraint.left(), constraint.right());

                continue;
            }
        };

        let target = components.representative(target);
        let initializer_type = intrinsic_initializer_type(request, expression)?;
        let mut initializes_nullable_from_present = false;

        for evidence in declared
            .evidence()
            .iter()
            .filter(|evidence| components.representative(evidence.term()) == target)
        {
            let TemplateResolution::Resolved(declared_type) =
                resolve_type_template(request, evidence.template(), &mut diagnostics)?
            else {
                continue;
            };

            let data = request
                .semantic_values()
                .type_data(declared_type)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            initializes_nullable_from_present = matches!((data.as_ref(), initializer_type), (TypeData::Nullable(contained), Some(actual)) if *contained == actual)
                || matches!(data.as_ref(), TypeData::Nullable(_))
                    && is_contextual_numeric_literal(request, expression);

            if initializes_nullable_from_present {
                break;
            }
        }

        if initializes_nullable_from_present {
            initializer_expectations.push((expression, target));
        } else {
            components.union(constraint.left(), constraint.right());
        }
    }

    let mut component_types = BTreeMap::<DeclaredValueTypeTerm, BTreeSet<TypeId>>::new();
    let mut unsupported = BTreeSet::new();

    for evidence in declared.evidence().iter().chain(supplemental) {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let representative = components.representative(evidence.term());

        match resolve_type_template(request, evidence.template(), &mut diagnostics)? {
            TemplateResolution::Resolved(ty) => {
                component_types
                    .entry(representative)
                    .or_default()
                    .insert(ty);
            }
            TemplateResolution::Unsupported => {
                unsupported.insert(representative);
            }
        }
    }

    let mut deferred = BTreeSet::new();
    let mut evidence = Vec::new();
    let mut expectations = Vec::new();

    for term in components.terms() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let DeclaredValueTypeTerm::Expression(expression) = term else {
            continue;
        };

        let representative = components.representative(term);

        if unsupported.contains(&representative) {
            deferred.insert(expression);

            continue;
        }

        evidence.extend(
            component_types
                .get(&representative)
                .into_iter()
                .flat_map(|types| types.iter().copied())
                .map(|ty| ExpressionTypeEvidence::new(expression, ty)),
        );
    }

    for (expression, target) in initializer_expectations {
        let representative = components.representative(target);

        if unsupported.contains(&representative) {
            deferred.insert(expression);

            continue;
        }

        expectations.extend(
            component_types
                .get(&representative)
                .into_iter()
                .flat_map(|types| types.iter().copied())
                .map(|ty| ExpressionTypeExpectation::new(expression, ty)),
        );
    }

    let mut input = ExpressionTypeInput::new()
        .with_evidence(evidence)
        .with_expectations(expectations);

    let mut unsupported_callable_result = false;

    if let Some(result) = declared.callable_result() {
        match resolve_type_template(request, result, &mut diagnostics)? {
            TemplateResolution::Resolved(result) => {
                input = input.with_callable_result_type(result);
            }
            TemplateResolution::Unsupported => unsupported_callable_result = true,
        }
    }

    Ok(SessionProgress::Complete(PreparedDeclaredTypes {
        input,
        deferred,
        unsupported_callable_result,
        diagnostics,
    }))
}

fn intrinsic_initializer_type<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Result<Option<TypeId>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression }.into());
    };

    if let Some(ty) = expression.ty() {
        return Ok(Some(ty));
    }

    if let BoundExpression::Conversion(conversion) = expression {
        return Ok(conversion.target_type());
    }

    intrinsic_representation_role(expression)
        .map(|role| representation_type(request, role))
        .transpose()
        .map_err(CheckerQueryError::Infrastructure)
}

fn is_contextual_numeric_literal<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    match request.view().expression(expression) {
        Some(BoundExpression::Literal(literal)) => matches!(
            literal.kind(),
            bray_bound_tree::BoundLiteralKind::Integer
                | bray_bound_tree::BoundLiteralKind::Real
                | bray_bound_tree::BoundLiteralKind::Imaginary
        ),
        Some(BoundExpression::Unary(unary))
            if matches!(
                unary.operator(),
                bray_bound_tree::BoundOperator::Add
                    | bray_bound_tree::BoundOperator::Subtract
                    | bray_bound_tree::BoundOperator::BitwiseNot
            ) =>
        {
            unary
                .operands()
                .first()
                .is_some_and(|operand| is_contextual_numeric_literal(request, *operand))
        }
        _ => false,
    }
}

pub(super) fn defer_return_operands<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    deferred: &mut BTreeSet<BoundExpressionId>,
) where
    C: CheckerRequestContext + ?Sized,
{
    for &expression in expressions {
        let Some(BoundExpression::ControlTransfer(transfer)) =
            request.view().expression(expression)
        else {
            continue;
        };

        if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Return
            && let Some(operand) = transfer.operand()
        {
            deferred.insert(operand);
        }
    }
}
