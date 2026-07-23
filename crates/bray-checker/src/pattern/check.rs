use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundPattern, BoundPatternEntryKind,
    BoundPatternId, BoundPatternKind, BoundPatternMode, BoundPatternTarget,
    BoundStructuredExpressionKind, BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes,
    CheckedPatternFacts, MatchCoverageEntry, PatternBindingTypeEntry, PatternCheckEntry,
    PatternRefutability, walk_bound_unit_view,
};
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{AnySymbolId, NamedTypeSymbolId, TypeData, TypeId};

use super::input::PatternCheckInput;
use crate::diagnostic::{diagnostic_id, pattern_span};
use crate::type_check::diagnostic_type;
use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

pub(crate) fn check_patterns<C>(
    request: CheckerUnitView<'_, C>,
    expression_types: &CheckedExpressionTypes,
    input: &PatternCheckInput,
) -> CheckerOutcome<CheckedPatternFacts>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut checker = match PatternChecker::new(request, expression_types, input) {
        Ok(checker) => checker,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    if checker.request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if let Err(error) = checker.collect_subjects() {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    if checker.request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if let Err(error) = checker.check_subjects() {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    if checker.request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if let Err(error) = checker.check_matches() {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    if checker.request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    CheckerOutcome::complete(
        CheckedPatternFacts::new(
            checker.request.view().unit(),
            checker.request.view().kind(),
            checker.patterns.into_values(),
            checker.binding_types.into_values(),
            checker.matches,
        ),
        DiagnosticBag::from(checker.diagnostics),
    )
}

#[derive(Clone, Copy)]
pub(super) struct PatternSubject {
    pub(super) ty: TypeId,
    pub(super) is_recovered: bool,
}

pub(super) struct PatternChecker<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'view, C>,
    expression_types: &'view CheckedExpressionTypes,
    iteration_patterns: BTreeMap<BoundPatternId, PatternSubject>,
    subjects: BTreeMap<BoundPatternId, PatternSubject>,
    pub(super) patterns: BTreeMap<BoundPatternId, PatternCheckEntry>,
    binding_types: BTreeMap<bray_symbols::LocalBindingSymbolId, PatternBindingTypeEntry>,
    pub(super) matches: Vec<MatchCoverageEntry>,
    pub(super) diagnostics: Vec<Diagnostic>,
    error_type: TypeId,
}

impl<'view, C> PatternChecker<'view, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new(
        request: CheckerUnitView<'view, C>,
        expression_types: &'view CheckedExpressionTypes,
        input: &PatternCheckInput,
    ) -> Result<Self, CheckerInfrastructureError> {
        let error_type = request
            .semantic_values()
            .intern_type(TypeData::Error)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let iteration_patterns = input
            .iteration_patterns()
            .iter()
            .map(|input| {
                (
                    input.pattern(),
                    PatternSubject {
                        ty: input.element_type(),
                        is_recovered: input.is_recovered(),
                    },
                )
            })
            .collect();

        Ok(Self {
            request,
            expression_types,
            iteration_patterns,
            subjects: BTreeMap::new(),
            patterns: BTreeMap::new(),
            binding_types: BTreeMap::new(),
            matches: Vec::new(),
            diagnostics: Vec::new(),
            error_type,
        })
    }

    fn collect_subjects(&mut self) -> Result<(), CheckerInfrastructureError> {
        let mut failure = None;

        walk_bound_unit_view(self.request.view(), self.request.unit().root(), |event| {
            if self.request.is_cancelled() {
                return BoundWalkControl::Stop;
            }

            match event {
                BoundWalkEvent::Enter(AnyBoundNodeId::Block(block)) => {
                    let Some(block) = self.request.view().block(block) else {
                        failure = Some(CheckerInfrastructureError::InvalidBoundNode {
                            node: block.into(),
                        });

                        return BoundWalkControl::Stop;
                    };

                    for item in block.items() {
                        let bray_bound_tree::BoundBlockItem::LocalBinding(binding) = item else {
                            continue;
                        };

                        match self.expression_type(binding.initializer()) {
                            Ok(subject) => {
                                self.subjects.insert(binding.pattern(), subject);
                            }
                            Err(error) => {
                                failure = Some(error);

                                return BoundWalkControl::Stop;
                            }
                        }
                    }
                }
                BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) => {
                    let Some(bound) = self.request.view().expression(expression) else {
                        failure = Some(CheckerInfrastructureError::InvalidBoundNode {
                            node: expression.into(),
                        });

                        return BoundWalkControl::Stop;
                    };

                    if let Err(error) = self.collect_expression_subjects(bound) {
                        failure = Some(error);

                        return BoundWalkControl::Stop;
                    }
                }
                BoundWalkEvent::Enter(_) | BoundWalkEvent::Exit(_) => {}
            }

            BoundWalkControl::Continue
        });

        if self.request.is_cancelled() {
            return Ok(());
        }

        failure.map_or(Ok(()), Err)
    }

    fn collect_expression_subjects(
        &mut self,
        expression: &BoundExpression,
    ) -> Result<(), CheckerInfrastructureError> {
        match expression {
            BoundExpression::Match(expression) => {
                let subject = self.expression_type(expression.subject())?;

                for arm in expression.arms() {
                    self.subjects.insert(arm.pattern(), subject);
                }
            }
            BoundExpression::For(expression) => {
                self.collect_iteration_subject(expression.pattern());
            }
            BoundExpression::Generator(expression) => {
                self.collect_iteration_subject(expression.pattern());
            }
            BoundExpression::Structured(expression)
                if expression.kind() == BoundStructuredExpressionKind::With =>
            {
                let Some(operand) = expression.operands().first().copied() else {
                    return Ok(());
                };

                let subject = self.expression_type(operand)?;

                for pattern in expression.patterns() {
                    self.subjects.insert(*pattern, subject);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn collect_iteration_subject(&mut self, pattern: BoundPatternId) {
        let subject = self
            .iteration_patterns
            .get(&pattern)
            .copied()
            .unwrap_or(PatternSubject {
                ty: self.error_type,
                is_recovered: true,
            });

        self.subjects.insert(pattern, subject);
    }

    pub(super) fn expression_type(
        &self,
        expression: BoundExpressionId,
    ) -> Result<PatternSubject, CheckerInfrastructureError> {
        let Some(result) = self.expression_types.expression(expression) else {
            return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
        };

        Ok(PatternSubject {
            ty: result.ty(),
            is_recovered: result.is_recovered(),
        })
    }

    fn check_subjects(&mut self) -> Result<(), CheckerInfrastructureError> {
        let subjects = self
            .subjects
            .iter()
            .map(|(pattern, subject)| (*pattern, *subject))
            .collect::<Vec<_>>();

        for (pattern, subject) in subjects {
            if self.request.is_cancelled() {
                break;
            }

            self.check_pattern(pattern, subject)?;
        }

        Ok(())
    }

    fn check_pattern(
        &mut self,
        id: BoundPatternId,
        subject: PatternSubject,
    ) -> Result<PatternCheckEntry, CheckerInfrastructureError> {
        if let Some(entry) = self.patterns.get(&id).copied() {
            return Ok(entry);
        }

        let Some(pattern) = self.request.view().pattern(id) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode { node: id.into() });
        };

        let type_data = self
            .request
            .semantic_values()
            .type_data(subject.ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let target = self.pattern_target(pattern, type_data.as_ref());
        let kind = effective_pattern_kind(pattern, target);
        let compatible = self.pattern_is_compatible(pattern, kind, target, type_data.as_ref())?;
        let child_subjects = self.child_subjects(pattern, subject, type_data.as_ref());
        let mut child_refutability = Vec::with_capacity(pattern.children().len());
        let mut child_recovered = false;

        for (child, subject) in pattern.children().iter().copied().zip(child_subjects) {
            let checked = self.check_pattern(child, subject)?;

            child_refutability.push(checked.refutability());
            child_recovered |= checked.is_recovered();
        }

        self.record_bindings(pattern, subject, kind);
        self.record_entry_bindings(pattern, type_data.as_ref());

        let is_recovered = subject.is_recovered
            || pattern.is_recovered()
            || pattern
                .entries()
                .iter()
                .any(|entry| entry.kind() == BoundPatternEntryKind::Recovered)
            || child_recovered
            || matches!(type_data.as_ref(), TypeData::Error)
            || !compatible;

        let shape_is_total =
            self.pattern_shape_is_total(pattern, kind, target, type_data.as_ref(), compatible);

        let refutability = pattern_refutability(
            kind,
            &child_refutability,
            compatible,
            shape_is_total,
            is_recovered,
        );

        if !compatible && !matches!(type_data.as_ref(), TypeData::Error) {
            self.report_incompatible(id, subject.ty)?;
        } else if pattern.mode() != BoundPatternMode::Match
            && refutability == PatternRefutability::Refutable
        {
            self.report(
                id,
                DiagnosticKind::CheckingRefutablePattern,
                SeverityKind::Error,
            )?;
        }

        let entry = PatternCheckEntry::new(id, subject.ty, refutability, target, is_recovered);

        self.patterns.insert(id, entry);

        Ok(entry)
    }

    fn pattern_target(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Option<BoundPatternTarget> {
        if let Some(target) = pattern.target()
            && !self.variant_target_conflicts_with_subject(target, subject)
        {
            return Some(target);
        }

        let (TypeData::Named { definition, .. }, Some(name)) = (subject, pattern.name()) else {
            return None;
        };

        let NamedTypeSymbolId::Union(union) = definition else {
            return None;
        };

        let bray_symbols::MemberLookupResult::Found(AnySymbolId::UnionVariant(variant)) = self
            .request
            .symbols()
            .lookup_member((*union).into(), name.as_str())
        else {
            return None;
        };

        Some(BoundPatternTarget::Surface(variant.into()))
    }

    fn variant_target_conflicts_with_subject(
        &self,
        target: BoundPatternTarget,
        subject: &TypeData,
    ) -> bool {
        let (
            BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant)),
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            },
        ) = (target, subject)
        else {
            return false;
        };

        self.request
            .symbols()
            .union_variant(variant)
            .is_none_or(|record| record.union() != *union)
    }

    fn child_subjects(
        &self,
        pattern: &BoundPattern,
        parent: PatternSubject,
        subject: &TypeData,
    ) -> Vec<PatternSubject> {
        let types = match (pattern.kind(), subject) {
            (BoundPatternKind::Grouped | BoundPatternKind::Alternative, _) => {
                vec![parent.ty; pattern.children().len()]
            }
            (BoundPatternKind::NullablePresent, TypeData::Nullable(target))
            | (BoundPatternKind::Box, TypeData::OwnedIndirection { target, .. }) => {
                vec![*target; pattern.children().len()]
            }
            (BoundPatternKind::Tuple, TypeData::Tuple(elements)) => elements.to_vec(),
            (BoundPatternKind::Array, TypeData::Array { element, .. })
            | (BoundPatternKind::Array, TypeData::Slice(element)) => {
                vec![*element; pattern.children().len()]
            }
            _ => vec![self.error_type; pattern.children().len()],
        };

        types
            .into_iter()
            .map(|ty| PatternSubject {
                ty,
                is_recovered: parent.is_recovered || ty == self.error_type,
            })
            .collect()
    }

    fn pattern_shape_is_total(
        &self,
        pattern: &BoundPattern,
        kind: BoundPatternKind,
        target: Option<BoundPatternTarget>,
        subject: &TypeData,
        compatible: bool,
    ) -> bool {
        if !compatible {
            return false;
        }

        match (kind, subject) {
            (BoundPatternKind::Product | BoundPatternKind::Tuple, _) => true,
            (BoundPatternKind::Array, TypeData::Array { .. }) => true,
            (BoundPatternKind::Array, TypeData::Slice(_)) => {
                pattern.entries().len() == 1 && pattern.entries()[0].is_remaining()
            }
            (
                BoundPatternKind::Variant,
                TypeData::Named {
                    definition: NamedTypeSymbolId::Union(union),
                    ..
                },
            ) if pattern.children().is_empty() && pattern.entries().is_empty() => {
                let Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))) = target
                else {
                    return false;
                };

                self.request
                    .symbols()
                    .union(*union)
                    .is_some_and(|record| record.variants() == [variant])
            }
            _ => false,
        }
    }

    fn record_bindings(
        &mut self,
        pattern: &BoundPattern,
        subject: PatternSubject,
        kind: BoundPatternKind,
    ) {
        if kind != BoundPatternKind::Binding {
            return;
        }

        for binding in pattern.bindings() {
            self.binding_types.insert(
                *binding,
                PatternBindingTypeEntry::new(*binding, subject.ty, subject.is_recovered),
            );
        }
    }

    fn record_entry_bindings(&mut self, pattern: &BoundPattern, subject: &TypeData) {
        let mut position = 0_usize;

        for entry in pattern.entries() {
            if entry.is_remaining() {
                continue;
            }

            let Some(binding) = entry.binding() else {
                position += 1;

                continue;
            };

            let ty = match subject {
                TypeData::Tuple(elements) => {
                    elements.get(position).copied().unwrap_or(self.error_type)
                }
                TypeData::Array { element, .. } | TypeData::Slice(element) => *element,
                _ => self.error_type,
            };

            self.binding_types.insert(
                binding,
                PatternBindingTypeEntry::new(binding, ty, ty == self.error_type),
            );

            position += 1;
        }
    }

    fn report_incompatible(
        &mut self,
        pattern: BoundPatternId,
        actual: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        let actual = diagnostic_type(self.request, actual)?;
        let span = pattern_span(self.request, pattern)?;

        self.diagnostics.push(
            Diagnostic::new(
                diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::CheckingIncompatiblePattern,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_arg(DiagnosticArg::actual_type(actual)),
        );

        Ok(())
    }

    pub(super) fn report(
        &mut self,
        pattern: BoundPatternId,
        kind: DiagnosticKind,
        severity: SeverityKind,
    ) -> Result<(), CheckerInfrastructureError> {
        let span = pattern_span(self.request, pattern)?;

        self.diagnostics.push(
            Diagnostic::new(diagnostic_id(self.diagnostics.len()), kind, severity)
                .with_primary_span(span),
        );

        Ok(())
    }
}

