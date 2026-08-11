use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundNodeOrigin, BoundPattern,
    BoundPatternId, BoundPatternKind, BoundPatternTarget, BoundWalkControl, BoundWalkEvent,
    MatchCoverageEntry, PatternPredicate, PatternRefutability, walk_bound_unit_view,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticPatternCoverage,
    DiagnosticPatternMissingCase, DiagnosticPatternUnreachability, DiagnosticRelatedLocation,
    DiagnosticRelatedLocationKind, SeverityKind,
};
use bray_source::TextRange;
use bray_symbols::{
    AnySymbolId, ConstantValueId, ConstantValueKind, NamedTypeSymbolId, StructFieldTypeFact,
    TypeData, UnionPayloadFieldTypeFact, UnionVariantSymbolId,
};

use super::check::{PatternChecker, available_dependency, effective_pattern_kind};
use crate::constant::constant_values_equal;
use crate::diagnostic::{diagnostic_id, diagnostic_type, expression_span, pattern_span};
use crate::{
    CheckerInfrastructureError, CheckerRequestContext, CheckerSemanticFactProvider, CheckerUnitView,
};

impl<C> PatternChecker<'_, '_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
{
    pub(super) fn check_matches(&mut self) -> Result<(), CheckerInfrastructureError> {
        let mut matches = Vec::new();

        walk_bound_unit_view(self.request.view(), self.request.unit().root(), |event| {
            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(BoundExpression::Match(expression)) = self.request.view().expression(id)
            else {
                return BoundWalkControl::Continue;
            };

            matches.push((id, expression));

            BoundWalkControl::Continue
        });

        for (id, expression) in matches {
            if self.request.is_cancelled() {
                break;
            }

            self.check_match(id, expression)?;
        }

        Ok(())
    }

    fn check_match(
        &mut self,
        expression_id: BoundExpressionId,
        expression: &bray_bound_tree::BoundMatchExpression,
    ) -> Result<(), CheckerInfrastructureError> {
        let subject = self.expression_type(expression.subject())?;

        let subject_data = self
            .request
            .semantic_values()
            .type_data(subject.ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let subject_data = match subject_data.as_ref() {
            TypeData::Borrow { target, .. } => self
                .request
                .semantic_values()
                .type_data(*target)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?,
            _ => subject_data,
        };

        let mut covered = Coverage::default();
        let mut unreachable = Vec::new();
        let mut recovered = subject.is_recovered || expression.is_recovered();

        for (index, arm) in expression.arms().iter().copied().enumerate() {
            let arm_coverage = self.coverage(arm.pattern())?;
            let guard = self.guard_truth(arm.guard())?;

            let covering = covered.covering_patterns(self.request, &arm_coverage)?;
            let is_unreachable = guard == GuardTruth::False || covering.is_some();

            if is_unreachable {
                let index = u32::try_from(index).unwrap_or(u32::MAX);

                unreachable.push(index);

                self.report_unreachable(
                    arm.pattern(),
                    DiagnosticKind::CheckingUnreachableMatchArm,
                    if guard == GuardTruth::False {
                        DiagnosticPatternUnreachability::GuardAlwaysFalse
                    } else {
                        DiagnosticPatternUnreachability::CoveredByEarlierPattern
                    },
                    covering.as_ref(),
                )?;
            }

            if guard == GuardTruth::True && !is_unreachable {
                covered.merge(&arm_coverage);
            }

            recovered |= self
                .patterns
                .get(&arm.pattern())
                .is_none_or(|pattern| pattern.is_recovered());

            recovered |= arm_coverage.is_unknown && guard != GuardTruth::False;
        }

        let exhaustive = covered.is_exhaustive(self.request, subject_data.as_ref())?;

        if !exhaustive && !recovered && !matches!(subject_data.as_ref(), TypeData::Error) {
            let span = expression_span(self.request, expression_id)?;

            let (missing, omitted_count) =
                covered.missing_cases(self.request, subject_data.as_ref())?;

            let coverage = DiagnosticPatternCoverage::new(
                diagnostic_type(self.request.context(), subject.ty)?,
                missing,
                omitted_count,
            );

            self.diagnostics.push(
                Diagnostic::new(
                    diagnostic_id(self.diagnostics.len()),
                    DiagnosticKind::CheckingNonExhaustiveMatch,
                    SeverityKind::Error,
                )
                .with_primary_span(span)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::MatchCoverage,
                    span,
                ))
                .with_arg(DiagnosticArg::pattern_coverage(coverage)),
            );
        }

