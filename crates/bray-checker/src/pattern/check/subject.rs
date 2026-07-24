use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundPattern, BoundPatternEntry, BoundPatternKind, BoundPatternTarget, PatternProjection,
};
use bray_symbols::{
    AnySymbolId, GenericSubstitutionId, MemberLookupResult, NamedTypeSymbolId, StructFieldTypeFact,
    SymbolFactContract, SymbolFactRequest, TypeData, TypeExpressionTemplate, TypeId,
    UnionPayloadFieldSymbolId, UnionPayloadFieldTypeFact, UnionVariantSymbolId,
};

use super::result::{effective_pattern_kind, symbol_ordinal};
use super::state::{PatternChecker, PatternChildren, PatternSubject};
use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, resolve_type_expression_template,
};

impl<C> PatternChecker<'_, '_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
{
    pub(super) fn child_subjects(
        &mut self,
        pattern: &BoundPattern,
        parent: PatternSubject,
        target: Option<BoundPatternTarget>,
        subject: &TypeData,
    ) -> Result<PatternChildren, CheckerInfrastructureError> {
        let mut children = PatternChildren {
            patterns: BTreeMap::new(),
            entries: Vec::new(),
            is_recovered: false,
        };

        match (effective_pattern_kind(pattern, target), subject) {
            (BoundPatternKind::Grouped | BoundPatternKind::Alternative, _) => {
                for child in pattern.children() {
                    children.patterns.insert(*child, (parent, None));
                }
            }
            (BoundPatternKind::NullablePresent, TypeData::Nullable(target)) => {
                self.push_uniform_children(
                    pattern,
                    self.subject(*target, parent.is_recovered),
                    PatternProjection::NullableValue,
                    &mut children,
                );
            }
            (BoundPatternKind::Box, TypeData::OwnedIndirection { target, .. }) => {
                self.push_uniform_children(
                    pattern,
                    self.subject(*target, parent.is_recovered),
                    PatternProjection::OwnedTarget,
                    &mut children,
                );
            }
            (BoundPatternKind::Tuple, TypeData::Tuple(elements)) => {
                for (position, entry) in pattern.entries().iter().enumerate() {
                    if entry.is_remaining() {
                        continue;
                    }

                    let subject = elements
                        .get(position)
                        .copied()
                        .map(|ty| self.subject(ty, parent.is_recovered))
                        .unwrap_or_else(|| self.recovered_subject());

                    let projection = PatternProjection::TupleElement(symbol_ordinal(position));

                    self.push_entry(entry, subject, projection, &mut children);
                }
            }
            (BoundPatternKind::Array, TypeData::Array { element, .. })
            | (BoundPatternKind::Array, TypeData::Slice(element)) => {
                let remaining = pattern
                    .entries()
                    .iter()
                    .position(BoundPatternEntry::is_remaining);

                let mut prefix_position = 0_usize;

                for (position, entry) in pattern.entries().iter().enumerate() {
                    if entry.is_remaining() {
                        continue;
                    }

                    let subject = self.subject(*element, parent.is_recovered);

                    let projection = if remaining.is_some_and(|remaining| position > remaining) {
                        let from_end = pattern.entries()[position + 1..]
                            .iter()
                            .filter(|entry| !entry.is_remaining())
                            .count();

                        PatternProjection::ElementFromEnd(symbol_ordinal(from_end))
                    } else {
                        let projection =
                            PatternProjection::ElementFromStart(symbol_ordinal(prefix_position));

                        prefix_position += 1;
                        projection
                    };

                    self.push_entry(entry, subject, projection, &mut children);
                }
            }
            (
                BoundPatternKind::Product,
                TypeData::Named {
                    definition: NamedTypeSymbolId::Struct(structure),
                    substitution,
                },
            ) => {
                for entry in pattern.entries() {
                    if entry.is_remaining() {
                        continue;
                    }

                    let Some(name) = entry.name() else {
                        self.push_recovered_entry(entry, &mut children);

                        continue;
                    };

                    let field = match self
                        .request
                        .symbols()
                        .lookup_member((*structure).into(), name.as_str())
                    {
                        MemberLookupResult::Found(AnySymbolId::StructField(field)) => field,
                        MemberLookupResult::Found(_)
                        | MemberLookupResult::NotFound
                        | MemberLookupResult::WrongKind(_)
                        | MemberLookupResult::Ambiguous(_)
                        | MemberLookupResult::Inaccessible(_)
                        | MemberLookupResult::Malformed(_) => {
                            self.push_recovered_entry(entry, &mut children);

                            continue;
                        }
                    };

                    let subject = self.field_subject(
                        SymbolFactRequest::<StructFieldTypeFact>::new(field),
                        *substitution,
                    )?;

                    let projection = PatternProjection::ProductField(field);

                    self.push_entry(entry, subject, projection, &mut children);
                }
            }
            (
                BoundPatternKind::Variant,
                TypeData::Named {
                    definition: NamedTypeSymbolId::Union(_),
                    substitution,
                },
            ) => {
                let Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))) = target
                else {
                    for entry in pattern.entries() {
                        self.push_recovered_entry(entry, &mut children);
                    }

                    return Ok(children);
                };

