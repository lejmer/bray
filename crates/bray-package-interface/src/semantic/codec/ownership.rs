use super::common::{
    SemanticDecodeContext, read_count, read_optional_u32, read_u32, write_count, write_optional_u32,
};
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceConstantTermId, InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits,
};

pub(super) fn encode_owned_result(
    encoder: &mut WireEncoder,
    result: &bray_symbols::DependencyOwnedResult<
        crate::InterfaceConstantProjection,
        InterfaceConstantTermId,
    >,
) {
    let mut pending = vec![result];

    while let Some(result) = pending.pop() {
        use bray_symbols::DependencyOwnedResult;

        match result {
            DependencyOwnedResult::Case { condition, result } => {
                encoder.write_u32(3);
                encoder.write_u32(condition.raw());
                pending.push(result);
            }
            DependencyOwnedResult::Transfer(transfer) => {
                encoder.write_u32(0);
                encoder.write_u32(transfer.input().raw());

                write_optional_u32(
                    encoder,
                    transfer.condition().map(|condition| condition.raw()),
                );

                write_count(encoder, transfer.output().len());

                for projection in transfer.output() {
                    super::encoding::value::encode_constant_projection(encoder, projection);
                }

                encode_preservation(encoder, transfer.preservation());
            }
            DependencyOwnedResult::All(values) | DependencyOwnedResult::Choice(values) => {
                encoder.write_u32(if matches!(result, DependencyOwnedResult::All(_)) {
                    1
                } else {
                    2
                });

                write_count(encoder, values.len());
                pending.extend(values.iter().rev());
            }
        }
    }
}

pub(super) fn decode_owned_result(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<
    bray_symbols::DependencyOwnedResult<
        crate::InterfaceConstantProjection,
        InterfaceConstantTermId,
    >,
    InterfaceValidationError,
> {
    use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer};

    let mut pending = Vec::new();

    loop {
        limits.check(
            InterfaceLimit::SemanticTypeDepth,
            u64::try_from(pending.len())
                .expect("ownership nesting fits the target")
                .saturating_add(1),
        )?;

        let tag = read_u32(reader)?;

        let mut result = match tag {
            0 => {
                let input = InterfaceConstantTermId::new(read_u32(reader)?);
                let condition = read_optional_u32(reader)?.map(InterfaceConstantTermId::new);
                let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                let mut output = context.allocate_items(reader, count)?;

                for _ in 0..count {
                    output.push(super::decoding::value::decode_constant_projection(
                        reader, context,
                    )?);
                }

                let depth = u64::try_from(pending.len()).expect("ownership nesting fits the target").saturating_add(1);
                let preservation = decode_preservation(reader, limits, context, depth)?;

                DependencyOwnedResult::Transfer(
                    DependencyOwnedTransfer::new(input, output)
                        .with_condition(condition)
                        .with_preservation(preservation),
                )
            }
            3 => {
                let condition = InterfaceConstantTermId::new(read_u32(reader)?);
                let values = context.allocate_items(reader, 1)?;

                pending.push((tag, 1, values, Some(condition)));
                continue;
            }
            1 | 2 => {
                let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                let values = context.allocate_items(reader, count)?;

                if count != 0 {
                    pending.push((tag, count, values, None));
                    continue;
                }

                DependencyOwnedResult::default()
            }
            _ => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Dependency,
                    tag,
                ));
            }
        };

        loop {
            let Some((tag, count, values, _)) = pending.last_mut() else {
                return Ok(result);
            };

            values.push(result);

            if values.len() < *count {
                break;
            }

            let tag = *tag;

            let (_, _, values, condition) = pending
                .pop()
                .expect("completed ownership layout retains its pending node");

            result = if let Some(condition) = condition {
                DependencyOwnedResult::case(
                    condition,
                    values
                        .into_iter()
                        .next()
                        .expect("conditional layout retains its single child"),
                )
            } else if tag == 1 {
                DependencyOwnedResult::all(values)
            } else {
                DependencyOwnedResult::choice(values)
            };
        }
    }
}

fn encode_preservation(
    encoder: &mut WireEncoder,
    mask: &bray_symbols::DependencyValuePreservation<crate::InterfaceConstantProjection>,
) {
    let mut pending = vec![(None, mask)];

    while let Some((projection, mask)) = pending.pop() {
        if let Some(projection) = projection {
            super::encoding::value::encode_constant_projection(encoder, projection);
        }

        encoder.write_u32(u32::from(mask.preserves_value()));
        write_count(encoder, mask.children().len());
        pending.extend(mask.children().iter().rev().map(|(projection, child)| (Some(projection), child)));
    }
}

