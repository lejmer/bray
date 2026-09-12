use bray_symbols::{
    CallableExecutionContract, CallableExecutionDomain, CallableExecutionEvidence,
    CallableExecutionObligation, CallableExecutionTarget, SymbolOrdinal,
};

use super::common::decode_tag;
use crate::semantic::codec::common::{SemanticDecodeContext, read_count, read_u32};
use crate::wire::WireReader;
use crate::{
    InterfaceCallableExecutionContract, InterfaceCallableInstanceId, InterfaceConstantTermId,
    InterfaceLimit, InterfaceTypeId, InterfaceValidationError, InterfaceValidationField,
    InterfaceValidationLimits,
};

pub(super) fn decode_execution_contract(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallableExecutionContract, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut domains = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let ordinal = SymbolOrdinal::new(read_u32(reader)?);
        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut entry = context.allocate_items(reader, count)?;

        for _ in 0..count {
            entry.push(InterfaceConstantTermId::new(read_u32(reader)?));
        }

        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut properties = context.allocate_items(reader, count)?;

        for _ in 0..count {
            properties.push(decode_tag(read_u32(reader)?)?);
        }

        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut postconditions = context.allocate_items(reader, count)?;

        for _ in 0..count {
            postconditions.push((
                SymbolOrdinal::new(read_u32(reader)?),
                InterfaceConstantTermId::new(read_u32(reader)?),
            ));
        }

        domains.push(CallableExecutionDomain {
            ordinal,
            entry: entry.into(),
            properties: properties.into(),
            postconditions: postconditions.into(),
        });
    }

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut evidence = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let origin = decode_tag(read_u32(reader)?)?;
        let obligation = decode_obligation(reader)?;
        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut dependencies = context.allocate_items(reader, count)?;

        for _ in 0..count {
            let raw = read_u32(reader)?;

            let target = match raw {
                0 => CallableExecutionTarget::Callable(InterfaceCallableInstanceId::new(read_u32(
                    reader,
                )?)),
                1 => CallableExecutionTarget::Indirect(InterfaceTypeId::new(read_u32(reader)?)),
                _ => {
                    return Err(crate::semantic::codec::invalid_discriminant(
                        InterfaceValidationField::Reference,
                        raw,
                    ));
                }
            };

            dependencies.push((target, decode_obligation(reader)?));
        }

        // Preserve wire order for strict validation before any identity conversion.
        evidence.push(CallableExecutionEvidence {
            origin,
            obligation,
            dependencies: dependencies.into(),
        });
    }

    Ok(CallableExecutionContract {
        domains: domains.into(),
        evidence: evidence.into(),
    })
}

fn decode_obligation(
    reader: &mut WireReader<'_>,
) -> Result<CallableExecutionObligation<SymbolOrdinal>, InterfaceValidationError> {
    let raw = read_u32(reader)?;

    match raw {
        0 => {
            let property = decode_tag(read_u32(reader)?)?;
            let raw = read_u32(reader)?;

            let guard = match raw {
                0 => None,
                1 => Some(SymbolOrdinal::new(read_u32(reader)?)),
                _ => {
                    return Err(crate::semantic::codec::invalid_discriminant(
                        InterfaceValidationField::Reference,
                        raw,
                    ));
                }
            };

            Ok(CallableExecutionObligation::Property(property, guard))
        }
        1 => Ok(CallableExecutionObligation::Postcondition(
            SymbolOrdinal::new(read_u32(reader)?),
        )),
        _ => Err(crate::semantic::codec::invalid_discriminant(
            InterfaceValidationField::Reference,
            raw,
        )),
    }
}
