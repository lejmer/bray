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
    AnySymbolId, NamedTypeSymbolId, StructFieldTypeFact, TypeData, UnionPayloadFieldTypeFact,
    UnionVariantSymbolId,
};

use super::check::{PatternChecker, effective_pattern_kind};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{
    CheckerInfrastructureError, CheckerRequestContext, CheckerSemanticFactProvider, CheckerUnitView,
};

impl<C> PatternChecker<'_, C>
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
            let is_unreachable = guard == GuardTruth::False || covered.contains(&arm_coverage);

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
        }

        let exhaustive = covered.is_exhaustive(self.request, subject_data.as_ref());

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

    fn coverage(&self, id: BoundPatternId) -> Result<Coverage, CheckerInfrastructureError> {
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
            BoundPatternKind::Literal => self.literal_coverage(pattern)?,
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
                    coverage.merge(&self.coverage(*child)?);
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
    ) -> Result<Coverage, CheckerInfrastructureError> {
        let Some(literal) = pattern.literal() else {
            return Ok(Coverage::default());
        };

        if literal.kind() != bray_bound_tree::BoundLiteralKind::Boolean {
            return Ok(Coverage::default());
        }

        Ok(
            match self.boolean_literal_value(pattern.origin(), literal.range())? {
                Some(value) => Coverage::boolean(value),
                None => Coverage::default(),
            },
        )
    }

    fn guard_truth(
        &self,
        guard: Option<BoundExpressionId>,
    ) -> Result<GuardTruth, CheckerInfrastructureError> {
        let Some(guard) = guard else {
            return Ok(GuardTruth::True);
        };

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
    booleans: u8,
    nullable_absent: bool,
    nullable_present: Option<Box<Coverage>>,
    variants: BTreeSet<UnionVariantSymbolId>,
}

impl Coverage {
    const FALSE: u8 = 1;
    const TRUE: u8 = 2;

    fn total() -> Self {
        Self {
            is_total: true,
            ..Self::default()
        }
    }

    fn boolean(value: bool) -> Self {
        Self {
            booleans: if value { Self::TRUE } else { Self::FALSE },
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
        self.booleans |= other.booleans;
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

    fn contains(&self, other: &Self) -> bool {
        if other.is_empty() {
            return false;
        }

        other.is_total && self.is_total
            || !other.is_total
                && (self.is_total
                    || self.booleans & other.booleans == other.booleans
                        && (!other.nullable_absent || self.nullable_absent)
                        && nullable_contains(
                            self.nullable_present.as_deref(),
                            other.nullable_present.as_deref(),
                        )
                        && other.variants.is_subset(&self.variants))
    }

    fn is_empty(&self) -> bool {
        !self.is_total
            && self.booleans == 0
            && !self.nullable_absent
            && self.nullable_present.is_none()
            && self.variants.is_empty()
    }

    fn is_exhaustive<C>(&self, request: CheckerUnitView<'_, C>, subject: &TypeData) -> bool
    where
        C: CheckerRequestContext + ?Sized,
    {
        if self.is_total {
            return true;
        }

        match subject {
            TypeData::Nullable(target) => {
                self.nullable_absent
                    && self.nullable_present.as_deref().is_some_and(|coverage| {
                        let Ok(target) = request.semantic_values().type_data(*target) else {
                            return false;
                        };

                        coverage.is_exhaustive(request, target.as_ref())
                    })
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
            TypeData::Named { definition, .. } => request
                .available_compiler_known_symbols()
                .representation_symbol::<bray_symbols::StructSymbolId>(
                    RepresentationRole::ScalarBool,
                )
                .is_some_and(|boolean| {
                    *definition == NamedTypeSymbolId::Struct(boolean)
                        && self.booleans == Self::FALSE | Self::TRUE
                }),
            _ => false,
        }
    }
}

fn nullable_contains(current: Option<&Coverage>, other: Option<&Coverage>) -> bool {
    match (current, other) {
        (_, None) => true,
        (Some(current), Some(other)) => current.contains(other),
        (None, Some(_)) => false,
    }
}
