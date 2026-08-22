use bray_bound_tree::{
    BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode, BoundPatternTarget,
    PatternBindingTypeEntry, PatternLiteralPredicate, PatternOperation, PatternPredicate,
    PatternProjection, PatternRefutability,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_symbols::{
    AnySymbolId, ConstantValueData, NamedTypeSymbolId, StructFieldTypeQuery, SymbolOrdinal,
    TypeData, TypeId, UnionPayloadFieldTypeQuery,
};

use super::state::{PatternChecker, PatternSubject, available_dependency};
use crate::constant::check_constant_literal;
use crate::diagnostic::{diagnostic_id, pattern_span};
use crate::representation::type_representation;
use crate::type_check::diagnostic_type;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerSemanticQueryProvider};

impl<C> PatternChecker<'_, '_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
        + ?Sized,
{
    pub(super) fn pattern_shape_is_total(
        &self,
        pattern: &BoundPattern,
        kind: BoundPatternKind,
        target: Option<BoundPatternTarget>,
        subject: &TypeData,
        compatible: bool,
        trusted_variant: bool,
    ) -> Result<bool, CheckerInfrastructureError> {
        if !compatible {
            return Ok(false);
        }

        let is_total = match (kind, subject) {
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
                    return Ok(false);
                };

                trusted_variant && self.tagless_union(subject)?
                    || available_dependency(self.request.union(*union))?
                        .flatten()
                        .is_some_and(|record| record.variants() == [variant])
            }
            _ => false,
        };

        Ok(is_total)
    }

    pub(super) fn tagless_union(
        &self,
        subject: &TypeData,
    ) -> Result<bool, CheckerInfrastructureError> {
        let TypeData::Named {
            definition: NamedTypeSymbolId::Union(union),
            ..
        } = subject
        else {
            return Ok(false);
        };

        Ok(available_dependency(self.request.declared_type_representation((*union).into()))?
            .is_some_and(|representation| representation.value().is_tagless_union()))
    }

    pub(super) fn pattern_predicate(
        &self,
        id: BoundPatternId,
        pattern: &BoundPattern,
        kind: BoundPatternKind,
        target: Option<BoundPatternTarget>,
        input_type: TypeId,
        subject: &TypeData,
    ) -> Result<Option<PatternPredicate>, CheckerInfrastructureError> {
        let predicate = match (kind, subject) {
            (BoundPatternKind::Literal, _) => self
                .literal_predicate(pattern, input_type)?
                .map(PatternPredicate::Literal),
            (BoundPatternKind::Path, _) => self
                .constant_patterns
                .get(&id)
                .map(|evidence| evidence.term())
                .map(PatternPredicate::Constant),
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
                    (!self.tagless_union(subject)?)
                        .then_some(PatternPredicate::ActiveUnionVariant(variant))
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

    pub(super) fn literal_predicate(
        &self,
        pattern: &BoundPattern,
        input_type: TypeId,
    ) -> Result<Option<PatternLiteralPredicate>, CheckerInfrastructureError> {
        let Some(literal) = pattern.literal() else {
            return Ok(None);
        };

        let source = self.request.source(pattern.origin().source_anchor())?;

        let Some(spelling) = source.text_for_range(literal.range()) else {
            return Err(CheckerInfrastructureError::InvalidSourceRange {
                span: bray_source::SourceSpan::new(source.span().source_id(), literal.range()),
            });
        };

        let Some(representation) = type_representation(self.request, input_type)? else {
            return Ok(None);
        };

        let Ok(value) = check_constant_literal(literal.kind(), spelling, representation, || {
            self.request
                .selected_target()
                .machine()
                .pointer_width_bits()
        }) else {
            return Ok(None);
        };

        let value = self
            .request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(input_type, value))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        Ok(Some(PatternLiteralPredicate::new(literal, value)))
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
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::PatternFailure,
                span,
            ))
            .with_arg(DiagnosticArg::actual_type(actual)),
        );

        Ok(())
    }

    pub(super) fn report_tagless_union_pattern(
        &mut self,
        pattern: BoundPatternId,
    ) -> Result<(), CheckerInfrastructureError> {
        let span = pattern_span(self.request, pattern)?;

        self.diagnostics.push(
            Diagnostic::new(
                diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::CheckingTaglessUnionPatternRequiresVariant,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::PatternFailure,
                span,
            )),
        );

        Ok(())
    }

    pub(super) fn report_refutable(
        &mut self,
        pattern: BoundPatternId,
        actual: TypeId,
    ) -> Result<(), CheckerInfrastructureError> {
        let actual = diagnostic_type(self.request, actual)?;
        let span = pattern_span(self.request, pattern)?;

        self.diagnostics.push(
            Diagnostic::new(
                diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::CheckingRefutablePattern,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::PatternFailure,
                span,
            ))
            .with_arg(DiagnosticArg::actual_type(actual))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::RefutablePatternRequiresConditionalContext,
            )),
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
