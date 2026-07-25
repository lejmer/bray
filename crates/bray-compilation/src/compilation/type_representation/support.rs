use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_source::SourceSpan;
use bray_symbols::{ConstantTermData, ConstantValueKind, IntegerConstant};

use crate::fact::FactQueryError;

pub(super) fn checked_integer(
    values: &bray_symbols::SemanticValueStore,
    term: bray_symbols::ConstantTermId,
) -> Result<Option<u64>, FactQueryError> {
    checked_integer_constant(values, term).map(|value| value.and_then(|value| value.to_u64()))
}

pub(super) fn checked_integer_constant(
    values: &bray_symbols::SemanticValueStore,
    term: bray_symbols::ConstantTermId,
) -> Result<Option<IntegerConstant>, FactQueryError> {
    let data = values
        .constant_term_data(term)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let value = match data.as_ref() {
        // Integer constants use shared immutable magnitude storage, so cloning preserves the
        // checked value without copying its arbitrary-width byte sequence.
        ConstantTermData::IntegerLiteral { value, .. } => Some(value.clone()),
        ConstantTermData::Value(value) => {
            let value = values
                .constant_value_data(*value)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let ConstantValueKind::Integer(integer) = value.kind() else {
                return Ok(None);
            };

            Some(integer.clone())
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
        | ConstantTermData::Projection(_) => None,
    };

    Ok(value)
}

pub(super) fn integer_role(text: &str) -> Option<RepresentationRole> {
    match text {
        "i8" => Some(RepresentationRole::ScalarI8),
        "i16" => Some(RepresentationRole::ScalarI16),
        "i32" => Some(RepresentationRole::ScalarI32),
        "i64" => Some(RepresentationRole::ScalarI64),
        "i128" => Some(RepresentationRole::ScalarI128),
        "u8" => Some(RepresentationRole::ScalarU8),
        "u16" => Some(RepresentationRole::ScalarU16),
        "u32" => Some(RepresentationRole::ScalarU32),
        "u64" => Some(RepresentationRole::ScalarU64),
        "u128" => Some(RepresentationRole::ScalarU128),
        "isize" => Some(RepresentationRole::ScalarIsize),
        "usize" => Some(RepresentationRole::ScalarUsize),
        _ => None,
    }
}

pub(super) fn symbol_span(syntax: Option<SyntaxAnchor>) -> Result<SourceSpan, FactQueryError> {
    let syntax = syntax.ok_or(FactQueryError::InfrastructureFailure)?;

    Ok(SourceSpan::new(syntax.source_id(), syntax.full_range()))
}