fn decode_preservation(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    depth: u64,
) -> Result<bray_symbols::DependencyValuePreservation<crate::InterfaceConstantProjection>, InterfaceValidationError> {
    limits.check(InterfaceLimit::SemanticTypeDepth, depth)?;

    let preserved = match read_u32(reader)? {
        0 => false,
        1 => true,
        flag => return Err(super::invalid_discriminant(crate::InterfaceValidationField::Dependency, flag)),
    };

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut children = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let projection = super::decoding::value::decode_constant_projection(reader, context)?;

        if children.last().is_some_and(|(previous, _)| previous >= &projection) {
            return Err(super::invalid_value(crate::InterfaceValidationField::Dependency));
        }

        let child = decode_preservation(reader, limits, context, depth.saturating_add(1))?;

        if child.preserves_value() == preserved && child.children().is_empty() {
            return Err(super::invalid_value(crate::InterfaceValidationField::Dependency));
        }

        children.push((projection, child));
    }

    Ok(bray_symbols::DependencyValuePreservation::new(preserved, children.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::{decode_owned_result, encode_owned_result};
    use crate::semantic::codec::common::SemanticDecodeContext;
    use crate::wire::{WireEncoder, WireReader};
    use crate::{
        InterfaceConstantProjection, InterfaceConstantTermId, InterfaceLimit,
        InterfaceValidationError, InterfaceValidationLimits,
    };
    use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer, SymbolOrdinal};

    #[test]
    fn complete_alternatives_round_trip_without_dropping_empty_paths() {
        let moved = |input, output| {
            DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                InterfaceConstantTermId::new(input),
                [InterfaceConstantProjection::TupleElement(
                    SymbolOrdinal::new(output),
                )],
            ).with_preservation(bray_symbols::DependencyValuePreservation::new(true, [(
                InterfaceConstantProjection::TupleElement(SymbolOrdinal::new(0)),
                bray_symbols::DependencyValuePreservation::none(),
            )].into())))
        };

        let result = DependencyOwnedResult::choice([
            DependencyOwnedResult::all([moved(0, 0), moved(1, 1)]),
            DependencyOwnedResult::all([moved(0, 1), moved(1, 0)]),
            DependencyOwnedResult::case(
                InterfaceConstantTermId::new(2),
                DependencyOwnedResult::default(),
            ),
        ]);

        let mut encoder = WireEncoder::new();

        encode_owned_result(&mut encoder, &result);

        let bytes = encoder.into_bytes();
        let limits = InterfaceValidationLimits::default();
        let mut reader = WireReader::new(&bytes);

        let decoded =
            decode_owned_result(&mut reader, limits, &mut SemanticDecodeContext::new(limits))
                .expect("complete ownership alternatives decode");

        reader
            .finish()
            .expect("ownership alternatives consume the complete record");

        assert_eq!(decoded, result);

        for length in 0..bytes.len() {
            assert!(
                decode_owned_result(
                    &mut WireReader::new(&bytes[..length]),
                    limits,
                    &mut SemanticDecodeContext::new(limits)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn preservation_nesting_is_bounded_and_flags_are_validated() {
        use bray_symbols::DependencyValuePreservation as Mask;

        let projection = InterfaceConstantProjection::TupleElement(SymbolOrdinal::new(0));
        let mask = Mask::new(true, [(projection.clone(), Mask::new(false, [(projection, Mask::all())].into()))].into());

        let result = DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(InterfaceConstantTermId::new(0), [])
            .with_preservation(mask));

        let mut encoder = WireEncoder::new();

        encode_owned_result(&mut encoder, &result);

        let mut bytes = encoder.into_bytes();
        let limits = InterfaceValidationLimits::default().with_semantic_type_depth(2);

        assert!(matches!(decode_owned_result(&mut WireReader::new(&bytes), limits, &mut SemanticDecodeContext::new(limits)),
            Err(InterfaceValidationError::ResourceLimitExceeded { limit: InterfaceLimit::SemanticTypeDepth, .. })));

        let flag = bytes.len() - 8;

        bytes[flag..flag + 4].copy_from_slice(&2_u32.to_le_bytes());

        let limits = InterfaceValidationLimits::default();

        assert!(decode_owned_result(&mut WireReader::new(&bytes), limits, &mut SemanticDecodeContext::new(limits)).is_err());
    }

    #[test]
    fn ownership_nesting_is_bounded_before_normalization() {
        let words = [1_u32, 1, 2, 1, 1, 0];

        let bytes = words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();

        let limits = InterfaceValidationLimits::default().with_semantic_type_depth(2);

        assert!(matches!(
            decode_owned_result(
                &mut WireReader::new(&bytes),
                limits,
                &mut SemanticDecodeContext::new(limits)
            ),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::SemanticTypeDepth,
                ..
            })
        ));
    }
}
