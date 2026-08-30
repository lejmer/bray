use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundPattern, BoundPatternEntryKind,
    BoundPatternId, BoundPatternKind, BoundPatternTarget, BoundStructuredExpressionKind,
    BoundWalkControl, BoundWalkEvent, CheckedExpressionTypes, CheckedPatterns, MatchCoverageEntry,
    PatternBindingTypeEntry, PatternCheckEntry, PatternProjection, PatternRefutability,
    walk_bound_unit_view,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag};
use bray_symbols::{
    AnySymbolId, MemberLookupResult, NamedTypeSymbolId, StructFieldTypeQuery, TypeData, TypeId,
    UnionPayloadFieldTypeQuery, UnionVariantSymbolId,
};

use super::result::{
    effective_pattern_kind, pattern_mode_accepts_refutable, pattern_operation, pattern_refutability,
};
use crate::expression::{TemplateResolution, resolve_type_template};
use crate::pattern::input::{PatternCheckInput, PatternConstantEvidence};
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerQueryResult,
    CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView,
};

pub(crate) fn check_patterns<C>(
    request: CheckerUnitView<'_, C>,
    expression_types: &CheckedExpressionTypes,
    input: &PatternCheckInput,
) -> CheckerOutcome<CheckedPatterns>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
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
        CheckedPatterns::new(
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
    pub(in crate::pattern) trusted_variant: bool,
}

pub(super) struct PatternChildren {
    pub(super) patterns: BTreeMap<BoundPatternId, (PatternSubject, Option<PatternProjection>)>,
    pub(super) entries: Vec<(PatternSubject, Option<PatternProjection>)>,
    pub(super) is_recovered: bool,
}

pub(in crate::pattern) fn available_dependency<T>(
    result: CheckerQueryResult<T>,
) -> Result<Option<T>, CheckerInfrastructureError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(CheckerQueryError::Cancelled) => Ok(None),
        Err(CheckerQueryError::Infrastructure(error)) => Err(error),
    }
}

pub(in crate::pattern) struct PatternChecker<'view, 'input, C>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
        + ?Sized,
{
    pub(in crate::pattern) request: CheckerUnitView<'view, C>,
    expression_types: &'view CheckedExpressionTypes,
    iteration_patterns: BTreeMap<BoundPatternId, PatternSubject>,
    declared_patterns: BTreeMap<BoundPatternId, TypeId>,
    pub(in crate::pattern) constant_patterns:
        &'input BTreeMap<BoundPatternId, PatternConstantEvidence>,
    pub(in crate::pattern) constant_guards:
        &'input BTreeMap<BoundExpressionId, bray_symbols::ConstantValueId>,
    subjects: BTreeMap<BoundPatternId, PatternSubject>,
    pub(in crate::pattern) patterns: BTreeMap<BoundPatternId, PatternCheckEntry>,
    pub(super) binding_types: BTreeMap<bray_symbols::LocalBindingSymbolId, PatternBindingTypeEntry>,
    pub(in crate::pattern) matches: Vec<MatchCoverageEntry>,
    pub(in crate::pattern) diagnostics: Vec<Diagnostic>,
    pub(super) error_type: TypeId,
}