fn pattern_refutability(
    kind: BoundPatternKind,
    children: &[PatternRefutability],
    is_compatible: bool,
    shape_is_total: bool,
    is_recovered: bool,
) -> PatternRefutability {
    match kind {
        BoundPatternKind::Binding | BoundPatternKind::Discard | BoundPatternKind::Remaining => {
            PatternRefutability::Irrefutable
        }
        BoundPatternKind::Error => PatternRefutability::Recovered,
        _ if is_recovered => PatternRefutability::Recovered,
        BoundPatternKind::Product
        | BoundPatternKind::Tuple
        | BoundPatternKind::Array
        | BoundPatternKind::Variant => {
            if is_compatible
                && shape_is_total
                && children
                    .iter()
                    .all(|child| *child == PatternRefutability::Irrefutable)
            {
                PatternRefutability::Irrefutable
            } else {
                PatternRefutability::Refutable
            }
        }
        BoundPatternKind::Grouped | BoundPatternKind::Box => {
            if children
                .iter()
                .all(|child| *child == PatternRefutability::Irrefutable)
            {
                PatternRefutability::Irrefutable
            } else {
                PatternRefutability::Refutable
            }
        }
        BoundPatternKind::Alternative
        | BoundPatternKind::Literal
        | BoundPatternKind::NullableAbsent
        | BoundPatternKind::NullablePresent
        | BoundPatternKind::Path => PatternRefutability::Refutable,
    }
}

pub(super) fn effective_pattern_kind(
    pattern: &BoundPattern,
    target: Option<BoundPatternTarget>,
) -> BoundPatternKind {
    if matches!(
        target,
        Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(_)))
    ) {
        BoundPatternKind::Variant
    } else {
        pattern.kind()
    }
}
