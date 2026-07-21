use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
use bray_symbols::TypeId;

use super::template::{TemplateResolution, resolve_type_template};
use crate::type_check::SessionProgress;
use crate::{
    CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView, ExpressionTypeEvidence,
    ExpressionTypeInput,
};

pub(super) struct PreparedDeclaredTypes {
    pub(super) input: ExpressionTypeInput,
    pub(super) deferred: BTreeSet<BoundExpressionId>,
    pub(super) unsupported_callable_result: bool,
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
) -> Result<SessionProgress<PreparedDeclaredTypes>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut components = TypeTermComponents::default();

    for evidence in declared.evidence() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        components.insert(evidence.term());
    }

    for constraint in declared.constraints() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        components.union(constraint.left(), constraint.right());
    }

    let mut component_types = BTreeMap::<DeclaredValueTypeTerm, BTreeSet<TypeId>>::new();
    let mut unsupported = BTreeSet::new();

    for evidence in declared.evidence() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let representative = components.representative(evidence.term());

        match resolve_type_template(request.semantic_values(), evidence.template())? {
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

    let mut input = ExpressionTypeInput::new().with_evidence(evidence);
    let mut unsupported_callable_result = false;

    if let Some(result) = declared.callable_result() {
        match resolve_type_template(request.semantic_values(), result)? {
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
    }))
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