impl<'view, 'input, C> PatternChecker<'view, 'input, C>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
        + ?Sized,
{
    fn new(
        request: CheckerUnitView<'view, C>,
        expression_types: &'view CheckedExpressionTypes,
        input: &'input PatternCheckInput,
    ) -> Result<Self, CheckerInfrastructureError> {
        if !input.is_consistent() {
            return Err(CheckerInfrastructureError::InvalidPatternCheckInput);
        }

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
                        trusted_variant: false,
                    },
                )
            })
            .collect();

        let mut diagnostics = DiagnosticBag::new();
        let mut declared_patterns = BTreeMap::new();

        for (&pattern, template) in input.declared_patterns() {
            if let TemplateResolution::Resolved(ty) =
                resolve_type_template(request, template, &mut diagnostics)?
            {
                declared_patterns.insert(pattern, ty);
            }
        }

        Ok(Self {
            request,
            expression_types,
            iteration_patterns,
            declared_patterns,
            constant_patterns: input.constant_patterns(),
            constant_guards: input.constant_guards(),
            subjects: BTreeMap::new(),
            patterns: BTreeMap::new(),
            binding_types: BTreeMap::new(),
            matches: Vec::new(),
            diagnostics: diagnostics.into_vec(),
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
                            Ok(mut subject) => {
                                if let Some(declared) =
                                    self.declared_patterns.get(&binding.pattern()).copied()
                                {
                                    let Ok(data) =
                                        self.request.semantic_values().type_data(declared)
                                    else {
                                        failure = Some(
                                            CheckerInfrastructureError::SemanticValueUnavailable,
                                        );

                                        return BoundWalkControl::Stop;
                                    };

                                    if matches!(data.as_ref(), TypeData::Nullable(contained) if *contained == subject.ty)
                                    {
                                        subject.ty = declared;
                                    }
                                }

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
                trusted_variant: false,
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
            trusted_variant: matches!(
                self.request.view().expression(expression),
                Some(BoundExpression::Structured(expression))
                    if expression.kind() == BoundStructuredExpressionKind::TrustBoundary
            ),
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

        let (matched_subject, type_data) = self.matched_subject(subject)?;

        let (target, is_ambiguous) = self.pattern_target(pattern, type_data.as_ref())?;

        let kind = effective_pattern_kind(pattern, target);
        let only_union_variant = self.only_union_variant(type_data.as_ref(), target)?;

        let mut compatible = self.pattern_is_compatible(
            id,
            pattern,
            kind,
            target,
            matched_subject.ty,
            type_data.as_ref(),
        )?;

        let tagless_variant_requires_fact = kind == BoundPatternKind::Variant
            && self.tagless_union(type_data.as_ref())?
            && !matched_subject.trusted_variant
            && !only_union_variant;

        if tagless_variant_requires_fact {
            compatible = false;
        }

        let child_parent = if matches!(
            kind,
            BoundPatternKind::Grouped | BoundPatternKind::Alternative
        ) {
            subject
        } else {
            matched_subject
        };

        let children = self.child_subjects(pattern, child_parent, target, type_data.as_ref())?;

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
            || is_ambiguous
            || self.constant_pattern_is_recovered(id, target)?
            || matches!(type_data.as_ref(), TypeData::Error)
            || !compatible;

        let operation = pattern_operation(pattern.mode(), is_recovered);

        if !is_ambiguous {
            self.record_bindings(pattern, subject, kind, operation, projection);
        }

        self.record_entry_bindings(pattern, operation, &children.entries);

        let shape_is_total = self.pattern_shape_is_total(
            pattern,
            kind,
            type_data.as_ref(),
            compatible,
            matched_subject.trusted_variant,
            only_union_variant,
        )?;

        let refutability = pattern_refutability(
            kind,
            &child_refutability,
            compatible,
            shape_is_total,
            is_recovered,
        );

        if is_ambiguous {
            self.report_ambiguous_name(id, pattern)?;
        } else if tagless_variant_requires_fact {
            self.report_tagless_union_pattern(id)?;
        } else if !compatible && !matches!(type_data.as_ref(), TypeData::Error) {
            self.report_incompatible(id, subject.ty)?;
        } else if !pattern_mode_accepts_refutable(pattern.mode())
            && refutability == PatternRefutability::Refutable
        {
            self.report_refutable(id, subject.ty)?;
        }

        let predicate = self.pattern_predicate(
            id,
            pattern,
            kind,
            target,
            matched_subject.ty,
            type_data.as_ref(),
        )?;

        let entry = PatternCheckEntry::new(id, subject.ty, operation, refutability, target)
            .with_test((!is_recovered).then_some(predicate).flatten())
            .with_projection(projection)
            .with_refinement((!is_recovered).then_some(predicate).flatten());

        self.patterns.insert(id, entry);

        Ok(entry)
    }

    fn matched_subject(
        &self,
        subject: PatternSubject,
    ) -> Result<(PatternSubject, std::sync::Arc<TypeData>), CheckerInfrastructureError> {
        let mut matched = subject;

        loop {
            let data = self
                .request
                .semantic_values()
                .type_data(matched.ty)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            let TypeData::Borrow { target, .. } = data.as_ref() else {
                return Ok((matched, data));
            };

            matched.ty = *target;
        }
    }

    fn pattern_target(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Result<(Option<BoundPatternTarget>, bool), CheckerInfrastructureError> {
        let expected = self
            .expected_subject_variant(pattern, subject)?
            .map(|variant| BoundPatternTarget::Surface(variant.into()));

        if pattern.is_contextual_name() {
            return Ok(match (pattern.target(), expected) {
                (Some(first), Some(second)) if first != second => (None, true),
                (Some(target), Some(_)) | (Some(target), None) | (None, Some(target)) => {
                    (Some(target), false)
                }
                (None, None) => (None, false),
            });
        }

        if let Some(target) = pattern.target()
            && !self.variant_target_conflicts_with_subject(target, subject)?
        {
            return Ok((Some(target), false));
        }

        if pattern.kind() != BoundPatternKind::Variant {
            return Ok((None, false));
        }

        Ok((expected, false))
    }

    fn expected_subject_variant(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Result<Option<UnionVariantSymbolId>, CheckerInfrastructureError> {
        let (
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            },
            Some(name),
        ) = (subject, pattern.name())
        else {
            return Ok(None);
        };

        let lookup =
            available_dependency(self.request.lookup_member((*union).into(), name.as_str()))?
                .unwrap_or(MemberLookupResult::NotFound);

        let MemberLookupResult::Found(AnySymbolId::UnionVariant(variant)) = lookup else {
            return Ok(None);
        };

        Ok(Some(variant))
    }

    fn variant_target_conflicts_with_subject(
        &self,
        target: BoundPatternTarget,
        subject: &TypeData,
    ) -> Result<bool, CheckerInfrastructureError> {
        let (
            BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant)),
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            },
        ) = (target, subject)
        else {
            return Ok(false);
        };

        Ok(available_dependency(self.request.union_variant(variant))?
            .flatten()
            .is_none_or(|record| record.union() != *union))
    }
}
