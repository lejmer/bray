use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_symbol_reference, read_u32,
};
use crate::tag::WireTag;
use crate::validation::is_strictly_sorted;
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceSectionTag, InterfaceSemanticRecord, InterfaceSemanticRecordKind,
    InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use std::sync::Arc;

use crate::semantic::codec::index::SemanticDirectory;

pub(crate) fn decode_semantic_directory(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Arc<[InterfaceSemanticRecord]>, InterfaceValidationError> {
    let Some(index) = context.record_index().map(Arc::clone) else {
        return read_semantic_directory(section, limits, context);
    };

    let directory = index.directory.get_or_init(|| {
        let mut context = SemanticDecodeContext::new(limits);
        let records = read_semantic_directory(section, limits, &mut context)?;

        Ok(SemanticDirectory {
            records,
            budget: context.into_budget(),
        })
    });

    // Replay failed reads with the query budget to preserve the first typed failure.
    let Ok(directory) = directory else {
        return read_semantic_directory(section, limits, context);
    };

    if context.include_budget(&directory.budget).is_err() {
        return read_semantic_directory(section, limits, context);
    }

    Ok(Arc::clone(&directory.records))
}

pub(super) fn records_for_owner<'records>(
    directory: &'records [InterfaceSemanticRecord],
    owner: &crate::InterfaceSymbolReference,
    kind: InterfaceSemanticRecordKind,
) -> &'records [InterfaceSemanticRecord] {
    let key = (owner, kind);
    let start = directory.partition_point(|entry| (entry.owner(), entry.kind()) < key);

    let end =
        start + directory[start..].partition_point(|entry| (entry.owner(), entry.kind()) == key);

    &directory[start..end]
}

fn read_semantic_directory(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Arc<[InterfaceSemanticRecord]>, InterfaceValidationError> {
    let count = usize::try_from(section.record_count()).map_err(|_| {
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference)
    })?;

    limits.check(InterfaceLimit::RecordCount, section.record_count())?;

    let mut reader = WireReader::new(section.bytes());
    let mut entries = context.allocate_items(&reader, count)?;

    for _ in 0..count {
        let owner = read_symbol_reference(&mut reader, context)?;

        let kind_raw = read_u32(&mut reader)?;

        let kind = InterfaceSemanticRecordKind::from_wire(kind_raw).ok_or(
            crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::Discriminant,
                kind_raw,
            ),
        )?;

        let section_raw = read_u32(&mut reader)?;

        let section = InterfaceSectionTag::from_wire_value(section_raw).ok_or(
            crate::semantic::codec::invalid_discriminant(
                crate::InterfaceValidationField::SectionTag,
                section_raw,
            ),
        )?;

        let record = read_u32(&mut reader)?;

        entries.push(InterfaceSemanticRecord {
            owner,
            kind,
            section,
            record,
        });
    }

    reader.finish().map_err(map_wire_error)?;

    if !is_strictly_sorted(&entries) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(entries.into())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::decode_semantic_directory;
    use crate::semantic::SemanticRecordIndex;
    use crate::semantic::codec::common::{
        SemanticDecodeContext, read_symbol_reference, write_symbol_reference,
    };
    use crate::tag::WireTag;
    use crate::wire::{WireEncoder, WireReader};
    use crate::{
        InterfaceSectionTag, InterfaceSemanticRecordKind, InterfaceSymbolReference,
        InterfaceValidationLimits, ValidatedInterfaceSection,
    };

    #[test]
    fn shared_directory_keeps_query_allocation_and_external_reference_limits() {
        let owner = InterfaceSymbolReference::Dependency {
            dependency: crate::DependencyInterfaceId::new(0),
            key: crate::test_support::module_key(
                bray_symbols::PackageIdentity::try_new("example.dependency")
                    .expect("test package must be valid"),
                "module",
            ),
        };

        let mut encoder = WireEncoder::new();

        write_symbol_reference(&mut encoder, &owner);

        let reference_length = encoder.bytes().len();

        encoder.write_u32(InterfaceSemanticRecordKind::CallableSignature.to_wire());
        encoder.write_u32(InterfaceSectionTag::DeclarationSemantics.wire_value());
        encoder.write_u32(0);

        let section = ValidatedInterfaceSection::for_test(
            InterfaceSectionTag::SemanticRecordDirectory,
            1,
            encoder.bytes(),
        );

        for limits in [
            InterfaceValidationLimits::default().with_decoded_allocation(4_096),
            InterfaceValidationLimits::default().with_external_reference_count(3),
        ] {
            let index = Arc::new(SemanticRecordIndex::default());
            let mut context = SemanticDecodeContext::with_index(limits, Arc::clone(&index));

            let first = decode_semantic_directory(section, limits, &mut context)
                .unwrap_or_else(|error| panic!("directory must decode: {error:?}"));

            let mut context = SemanticDecodeContext::with_index(limits, Arc::clone(&index));

            let second = decode_semantic_directory(section, limits, &mut context)
                .unwrap_or_else(|error| panic!("shared directory must decode: {error:?}"));

            assert!(Arc::ptr_eq(&first, &second));

            let mut raw = SemanticDecodeContext::new(limits);
            let mut cached = SemanticDecodeContext::with_index(limits, Arc::clone(&index));

            for context in [&mut raw, &mut cached] {
                if limits.maximum(crate::InterfaceLimit::DecodedAllocation) == 4_096 {
                    context
                        .charge_items::<u8>(4_090)
                        .expect("prior allocation must fit");
                } else {
                    let mut reader = WireReader::new(&encoder.bytes()[..reference_length]);

                    read_symbol_reference(&mut reader, context)
                        .unwrap_or_else(|error| panic!("prior reference must decode: {error:?}"));
                }
            }

            let expected = decode_semantic_directory(section, limits, &mut raw);

            assert!(expected.is_err());

            assert_eq!(
                decode_semantic_directory(section, limits, &mut cached),
                expected
            );
        }
    }

    #[test]
    fn cached_directory_preserves_ordering_errors() {
        let mut encoder = WireEncoder::new();

        for _ in 0..2 {
            write_symbol_reference(
                &mut encoder,
                &InterfaceSymbolReference::Local(bray_symbols::InterfaceSymbolId::new(0)),
            );

            encoder.write_u32(InterfaceSemanticRecordKind::CallableSignature.to_wire());
            encoder.write_u32(InterfaceSectionTag::DeclarationSemantics.wire_value());
            encoder.write_u32(0);
        }

        let section = ValidatedInterfaceSection::for_test(
            InterfaceSectionTag::SemanticRecordDirectory,
            2,
            encoder.bytes(),
        );

        let limits = InterfaceValidationLimits::default();
        let mut context = SemanticDecodeContext::new(limits);
        let expected = decode_semantic_directory(section, limits, &mut context);

        assert!(expected.is_err());

        let index = Arc::new(SemanticRecordIndex::default());

        for _ in 0..2 {
            let mut context = SemanticDecodeContext::with_index(limits, Arc::clone(&index));

            assert_eq!(
                decode_semantic_directory(section, limits, &mut context),
                expected
            );
        }
    }
}
