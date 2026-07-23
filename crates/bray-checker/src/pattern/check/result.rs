use bray_bound_tree::{
    BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode, BoundPatternTarget,
    PatternBindingTypeEntry, PatternOperation, PatternPredicate, PatternProjection,
    PatternRefutability,
};
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticKind, SeverityKind};
use bray_symbols::{
    AnySymbolId, NamedTypeSymbolId, StructFieldTypeFact, SymbolOrdinal, TypeData, TypeId,
    UnionPayloadFieldTypeFact,
};

use super::state::{PatternChecker, PatternSubject};
use crate::diagnostic::{diagnostic_id, pattern_span};
use crate::type_check::diagnostic_type;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerSemanticFactProvider};

impl<C> PatternChecker<'_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
{
    pub(super) fn pattern_shape_is_total(
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
            ) => {
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

    pub(super) fn pattern_predicate(
        &self,
        pattern: &BoundPattern,
        kind: BoundPatternKind,
        target: Option<BoundPatternTarget>,
        subject: &TypeData,
    ) -> Result<Option<PatternPredicate>, CheckerInfrastructureError> {
        let predicate = match (kind, subject) {
            (BoundPatternKind::Literal, _) => pattern.literal().map(PatternPredicate::Literal),
            (BoundPatternKind::NullableAbsent, TypeData::Nullable(_)) => {
                Some(PatternPredicate::NullableAbsent)
            }
            (BoundPatternKind::NullablePresent, TypeData::Nullable(_)) => {
                Some(PatternPredicate::NullablePresent)
            }
            (
                BoundPatternKind::Variant,
                TypeData::Named {
                    definition: NamedTypeSymbolId::Union(_),
                    ..
                },
            ) => match target {
                Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))) => {
                    Some(PatternPredicate::ActiveUnionVariant(variant))
                }
                _ => None,
            },
            (
                BoundPatternKind::Product,
                TypeData::Named {
                    definition: NamedTypeSymbolId::Struct(structure),
                    ..
                },
            ) => Some(PatternPredicate::ProductShape(*structure)),
            (BoundPatternKind::Tuple, TypeData::Tuple(elements)) => {
                Some(PatternPredicate::TupleShape(length_u32(elements.len())))
            }
            (BoundPatternKind::Array, TypeData::Array { length, .. }) => self
                .fixed_array_length(*length)?
                .map(|length| PatternPredicate::ArrayShape(length_u32(length))),
            (BoundPatternKind::Box, TypeData::OwnedIndirection { .. }) => {
                Some(PatternPredicate::OwnedTarget)
            }
            _ => None,
        };

        Ok(predicate)
    }

    pub(super) fn record_bindings(
        &mut self,
        pattern: &BoundPattern,
        subject: PatternSubject,
        kind: BoundPatternKind,
        operation: PatternOperation,
        projection: Option<PatternProjection>,
    ) {
        if kind != BoundPatternKind::Binding {
            return;
        }

        for binding in pattern.bindings() {
            self.binding_types.insert(
                *binding,
                PatternBindingTypeEntry::new(
                    *binding,
                    subject.ty,
                    operation,
                    subject.is_recovered || operation == PatternOperation::Recovered,
                )
                .with_projection(projection),
            );
        }
    }

    pub(super) fn record_entry_bindings(
        &mut self,
        pattern: &BoundPattern,
        operation: PatternOperation,
        subjects: &[(PatternSubject, Option<PatternProjection>)],
    ) {
        for (entry, (subject, projection)) in pattern
            .entries()
            .iter()
            .filter(|entry| !entry.is_remaining())
            .zip(subjects.iter().copied())
        {
            let Some(binding) = entry.binding() else {
                continue;
            };

            self.binding_types.insert(
                binding,
                PatternBindingTypeEntry::new(
                    binding,
                    subject.ty,
                    operation,
                    subject.is_recovered || operation == PatternOperation::Recovered,
                )
                .with_projection(projection),
            );
        }
    }

    pub(super) fn report_incompatible(
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

    pub(super) fn report_ambiguous_name(
        &mut self,
        pattern: BoundPatternId,
        source: &BoundPattern,
    ) -> Result<(), CheckerInfrastructureError> {
        let span = pattern_span(self.request, pattern)?;
        let name = source.name().map_or("", bray_symbols::SymbolName::as_str);

        self.diagnostics.push(
            Diagnostic::new(
                diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::BindingAmbiguousName,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_arg(DiagnosticArg::referenced_name(name)),
        );

        Ok(())
    }

    pub(in crate::pattern) fn report(
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

pub(super) fn pattern_refutability(
    kind: BoundPatternKind,
    children: &[PatternRefutability],
    is_compatible: bool,
    shape_is_total: bool,
    is_recovered: bool,
) -> PatternRefutability {
    match kind {
        BoundPatternKind::Error => PatternRefutability::Recovered,
        _ if is_recovered => PatternRefutability::Recovered,
        BoundPatternKind::Binding | BoundPatternKind::Discard | BoundPatternKind::Remaining => {
            PatternRefutability::Irrefutable
        }
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

pub(super) const fn pattern_operation(
    mode: BoundPatternMode,
    is_recovered: bool,
) -> PatternOperation {
    if is_recovered {
        return PatternOperation::Recovered;
    }

    match mode {
        BoundPatternMode::Declaration | BoundPatternMode::Assignment => PatternOperation::Consume,
        BoundPatternMode::MatchObserve => PatternOperation::Observe,
        BoundPatternMode::MatchConsume => PatternOperation::Consume,
    }
}

pub(super) const fn pattern_mode_accepts_refutable(mode: BoundPatternMode) -> bool {
    matches!(
        mode,
        BoundPatternMode::MatchObserve | BoundPatternMode::MatchConsume
    )
}

pub(super) fn symbol_ordinal(position: usize) -> SymbolOrdinal {
    SymbolOrdinal::new(length_u32(position))
}

fn length_u32(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

pub(in crate::pattern) fn effective_pattern_kind(
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
