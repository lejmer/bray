use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundNodeOrigin, BoundPattern,
    BoundPatternId, BoundPatternKind, BoundPatternTarget, BoundWalkControl, BoundWalkEvent,
    MatchCoverageEntry, PatternRefutability, walk_bound_unit_view,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{Diagnostic, DiagnosticKind, SeverityKind};
use bray_source::TextRange;
use bray_symbols::{
    AnySymbolId, ConstantValueData, ConstantValueId, ConstantValueKind, NamedTypeSymbolId,
    StructFieldTypeFact, TypeData, UnionPayloadFieldTypeFact, UnionVariantSymbolId,
};

use super::check::{PatternChecker, effective_pattern_kind};
use crate::constant::{check_constant_literal, constant_values_equal};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;
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

        let mut covered = Coverage::default();
        let mut unreachable = Vec::new();
        let mut recovered = subject.is_recovered || expression.is_recovered();

        for (index, arm) in expression.arms().iter().copied().enumerate() {
            let arm_coverage = self.coverage(arm.pattern())?;
            let guard = self.guard_truth(arm.guard())?;
            let is_unreachable =
                guard == GuardTruth::False || covered.contains(self.request, &arm_coverage)?;

            if is_unreachable {
                let index = u32::try_from(index).unwrap_or(u32::MAX);

                unreachable.push(index);
                self.report(
                    arm.pattern(),
                    DiagnosticKind::CheckingUnreachableMatchArm,
                    SeverityKind::Warning,
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

            self.diagnostics.push(
                Diagnostic::new(
                    diagnostic_id(self.diagnostics.len()),
                    DiagnosticKind::CheckingNonExhaustiveMatch,
                    SeverityKind::Error,
                )
                .with_primary_span(span),
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
            return Ok(Coverage::total());
        }

        let kind = effective_pattern_kind(pattern, checked.target());

        let coverage = match kind {
            BoundPatternKind::Literal => self.literal_coverage(pattern, checked.input_type())?,
            BoundPatternKind::Path => self.constant_coverage(id)?,
            BoundPatternKind::NullableAbsent => Coverage::nullable_absent(),
            BoundPatternKind::NullablePresent => {
                let mut contained = Coverage::default();

                for child in pattern.children() {
                    contained.merge(&self.coverage(*child)?);
                }

                Coverage::nullable_present(contained)
            }
            BoundPatternKind::Variant if self.children_are_irrefutable(pattern) => {
                match checked.target() {
                    Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))) => {
                        Coverage::variant(variant)
                    }
                    _ => Coverage::default(),
                }
            }
            BoundPatternKind::Alternative => {
                let mut coverage = Coverage::default();

                for child in pattern.children() {
                    let alternative = self.coverage(*child)?;

                    if coverage.contains(self.request, &alternative)? {
                        self.report(
                            *child,
                            DiagnosticKind::CheckingUnreachablePatternAlternative,
                            SeverityKind::Warning,
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

    fn children_are_irrefutable(&self, pattern: &BoundPattern) -> bool {
        pattern.children().iter().all(|child| {
            self.patterns
                .get(child)
                .is_some_and(|entry| entry.refutability() == PatternRefutability::Irrefutable)
        })
    }

    fn literal_coverage(
        &self,
        pattern: &BoundPattern,
        input_type: bray_symbols::TypeId,
    ) -> Result<Coverage, CheckerInfrastructureError> {
        let Some(literal) = pattern.literal() else {
            return Ok(Coverage::unknown());
        };

        let source = self.request.source(pattern.origin().source_anchor())?;

        let Some(spelling) = source.text_for_range(literal.range()) else {
            return Err(CheckerInfrastructureError::InvalidSourceRange {
                span: bray_source::SourceSpan::new(source.span().source_id(), literal.range()),
            });
        };

        let Some(representation) = type_representation(self.request, input_type)? else {
            return Ok(Coverage::unknown());
        };

        let Ok(value) = check_constant_literal(literal.kind(), spelling, representation, || {
            self.request
                .selected_target()
                .machine()
                .pointer_width_bits()
        }) else {
            return Ok(Coverage::unknown());
        };

        let value = self
            .request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(input_type, value))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        Ok(Coverage::constant(value))
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

        Ok(Coverage::constant(*value))
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
    is_total: bool,
    is_unknown: bool,
    constants: Vec<ConstantValueId>,
    nullable_absent: bool,
    nullable_present: Option<Box<Coverage>>,
    variants: BTreeSet<UnionVariantSymbolId>,
}

impl Coverage {
    fn total() -> Self {
        Self {
            is_total: true,
            ..Self::default()
        }
    }

    fn unknown() -> Self {
        Self {
            is_unknown: true,
            ..Self::default()
        }
    }

    fn constant(value: ConstantValueId) -> Self {
        Self {
            constants: vec![value],
            ..Self::default()
        }
    }

    fn nullable_absent() -> Self {
        Self {
            nullable_absent: true,
            ..Self::default()
        }
    }

    fn nullable_present(contained: Self) -> Self {
        Self {
            nullable_present: Some(Box::new(contained)),
            ..Self::default()
        }
    }

    fn variant(variant: UnionVariantSymbolId) -> Self {
        Self {
            variants: BTreeSet::from([variant]),
            ..Self::default()
        }
    }

    fn merge(&mut self, other: &Self) {
        self.is_total |= other.is_total;
        self.is_unknown |= other.is_unknown;
        self.constants.extend_from_slice(&other.constants);
        self.nullable_absent |= other.nullable_absent;

        match (&mut self.nullable_present, &other.nullable_present) {
            (Some(current), Some(other)) => current.merge(other),
            (None, Some(other)) => {
                let mut contained = Coverage::default();

                contained.merge(other);
                self.nullable_present = Some(Box::new(contained));
            }
            (Some(_), None) | (None, None) => {}
        }

        self.variants.extend(other.variants.iter().copied());
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

        let contains = other.is_total && self.is_total
            || !other.is_total
                && (self.is_total
                    || constants_contained
                        && (!other.nullable_absent || self.nullable_absent)
                        && nullable_contains(
                            request,
                            self.nullable_present.as_deref(),
                            other.nullable_present.as_deref(),
                        )?
                        && other.variants.is_subset(&self.variants));

        Ok(contains)
    }

    fn is_empty(&self) -> bool {
        !self.is_total
            && !self.is_unknown
            && self.constants.is_empty()
            && !self.nullable_absent
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
        if self.is_total {
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

                self.nullable_absent && coverage.is_exhaustive(request, target.as_ref())?
            }
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            } => request.symbols().union(*union).is_some_and(|record| {
                record
                    .variants()
                    .iter()
                    .all(|variant| self.variants.contains(variant))
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

                for value in &self.constants {
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
    current: &[ConstantValueId],
    other: &[ConstantValueId],
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for other in other {
        let mut contained = false;

        for current in current {
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
