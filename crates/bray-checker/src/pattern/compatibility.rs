use std::collections::BTreeSet;

use bray_bound_tree::{BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternTarget};
use bray_compiler_known::{NumericRepresentationKind, RepresentationRole};
use bray_symbols::{
    AnySymbolId, ConstantTermData, ConstantValueKind, NamedTypeSymbolId, StructFieldTypeFact,
    TypeData, UnionPayloadFieldTypeFact,
};

use super::check::PatternChecker;
use crate::constant::integer_to_usize;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerSemanticFactProvider};

impl<C> PatternChecker<'_, '_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<StructFieldTypeFact>
        + CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
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
    ) -> Result<bool, CheckerInfrastructureError> {
        if let TypeData::Borrow {
            target: borrowed_target,
            ..
        } = subject
        {
            let target_data = self
                .request
                .semantic_values()
                .type_data(*borrowed_target)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            return self.pattern_is_compatible(
                id,
                pattern,
                kind,
                target,
                *borrowed_target,
                target_data.as_ref(),
            );
        }

        let compatible = match kind {
            BoundPatternKind::Binding
            | BoundPatternKind::Discard
            | BoundPatternKind::Grouped
            | BoundPatternKind::Alternative
            | BoundPatternKind::Remaining
            | BoundPatternKind::Error => true,
            BoundPatternKind::Path => {
                self.constant_pattern_type(id, target)?
                    .is_none_or(|constant_type| {
                        matches!(subject, TypeData::Error) || constant_type == subject_type
                    })
            }
            BoundPatternKind::Literal => pattern
                .literal()
                .is_some_and(|literal| self.type_accepts_literal(subject, literal.kind())),
            BoundPatternKind::NullableAbsent | BoundPatternKind::NullablePresent => {
                matches!(subject, TypeData::Nullable(_))
            }
            BoundPatternKind::Box => matches!(subject, TypeData::OwnedIndirection { .. }),
            BoundPatternKind::Product => self.product_shape_is_compatible(pattern, subject),
            BoundPatternKind::Tuple => {
                matches!(subject, TypeData::Tuple(elements) if elements.len() == pattern.children().len())
            }
            BoundPatternKind::Array => self.array_shape_is_compatible(pattern, subject)?,
            BoundPatternKind::Variant => self.variant_shape_is_compatible(pattern, target, subject),
        };

        Ok(compatible)
    }

    fn constant_pattern_type(
        &self,
        pattern: BoundPatternId,
        target: Option<BoundPatternTarget>,
    ) -> Result<Option<bray_symbols::TypeId>, CheckerInfrastructureError> {
        if !target.is_some_and(BoundPatternTarget::is_constant) {
            return Ok(None);
        }

        let Some(evidence) = self.constant_patterns.get(&pattern) else {
            return Ok(None);
        };

        let term = self
            .request
            .semantic_values()
            .constant_term_data(evidence.term())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let is_error = match term.as_ref() {
            ConstantTermData::Value(value) => self
                .request
                .semantic_values()
                .constant_value_data(*value)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
                .map(|value| matches!(value.kind(), ConstantValueKind::Error))?,
            _ => false,
        };

        Ok((!is_error).then_some(evidence.ty()))
    }

    pub(super) fn constant_pattern_is_recovered(
        &self,
        pattern: BoundPatternId,
        target: Option<BoundPatternTarget>,
    ) -> Result<bool, CheckerInfrastructureError> {
        let Some(evidence) = self.constant_patterns.get(&pattern) else {
            return Ok(target.is_some_and(BoundPatternTarget::is_constant));
        };

        let term = self
            .request
            .semantic_values()
            .constant_term_data(evidence.term())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let ConstantTermData::Value(value) = term.as_ref() else {
            return Ok(false);
        };

        let value = self
            .request
            .semantic_values()
            .constant_value_data(*value)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        Ok(matches!(value.kind(), ConstantValueKind::Error))
    }

    fn product_shape_is_compatible(&self, pattern: &BoundPattern, subject: &TypeData) -> bool {
        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(structure),
            ..
        } = subject
        else {
            return false;
        };

        let Some(structure) = self.request.symbols().structure(*structure) else {
            return false;
        };

        let mut selected = BTreeSet::new();
        let mut has_remaining = false;

        for entry in pattern.entries() {
            if entry.is_remaining() {
                has_remaining = true;

                continue;
            }

            let Some(name) = entry.name() else {
                return false;
            };

            if !selected.insert(name.as_str())
                || !structure.fields().iter().any(|field| {
                    self.request
                        .symbols()
                        .member_name((*field).into())
                        .is_some_and(|candidate| candidate == name)
                })
            {
                return false;
            }
        }

        has_remaining || selected.len() == structure.fields().len()
    }

    fn array_shape_is_compatible(
        &self,
        pattern: &BoundPattern,
        subject: &TypeData,
    ) -> Result<bool, CheckerInfrastructureError> {
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
    ) -> Result<Option<usize>, CheckerInfrastructureError> {
        let values = self.request.semantic_values();

        let term = values
            .constant_term_data(length)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let integer = match term.as_ref() {
            ConstantTermData::IntegerLiteral { value, .. } => value,
            ConstantTermData::Value(value) => {
                let value = values
                    .constant_value_data(*value)
                    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

                let ConstantValueKind::Integer(integer) = value.kind() else {
                    return Ok(None);
                };

                return Ok(integer_to_usize(integer));
            }
            ConstantTermData::Parameter(_)
            | ConstantTermData::TargetFact(_)
            | ConstantTermData::Unary { .. }
            | ConstantTermData::Binary { .. }
            | ConstantTermData::Conversion { .. }
            | ConstantTermData::NullablePresent(_)
            | ConstantTermData::Tuple(_)
            | ConstantTermData::Array(_)
            | ConstantTermData::Product(_)
            | ConstantTermData::Union { .. }
            | ConstantTermData::DefinitionApplication { .. }
            | ConstantTermData::Call { .. }
            | ConstantTermData::PredicateCall { .. }
            | ConstantTermData::Projection(_) => return Ok(None),
        };

        Ok(integer_to_usize(integer))
    }

    fn type_accepts_literal(
        &self,
        subject: &TypeData,
        literal: bray_bound_tree::BoundLiteralKind,
    ) -> bool {
        let TypeData::Named {
            definition: NamedTypeSymbolId::Struct(structure),
            ..
        } = subject
        else {
            return false;
        };

        let Some(role) = self
            .request
            .available_compiler_known_symbols()
            .symbol_representation(*structure)
        else {
            return false;
        };

        match literal {
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
        }
    }

    fn variant_shape_is_compatible(
        &self,
        pattern: &BoundPattern,
        target: Option<BoundPatternTarget>,
        subject: &TypeData,
    ) -> bool {
        let (
            Some(BoundPatternTarget::Surface(AnySymbolId::UnionVariant(variant))),
            TypeData::Named {
                definition: NamedTypeSymbolId::Union(union),
                ..
            },
        ) = (target, subject)
        else {
            return false;
        };

        let Some(variant) = self
            .request
            .symbols()
            .union_variant(variant)
            .filter(|record| record.union() == *union)
        else {
            return false;
        };

        let mut selected = BTreeSet::new();
        let mut has_remaining = false;
        let mut position = 0_usize;

        for entry in pattern.entries() {
            if entry.is_remaining() {
                has_remaining = true;

                continue;
            }

            let Some(field) = self.union_payload_field(variant.id(), entry, position) else {
                return false;
            };

            if !selected.insert(field) {
                return false;
            }

            position += 1;
        }

        has_remaining || selected.len() == variant.payload_fields().len()
    }
}