                let mut position = 0_usize;

                for entry in pattern.entries() {
                    if entry.is_remaining() {
                        continue;
                    }

                    let Some(field) = self.union_payload_field(variant, entry, position) else {
                        self.push_recovered_entry(entry, &mut children);
                        position += 1;

                        continue;
                    };

                    let subject = self.field_subject(
                        SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(field),
                        *substitution,
                    )?;

                    let projection = PatternProjection::ActiveUnionPayloadField { variant, field };

                    self.push_entry(entry, subject, projection, &mut children);

                    position += 1;
                }
            }
            _ => {
                for child in pattern.children() {
                    children
                        .patterns
                        .insert(*child, (self.recovered_subject(), None));
                }

                children.is_recovered |= !pattern.children().is_empty();
            }
        }

        Ok(children)
    }

    fn push_uniform_children(
        &self,
        pattern: &BoundPattern,
        subject: PatternSubject,
        projection: PatternProjection,
        children: &mut PatternChildren,
    ) {
        for child in pattern.children() {
            children
                .patterns
                .insert(*child, (subject, Some(projection)));
        }
    }

    fn push_entry(
        &self,
        entry: &BoundPatternEntry,
        subject: PatternSubject,
        projection: PatternProjection,
        children: &mut PatternChildren,
    ) {
        if let Some(pattern) = entry.pattern() {
            children
                .patterns
                .insert(pattern, (subject, Some(projection)));
        }

        children.entries.push((subject, Some(projection)));
        children.is_recovered |= subject.is_recovered;
    }

    fn push_recovered_entry(&self, entry: &BoundPatternEntry, children: &mut PatternChildren) {
        let subject = self.recovered_subject();

        if let Some(pattern) = entry.pattern() {
            children.patterns.insert(pattern, (subject, None));
        }

        children.entries.push((subject, None));
        children.is_recovered = true;
    }

    pub(in crate::pattern) fn union_payload_field(
        &self,
        variant: UnionVariantSymbolId,
        entry: &BoundPatternEntry,
        position: usize,
    ) -> Option<UnionPayloadFieldSymbolId> {
        if let Some(name) = entry.name() {
            let MemberLookupResult::Found(AnySymbolId::UnionPayloadField(field)) = self
                .request
                .symbols()
                .lookup_member(variant.into(), name.as_str())
            else {
                return None;
            };

            return Some(field);
        }

        self.request
            .symbols()
            .union_variant(variant)?
            .payload_fields()
            .get(position)
            .copied()
    }

    fn field_subject<F>(
        &mut self,
        request: SymbolFactRequest<F>,
        substitution: GenericSubstitutionId,
    ) -> Result<PatternSubject, CheckerInfrastructureError>
    where
        F: SymbolFactContract<Value = TypeExpressionTemplate>,
        C: CheckerSemanticFactProvider<F>,
    {
        let result = match self.request.symbol_fact(request) {
            Ok(result) => result,
            Err(CheckerFactError::Cancelled) => return Ok(self.recovered_subject()),
            Err(CheckerFactError::Infrastructure(error)) => return Err(error),
        };

        self.diagnostics
            .extend(result.diagnostics().iter().cloned());

        self.resolve_field_subject(
            result.value(),
            substitution,
            result.diagnostics().has_errors(),
        )
    }

    fn resolve_field_subject(
        &mut self,
        template: &TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
        mut is_recovered: bool,
    ) -> Result<PatternSubject, CheckerInfrastructureError> {
        let constants = match self.request.checked_constant_terms(template) {
            Ok(constants) => constants,
            Err(CheckerFactError::Cancelled) => return Ok(self.recovered_subject()),
            Err(CheckerFactError::Infrastructure(error)) => return Err(error),
        };

        is_recovered |= constants.diagnostics().has_errors();
        self.diagnostics
            .extend(constants.diagnostics().iter().cloned());

        let Some(ty) = resolve_type_expression_template(
            self.request.semantic_values(),
            template,
            constants.value(),
        )?
        else {
            return Ok(self.recovered_subject());
        };

        let ty = self
            .request
            .semantic_values()
            .substitute_type(ty, substitution)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        Ok(self.subject(ty, is_recovered))
    }

    pub(super) const fn subject(&self, ty: TypeId, is_recovered: bool) -> PatternSubject {
        PatternSubject { ty, is_recovered }
    }

    pub(super) const fn recovered_subject(&self) -> PatternSubject {
        PatternSubject {
            ty: self.error_type,
            is_recovered: true,
        }
    }
}