        self.matches.push(MatchCoverageEntry::new(
            expression_id,
            unreachable,
            exhaustive,
            recovered,
        ));

        Ok(())
    }

    fn coverage(&mut self, id: BoundPatternId) -> Result<Coverage, CheckerInfrastructureError> {
        let Some(pattern) = self.request.view().pattern(id) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode { node: id.into() });
        };

        let Some(checked) = self.patterns.get(&id) else {
            return Ok(Coverage::default());
        };

        if checked.is_recovered() {
            return Ok(Coverage::default());
        }

        if checked.refutability() == PatternRefutability::Irrefutable {
            return Ok(Coverage::total(id));
        }

        let kind = effective_pattern_kind(pattern, checked.target());

        let coverage = match kind {
            BoundPatternKind::Literal => match checked.test() {
                Some(PatternPredicate::Literal(literal)) => Coverage::constant(literal.value(), id),
                _ => Coverage::unknown(),
            },
            BoundPatternKind::Path => self.constant_coverage(id)?,
            BoundPatternKind::NullableAbsent => Coverage::nullable_absent(id),
            BoundPatternKind::NullablePresent => {
                let mut contained = Coverage::default();

                for child in pattern.children() {
                    contained.merge(&self.coverage(*child)?);
                }

                Coverage::nullable_present(id, contained)
            }
            BoundPatternKind::Variant if self.children_are_irrefutable(pattern) => {
                match checked.target() {
                    Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))) => {
                        Coverage::variant(variant, id)
                    }
                    _ => Coverage::default(),
                }
            }
            BoundPatternKind::Alternative => {
                let mut coverage = Coverage::default();

                for child in pattern.children() {
                    let alternative = self.coverage(*child)?;

                    if let Some(covering) =
                        coverage.covering_patterns(self.request, &alternative)?
                    {
                        self.report_unreachable(
                            *child,
                            DiagnosticKind::CheckingUnreachablePatternAlternative,
                            DiagnosticPatternUnreachability::CoveredByEarlierPattern,
                            Some(&covering),
                        )?;
                    } else {
                        coverage.merge(&alternative);
                    }
                }

                coverage
            }
            _ => Coverage::default(),
        };

        Ok(coverage)
    }

    fn report_unreachable(
        &mut self,
        pattern: BoundPatternId,
        kind: DiagnosticKind,
        reason: DiagnosticPatternUnreachability,
        covering: Option<&BTreeSet<BoundPatternId>>,
    ) -> Result<(), CheckerInfrastructureError> {
        let span = pattern_span(self.request, pattern)?;

        let mut diagnostic = Diagnostic::new(
            diagnostic_id(self.diagnostics.len()),
            kind,
            SeverityKind::Warning,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::PatternFailure,
            span,
        ))
        .with_arg(DiagnosticArg::pattern_unreachability(reason));

        for covering in covering.into_iter().flatten().copied() {
            let related = pattern_span(self.request, covering)?;

            if related != span {
                diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                    DiagnosticRelatedLocationKind::CoveredByPattern,
                    related,
                ));
            }
        }

        self.diagnostics.push(diagnostic);

        Ok(())
    }

    fn children_are_irrefutable(&self, pattern: &BoundPattern) -> bool {
        pattern.children().iter().all(|child| {
            self.patterns
                .get(child)
                .is_some_and(|entry| entry.refutability() == PatternRefutability::Irrefutable)
        })
    }

    fn constant_coverage(
        &self,
        pattern: BoundPatternId,
    ) -> Result<Coverage, CheckerInfrastructureError> {
        let Some(evidence) = self.constant_patterns.get(&pattern) else {
            return Ok(Coverage::unknown());
        };

        let term = self
            .request
            .semantic_values()
            .constant_term_data(evidence.term())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let bray_symbols::ConstantTermData::Value(value) = term.as_ref() else {
            return Ok(Coverage::unknown());
        };

        let data = self
            .request
            .semantic_values()
            .constant_value_data(*value)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        if matches!(data.kind(), ConstantValueKind::Error) {
            return Ok(Coverage::unknown());
        }

        Ok(Coverage::constant(*value, pattern))
    }

    fn guard_truth(
        &self,
        guard: Option<BoundExpressionId>,
    ) -> Result<GuardTruth, CheckerInfrastructureError> {
        let Some(guard) = guard else {
            return Ok(GuardTruth::True);
        };

        if let Some(value) = self.constant_guards.get(&guard) {
            let value = self
                .request
                .semantic_values()
                .constant_value_data(*value)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            return Ok(match value.kind() {
                ConstantValueKind::Boolean(true) => GuardTruth::True,
                ConstantValueKind::Boolean(false) => GuardTruth::False,
                _ => GuardTruth::Unknown,
            });
        }

        let Some(BoundExpression::Literal(literal)) = self.request.view().expression(guard) else {
            return Ok(GuardTruth::Unknown);
        };

        if literal.kind() != bray_bound_tree::BoundLiteralKind::Boolean {
            return Ok(GuardTruth::Unknown);
        }

        Ok(
            match self.boolean_literal_value(literal.origin(), literal.spelling_range())? {
                Some(true) => GuardTruth::True,
                Some(false) => GuardTruth::False,
                None => GuardTruth::Unknown,
            },
        )
    }

    fn boolean_literal_value(
        &self,
        origin: BoundNodeOrigin,
        range: TextRange,
    ) -> Result<Option<bool>, CheckerInfrastructureError> {
        let source = self.request.source(origin.source_anchor())?;

        Ok(match source.text_for_range(range) {
            Some("true") => Some(true),
            Some("false") => Some(false),
            Some(_) | None => None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GuardTruth {
    False,
    True,
    Unknown,
}

#[derive(Clone, Debug, Default)]
struct Coverage {
    total_origin: Option<BoundPatternId>,
    is_unknown: bool,
    constants: Vec<(ConstantValueId, BoundPatternId)>,
    nullable_absent: Option<BoundPatternId>,
    nullable_present_origin: Option<BoundPatternId>,
    nullable_present: Option<Box<Coverage>>,
    variants: BTreeMap<UnionVariantSymbolId, BoundPatternId>,
}

impl Coverage {
    fn total(origin: BoundPatternId) -> Self {
        Self {
            total_origin: Some(origin),
            ..Self::default()
        }
    }

    fn unknown() -> Self {
        Self {
            is_unknown: true,
            ..Self::default()
        }
    }

    fn constant(value: ConstantValueId, origin: BoundPatternId) -> Self {
        Self {
            constants: vec![(value, origin)],
            ..Self::default()
        }
    }

    fn nullable_absent(origin: BoundPatternId) -> Self {
        Self {
            nullable_absent: Some(origin),
            ..Self::default()
        }
    }

    fn nullable_present(origin: BoundPatternId, contained: Self) -> Self {
        Self {
            nullable_present_origin: Some(origin),
            nullable_present: Some(Box::new(contained)),
            ..Self::default()
        }
    }

    fn variant(variant: UnionVariantSymbolId, origin: BoundPatternId) -> Self {
        Self {
            variants: BTreeMap::from([(variant, origin)]),
            ..Self::default()
        }
    }

    fn merge(&mut self, other: &Self) {
        self.total_origin = self.total_origin.or(other.total_origin);
        self.is_unknown |= other.is_unknown;
        self.constants.extend_from_slice(&other.constants);
        self.nullable_absent = self.nullable_absent.or(other.nullable_absent);

        self.nullable_present_origin = self
            .nullable_present_origin
            .or(other.nullable_present_origin);

        match (&mut self.nullable_present, &other.nullable_present) {
            (Some(current), Some(other)) => current.merge(other),
            (None, Some(other)) => {
                let mut contained = Coverage::default();

                contained.merge(other);
                self.nullable_present = Some(Box::new(contained));
            }
            (Some(_), None) | (None, None) => {}
        }

        for (&variant, &origin) in &other.variants {
            self.variants.entry(variant).or_insert(origin);
        }
    }

    fn contains<C>(
        &self,
        request: CheckerUnitView<'_, C>,
        other: &Self,
    ) -> Result<bool, CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        if other.is_empty() || other.is_unknown {
            return Ok(false);
        }

        let constants_contained = constants_contain(
            request,
            self.constants.as_slice(),
            other.constants.as_slice(),
        )?;

        let contains = other.total_origin.is_some() && self.total_origin.is_some()
            || other.total_origin.is_none()
                && (self.total_origin.is_some()
                    || constants_contained
                        && (other.nullable_absent.is_none() || self.nullable_absent.is_some())
                        && nullable_contains(
                            request,
                            self.nullable_present.as_deref(),
                            other.nullable_present.as_deref(),
                        )?
                        && other
                            .variants
                            .keys()
                            .all(|variant| self.variants.contains_key(variant)));

        Ok(contains)
    }

    fn covering_patterns<C>(
        &self,
        request: CheckerUnitView<'_, C>,
        other: &Self,
    ) -> Result<Option<BTreeSet<BoundPatternId>>, CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        if !self.contains(request, other)? {
            return Ok(None);
        }

        if let Some(origin) = self.total_origin {
            return Ok(Some(BTreeSet::from([origin])));
        }

        let mut origins = BTreeSet::new();

        for (other, _) in &other.constants {
            for (current, origin) in &self.constants {
                if constant_values_equal(request.semantic_values(), *current, *other)? {
                    origins.insert(*origin);

                    break;
                }
            }
        }

        if other.nullable_absent.is_some()
            && let Some(origin) = self.nullable_absent
        {
            origins.insert(origin);
        }

        if other.nullable_present.is_some()
            && let Some(origin) = self.nullable_present_origin
        {
            origins.insert(origin);
        }

        for variant in other.variants.keys() {
            if let Some(origin) = self.variants.get(variant) {
                origins.insert(*origin);
            }
        }

        Ok(Some(origins))
    }

    fn missing_cases<C>(
        &self,
        request: CheckerUnitView<'_, C>,
        subject: &TypeData,
    ) -> Result<(Vec<DiagnosticPatternMissingCase>, u64), CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        const MAX_REPORTED_CASES: usize = 8;

        let mut missing = match subject {
            TypeData::Nullable(target) => {
                let mut missing = Vec::new();

                if self.nullable_absent.is_none() {
                    missing.push(DiagnosticPatternMissingCase::NullableAbsent);
                }

                let present_is_exhaustive = match self.nullable_present.as_deref() {
                    Some(coverage) => {
                        let target = request
                            .semantic_values()
                            .type_data(*target)
                            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                        coverage.is_exhaustive(request, target.as_ref())?
                    }
                    None => false,
                };

                if !present_is_exhaustive {
                    missing.push(DiagnosticPatternMissingCase::NullablePresent);
                }

                missing
            }
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            } => {
                let Some(record) = available_dependency(request.union(*union))?.flatten() else {
                    return Ok((vec![DiagnosticPatternMissingCase::RemainingValues], 0));
                };

                record
                    .variants()
                    .iter()
                    .filter(|variant| !self.variants.contains_key(variant))
                    .map(|variant| {
                        request
                            .symbols()
                            .member_name((*variant).into())
                            .map(|name| {
                                DiagnosticPatternMissingCase::UnionVariant(
                                    name.as_str().to_owned(),
                                )
                            })
                            .ok_or(CheckerInfrastructureError::SemanticValueUnavailable)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            TypeData::Named { definition, .. }
                if request
                    .available_compiler_known_symbols()
                    .representation_symbol::<bray_symbols::StructSymbolId>(
                        RepresentationRole::ScalarBool,
                    )
                    .is_some_and(|boolean| *definition == NamedTypeSymbolId::Struct(boolean)) =>
            {
                let mut covered = BTreeSet::new();

                for (value, _) in &self.constants {
                    let value = request
                        .semantic_values()
                        .constant_value_data(*value)
                        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                    if let ConstantValueKind::Boolean(value) = value.kind() {
                        covered.insert(*value);
                    }
                }

                [false, true]
                    .into_iter()
                    .filter(|value| !covered.contains(value))
                    .map(DiagnosticPatternMissingCase::Boolean)
                    .collect()
            }
            TypeData::Error => Vec::new(),
            _ => vec![DiagnosticPatternMissingCase::RemainingValues],
        };

        let omitted = missing.len().saturating_sub(MAX_REPORTED_CASES);
        missing.truncate(MAX_REPORTED_CASES);
        let omitted = u64::try_from(omitted).unwrap_or(u64::MAX);

        Ok((missing, omitted))
    }

    fn is_empty(&self) -> bool {
        self.total_origin.is_none()
            && !self.is_unknown
            && self.constants.is_empty()
            && self.nullable_absent.is_none()
            && self.nullable_present.is_none()
            && self.variants.is_empty()
    }

    fn is_exhaustive<C>(
        &self,
        request: CheckerUnitView<'_, C>,
        subject: &TypeData,
    ) -> Result<bool, CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        if self.total_origin.is_some() {
            return Ok(true);
        }

        let is_exhaustive = match subject {
            TypeData::Nullable(target) => {
                let Some(coverage) = self.nullable_present.as_deref() else {
                    return Ok(false);
                };

                let target = request
                    .semantic_values()
                    .type_data(*target)
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                self.nullable_absent.is_some()
                    && coverage.is_exhaustive(request, target.as_ref())?
            }
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            } => available_dependency(request.union(*union))?
                .flatten()
                .is_some_and(|record| {
                    record
                        .variants()
                        .iter()
                        .all(|variant| self.variants.contains_key(variant))
                }),
            TypeData::Named { definition, .. } => {
                let is_boolean = request
                    .available_compiler_known_symbols()
                    .representation_symbol::<bray_symbols::StructSymbolId>(
                        RepresentationRole::ScalarBool,
                    )
                    .is_some_and(|boolean| *definition == NamedTypeSymbolId::Struct(boolean));

                if !is_boolean {
                    return Ok(false);
                }

                let mut values = BTreeSet::new();

                for (value, _) in &self.constants {
                    let value = request
                        .semantic_values()
                        .constant_value_data(*value)
                        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                    if let ConstantValueKind::Boolean(value) = value.kind() {
                        values.insert(*value);
                    }
                }

                values == BTreeSet::from([false, true])
            }
            _ => false,
        };

        Ok(is_exhaustive)
    }
}

fn constants_contain<C>(
    request: CheckerUnitView<'_, C>,
    current: &[(ConstantValueId, BoundPatternId)],
    other: &[(ConstantValueId, BoundPatternId)],
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for (other, _) in other {
        let mut contained = false;

        for (current, _) in current {
            if constant_values_equal(request.semantic_values(), *current, *other)? {
                contained = true;

                break;
            }
        }

        if !contained {
            return Ok(false);
        }
    }

    Ok(true)
}

fn nullable_contains<C>(
    request: CheckerUnitView<'_, C>,
    current: Option<&Coverage>,
    other: Option<&Coverage>,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    match (current, other) {
        (_, None) => Ok(true),
        (Some(current), Some(other)) => current.contains(request, other),
        (None, Some(_)) => Ok(false),
    }
}
