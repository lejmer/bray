use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, DeclaredValueTypeConstraintKind, DeclaredValueTypeEvidence,
    DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{TypeData, TypeId};

use super::template::{TemplateResolution, resolve_type_template};
use crate::type_check::SessionProgress;
use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerUnitView, ExpressionTypeEvidence,
    ExpressionTypeExpectation, ExpressionTypeInput,
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

    for constraint in declared.constraints() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        components.insert(constraint.left());
        components.insert(constraint.right());

        if constraint.kind() != DeclaredValueTypeConstraintKind::Initializer {
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

    let mut expectations = Vec::new();
    let mut inferred_initializers = Vec::new();

    // Nullable destinations admit a present initializer without changing its type.
    // Identity-only destinations and inferred aliases keep their equality constraints.
    for constraint in declared
        .constraints()
        .iter()
        .filter(|constraint| constraint.kind() == DeclaredValueTypeConstraintKind::Initializer)
    {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let (expression, target) = match (constraint.left(), constraint.right()) {
            (DeclaredValueTypeTerm::Expression(expression), target) => (expression, target),
            (target, DeclaredValueTypeTerm::Expression(expression)) => (expression, target),
            _ => {
                inferred_initializers.push((constraint.left(), constraint.right()));
                continue;
            }
        };

        let target = components.representative(target);

        if let Some(types) = component_types.get(&target)
            && types.iter().any(|ty| {
                matches!(
                    request.semantic_values().type_data(*ty).as_ref(),
                    TypeData::Nullable(_)
                )
            })
        {
            expectations.extend(
                types
                    .iter()
                    .copied()
                    .map(|ty| ExpressionTypeExpectation::new(expression, ty)),
            );
        } else {
            inferred_initializers.push((constraint.left(), constraint.right()));
        }
    }

    for (left, right) in inferred_initializers {
        components.union(left, right);
    }

    // Close inferred aliases once, reusing the types already resolved above.
    let mut inferred_types = BTreeMap::<DeclaredValueTypeTerm, BTreeSet<TypeId>>::new();

    for (term, types) in component_types {
        inferred_types
            .entry(components.representative(term))
            .or_default()
            .extend(types);
    }

    let component_types = inferred_types;

    let unsupported = unsupported
        .into_iter()
        .map(|term| components.representative(term))
        .collect::<BTreeSet<_>>();

    let mut deferred = BTreeSet::new();
    let mut evidence = Vec::new();
    let mut box_storage_policies = Vec::new();

    for term in components.terms() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let expression = match term {
            DeclaredValueTypeTerm::Expression(expression)
            | DeclaredValueTypeTerm::BoxStoragePolicy(expression) => expression,
            _ => continue,
        };

        let representative = components.representative(term);

        if unsupported.contains(&representative) {
            deferred.insert(expression);

            continue;
        }

        if matches!(term, DeclaredValueTypeTerm::BoxStoragePolicy(_)) {
            box_storage_policies.extend(
                component_types
                    .get(&representative)
                    .into_iter()
                    .flat_map(|types| types.iter().copied())
                    .map(|ty| (expression, ty)),
            );

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

    let mut input = ExpressionTypeInput::new()
        .with_evidence(evidence)
        .with_box_storage_policies(box_storage_policies)
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
