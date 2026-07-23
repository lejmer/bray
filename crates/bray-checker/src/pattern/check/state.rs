use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundPattern, BoundPatternEntryKind,
    BoundPatternId, BoundPatternKind, BoundPatternTarget, BoundStructuredExpressionKind,
    BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes, CheckedPatternFacts,
    MatchCoverageEntry, PatternBindingTypeEntry, PatternCheckEntry, PatternProjection,
    PatternRefutability, walk_bound_unit_view,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{
    AnySymbolId, MemberLookupResult, NamedTypeSymbolId, StructFieldTypeFact, TypeData, TypeId,
    UnionPayloadFieldTypeFact, UnionVariantSymbolId,
};

use super::result::{
    effective_pattern_kind, pattern_mode_accepts_refutable, pattern_operation, pattern_refutability,
};
use crate::pattern::input::PatternCheckInput;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerSemanticFactProvider,
    CheckerUnitView,
};

pub(crate) fn check_patterns<C>(
    request: CheckerUnitView<'_, C>,
    expression_types: &CheckedExpressionTypes,
    input: &PatternCheckInput,
) -> CheckerOutcome<CheckedPatternFacts>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
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
pub(in crate::pattern) struct PatternSubject {
    pub(in crate::pattern) ty: TypeId,
    pub(in crate::pattern) is_recovered: bool,
}

pub(super) struct PatternChildren {
    pub(super) patterns: BTreeMap<BoundPatternId, (PatternSubject, Option<PatternProjection>)>,
    pub(super) entries: Vec<(PatternSubject, Option<PatternProjection>)>,
    pub(super) is_recovered: bool,
}

pub(in crate::pattern) struct PatternChecker<'view, C>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
{
    pub(in crate::pattern) request: CheckerUnitView<'view, C>,
    expression_types: &'view CheckedExpressionTypes,
    iteration_patterns: BTreeMap<BoundPatternId, PatternSubject>,
    subjects: BTreeMap<BoundPatternId, PatternSubject>,
    pub(in crate::pattern) patterns: BTreeMap<BoundPatternId, PatternCheckEntry>,
    pub(super) binding_types: BTreeMap<bray_symbols::LocalBindingSymbolId, PatternBindingTypeEntry>,
    pub(in crate::pattern) matches: Vec<MatchCoverageEntry>,
    pub(in crate::pattern) diagnostics: Vec<Diagnostic>,
    pub(super) error_type: TypeId,
}

impl<'view, C> PatternChecker<'view, C>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
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

    pub(in crate::pattern) fn expression_type(
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

            self.check_pattern(pattern, subject, None)?;
        }

        Ok(())
    }

    fn check_pattern(
        &mut self,
        id: BoundPatternId,
        subject: PatternSubject,
        projection: Option<PatternProjection>,
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

        let contextual_variant = pattern.kind() == BoundPatternKind::Binding
            && pattern_mode_accepts_refutable(pattern.mode())
            && self
                .expected_subject_variant(pattern, type_data.as_ref())
                .is_some();

        let target = if contextual_variant {
            None
        } else {
            self.pattern_target(pattern, type_data.as_ref())
        };

        let kind = effective_pattern_kind(pattern, target);
        let compatible = self.pattern_is_compatible(pattern, kind, target, type_data.as_ref())?;
        let children = self.child_subjects(pattern, subject, target, type_data.as_ref())?;
        let mut child_refutability = Vec::with_capacity(pattern.children().len());
        let mut child_recovered = false;

        for child in pattern.children().iter().copied() {
            let (subject, projection) = children
                .patterns
                .get(&child)
                .copied()
                .unwrap_or((self.recovered_subject(), None));

            let checked = self.check_pattern(child, subject, projection)?;

            child_refutability.push(checked.refutability());
            child_recovered |= checked.is_recovered();
        }

        let is_recovered = subject.is_recovered
            || pattern.is_recovered()
            || pattern
                .entries()
                .iter()
                .any(|entry| entry.kind() == BoundPatternEntryKind::Recovered)
            || child_recovered
            || children.is_recovered
            || contextual_variant
            || matches!(type_data.as_ref(), TypeData::Error)
            || !compatible;

        let operation = pattern_operation(pattern.mode(), is_recovered);

        if !contextual_variant {
            self.record_bindings(pattern, subject, kind, operation, projection);
        }

        self.record_entry_bindings(pattern, operation, &children.entries);

        let shape_is_total =
            self.pattern_shape_is_total(pattern, kind, target, type_data.as_ref(), compatible);

        let refutability = pattern_refutability(
            kind,
            &child_refutability,
            compatible,
            shape_is_total,
            is_recovered,
        );

        if contextual_variant {
            // TODO(pattern): Resolve bare variant names before provisional bindings become visible.
            self.report(
                id,
                DiagnosticKind::CheckingContextualPatternNameUnsupported,
                SeverityKind::Error,
            )?;
        } else if !compatible && !matches!(type_data.as_ref(), TypeData::Error) {
            self.report_incompatible(id, subject.ty)?;
        } else if !pattern_mode_accepts_refutable(pattern.mode())
            && refutability == PatternRefutability::Refutable
        {
            self.report(
                id,
                DiagnosticKind::CheckingRefutablePattern,
                SeverityKind::Error,
            )?;
        }

        let predicate = self.pattern_predicate(pattern, kind, target, type_data.as_ref())?;

        let entry = PatternCheckEntry::new(id, subject.ty, operation, refutability, target)
            .with_test((!is_recovered).then_some(predicate).flatten())
            .with_projection(projection)
            .with_refinement((!is_recovered).then_some(predicate).flatten());

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

        if pattern.kind() != BoundPatternKind::Variant {
            return None;
        }

        self.expected_subject_variant(pattern, subject)
            .map(|variant| BoundPatternTarget::Surface(variant.into()))
    }

    fn expected_subject_variant(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Option<UnionVariantSymbolId> {
        let (
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            },
            Some(name),
        ) = (subject, pattern.name())
        else {
            return None;
        };

        let MemberLookupResult::Found(AnySymbolId::UnionVariant(variant)) = self
            .request
            .symbols()
            .lookup_member((*union).into(), name.as_str())
        else {
            return None;
        };

        Some(variant)
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
}
