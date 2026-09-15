use std::collections::BTreeSet;

use bray_bound_tree::{BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternTarget};
use bray_compiler_known::{NumericRepresentationKind, RepresentationRole};
use bray_symbols::{
    AnySymbolId, ConstantTermData, ConstantValueKind, NamedTypeSymbolId, StructFieldTypeQuery,
    TypeData, UnionPayloadFieldTypeQuery,
};

use super::check::{PatternChecker, available_dependency};
use crate::constant::integer_to_usize;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider};

impl<C> PatternChecker<'_, '_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
        + ?Sized,
{
    pub(super) fn pattern_is_compatible(
        &self,
        id: BoundPatternId,
        pattern: &BoundPattern,
        kind: BoundPatternKind,
        target: Option<BoundPatternTarget>,
        subject_type: bray_symbols::TypeId,
        subject: &TypeData,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let compatible = match kind {
            BoundPatternKind::Binding
            | BoundPatternKind::Discard
            | BoundPatternKind::Grouped
            | BoundPatternKind::Alternative
            | BoundPatternKind::Remaining
            | BoundPatternKind::Error => true,
            BoundPatternKind::Path => {
                self.constant_pattern_type(id, target)
                    .is_none_or(|constant_type| {
                        matches!(subject, TypeData::Error) || constant_type == subject_type
                    })
            }
            BoundPatternKind::Literal => match pattern.literal() {
                Some(literal) => self.type_accepts_literal(subject, literal.kind())?,
                None => false,
            },
            BoundPatternKind::NullableAbsent | BoundPatternKind::NullablePresent => {
                matches!(subject, TypeData::Nullable(_))
            }
            BoundPatternKind::Box => matches!(subject, TypeData::OwnedIndirection { .. }),
            BoundPatternKind::Product => self.product_shape_is_compatible(pattern, subject)?,
            BoundPatternKind::Tuple => {
                matches!(subject, TypeData::Tuple(elements) if elements.len() == pattern.children().len())
            }
            BoundPatternKind::Array => self.array_shape_is_compatible(pattern, subject)?,
            BoundPatternKind::Variant => {
                self.variant_shape_is_compatible(pattern, target, subject)?
            }
        };

        Ok(compatible)
    }

    fn constant_pattern_type(
        &self,
        pattern: BoundPatternId,
        target: Option<BoundPatternTarget>,
    ) -> Option<bray_symbols::TypeId> {
        if !target.is_some_and(BoundPatternTarget::is_constant) {
            return None;
        }

        let Some(evidence) = self.constant_patterns.get(&pattern) else {
            return None;
        };

        let term = self
            .request
            .semantic_values()
            .constant_term_data(evidence.term());

        let is_error = match term.as_ref() {
            ConstantTermData::Value(value) => matches!(
                self.request
                    .semantic_values()
                    .constant_value_data(*value)
                    .kind(),
                ConstantValueKind::Error
            ),
            _ => false,
        };

        (!is_error).then_some(evidence.ty())
    }

    pub(super) fn constant_pattern_is_recovered(
        &self,
        pattern: BoundPatternId,
        target: Option<BoundPatternTarget>,
    ) -> bool {
        let Some(evidence) = self.constant_patterns.get(&pattern) else {
            return target.is_some_and(BoundPatternTarget::is_constant);
        };

        let term = self
            .request
            .semantic_values()
            .constant_term_data(evidence.term());

        let ConstantTermData::Value(value) = term.as_ref() else {
            return false;
        };

        let value = self.request.semantic_values().constant_value_data(*value);

        matches!(value.kind(), ConstantValueKind::Error)
    }

    fn product_shape_is_compatible(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(structure),
            ..
        } = subject
        else {
            return Ok(false);
        };

        let representation = available_dependency(
            self.request
                .declared_type_representation((*structure).into()),
        )?;

        if representation
            .is_some_and(|representation| representation.value().has_flexible_trailing_member())
        {
            return Ok(false);
        }

        let Some(structure) = available_dependency(self.request.structure(*structure))?.flatten()
        else {
            return Ok(false);
        };

        let mut selected = BTreeSet::new();
        let mut has_remaining = false;

        for entry in pattern.entries() {
            if entry.is_remaining() {
                has_remaining = true;

                continue;
            }

            let Some(name) = entry.name() else {
                return Ok(false);
            };

            if !selected.insert(name.as_str()) {
                return Ok(false);
            }

            let mut found = false;

            for field in structure.fields() {
                let candidate =
                    available_dependency(self.request.member_name((*field).into()))?.flatten();

                if candidate.is_some_and(|candidate| candidate == name) {
                    found = true;

                    break;
                }
            }

            if !found {
                return Ok(false);
            }
        }

        Ok(has_remaining || selected.len() == structure.fields().len())
    }

    fn array_shape_is_compatible(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let explicit = pattern
            .entries()
            .iter()
            .filter(|entry| !entry.is_remaining())
            .count();

        let has_remaining = pattern
            .entries()
            .iter()
            .any(bray_bound_tree::BoundPatternEntry::is_remaining);

        match subject {
            TypeData::Array { length, .. } => {
                let Some(length) = self.fixed_array_length(*length)? else {
                    return Ok(false);
                };

                Ok(if has_remaining {
                    explicit <= length
                } else {
                    explicit == length
                })
            }
            TypeData::Slice(_) => Ok(true),
            _ => Ok(false),
        }
    }

    pub(super) fn fixed_array_length(
        &self,
        length: bray_symbols::ConstantTermId,
    ) -> Result<Option<usize>, CheckerQueryError<C::UpstreamError>> {
        let values = self.request.semantic_values();

        let integer = values.constant_term_integer(length);

        Ok(integer.as_ref().and_then(integer_to_usize))
    }

    fn type_accepts_literal(
        &self,
        subject: &TypeData,
        literal: bray_bound_tree::BoundLiteralKind,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(structure),
            ..
        } = subject
        else {
            return Ok(false);
        };

        let Some(role) = self
            .request
            .available_compiler_known_symbols()
            .symbol_representation(*structure)
        else {
            return Ok(false);
        };

        let accepts = match literal {
            bray_bound_tree::BoundLiteralKind::Boolean => role == RepresentationRole::ScalarBool,
            bray_bound_tree::BoundLiteralKind::Character => role == RepresentationRole::ScalarChar,
            bray_bound_tree::BoundLiteralKind::String => role == RepresentationRole::String,
            bray_bound_tree::BoundLiteralKind::Integer => {
                role.numeric_kind() == Some(NumericRepresentationKind::Integer)
            }
            bray_bound_tree::BoundLiteralKind::Real => {
                role.numeric_kind() == Some(NumericRepresentationKind::Real)
            }
            bray_bound_tree::BoundLiteralKind::Imaginary => {
                role.numeric_kind() == Some(NumericRepresentationKind::Complex)
            }
        };

        Ok(accepts)
    }

    fn variant_shape_is_compatible(
        &self,
        pattern: &BoundPattern,
        target: Option<BoundPatternTarget>,
        subject: &TypeData,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let (
            Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))),
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            },
        ) = (target, subject)
        else {
            return Ok(false);
        };

        let Some(variant) = available_dependency(self.request.union_variant(variant))?
            .flatten()
            .filter(|record| record.union() == *union)
        else {
            return Ok(false);
        };

        let mut selected = BTreeSet::new();
        let mut has_remaining = false;
        let mut position = 0_usize;

        for entry in pattern.entries() {
            if entry.is_remaining() {
                has_remaining = true;

                continue;
            }

            let Some((field, consumes_position)) =
                self.union_payload_field(variant.id(), entry, position)?
            else {
                return Ok(false);
            };

            if !selected.insert(field) {
                return Ok(false);
            }

            if consumes_position {
                position += 1;
            }
        }

        Ok(has_remaining || selected.len() == variant.payload_fields().len())
    }
}
