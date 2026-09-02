use bray_symbols::{
    ExternalSymbolKey, ModulePathKey, PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal,
    SynthesizedSymbolRole,
};

use super::configuration::{read_configuration, read_ordinals, write_configuration};
use super::contract::{decode_specialization_key, encode_specialization_key};
use super::wire::read_arguments;
use crate::decode::DecodeBudget;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    ImplementationSpecializationArgument, ImplementationSpecializationArgumentKind,
    ImplementationSpecializationWitness, InterfaceValidationError, InterfaceValidationLimits,
    PackageImplementationSpecializationKey,
};

#[test]
fn specialization_keys_round_trip_complete_structured_symbol_keys() {
    let declaration = nested_external_key();

    let witness = ExternalSymbolKey::named(
        declaration
            .owner()
            .cloned()
            .unwrap_or_else(|| panic!("declaration must have an owner")),
        SymbolKind::NamedTraitImplementation,
        symbol_name("Printable"),
    )
    .unwrap_or_else(|| panic!("implementation key must be valid"));

    let key = PackageImplementationSpecializationKey::new(
        ImplementationExternalSymbolIdentity::new(&declaration),
        [ImplementationSpecializationArgument::new(
            ImplementationSpecializationArgumentKind::Type,
            [7; 32],
        )],
        [ImplementationSpecializationWitness::new(
            ImplementationExternalSymbolIdentity::new(&witness),
            [],
        )],
        crate::test_support::implementation_configuration(),
        CURRENT_TEMPLATE_SCHEMA_REVISION,
        [],
    );

    let mut encoder = WireEncoder::new();

    encode_specialization_key(&key, &mut encoder);

    let mut reader = WireReader::new(encoder.bytes());

    let decoded = decode_specialization_key(&mut reader, InterfaceValidationLimits::default())
        .unwrap_or_else(|error| panic!("structured specialization key must decode: {error:?}"));

    reader
        .finish()
        .unwrap_or_else(|error| panic!("specialization key must consume its payload: {error:?}"));

    assert_eq!(decoded, key);
    assert_eq!(decoded.cache_identity(), key.cache_identity());
}

#[test]
fn collection_counts_must_fit_the_remaining_payload_before_allocation() {
    let bytes = u32::MAX.to_le_bytes();
    let mut reader = WireReader::new(&bytes);
    let mut budget = DecodeBudget::new(InterfaceValidationLimits::default());

    assert_eq!(
        read_arguments(&mut reader, &mut budget),
        Err(InterfaceValidationError::ResourceLimitExceeded {
            limit: crate::InterfaceLimit::RecordCount,
            actual: u64::from(u32::MAX),
            maximum: InterfaceValidationLimits::default()
                .maximum(crate::InterfaceLimit::RecordCount),
        })
    );

    let bytes = 4_u32.to_le_bytes();
    let mut reader = WireReader::new(&bytes);
    let mut budget = DecodeBudget::new(InterfaceValidationLimits::default());

    assert_eq!(
        read_ordinals(&mut reader, &mut budget, [0_u8, 1]),
        Err(InterfaceValidationError::Truncated {
            context: crate::InterfaceValidationContext::Artifact,
            field: crate::InterfaceValidationField::Ordinal,
            offset: 4,
            expected_length: 8,
            actual_length: 0,
        })
    );
}

#[test]
fn target_machine_properties_must_match_their_individual_target_properties() {
    let configuration = crate::test_support::implementation_configuration();
    let mut encoder = WireEncoder::new();

    write_configuration(&mut encoder, &configuration);

    let mut bytes = encoder.into_bytes();
    let pointer_width_offset = 4 + configuration.target().as_str().len() + 3;

    bytes[pointer_width_offset..pointer_width_offset + 2].copy_from_slice(&32_u16.to_le_bytes());

    let mut reader = WireReader::new(&bytes);
    let mut budget = DecodeBudget::new(InterfaceValidationLimits::default());

    assert_eq!(
        read_configuration(&mut reader, &mut budget),
        Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::TargetProperty
        ))
    );
}

fn nested_external_key() -> ExternalSymbolKey {
    let package = PackageIdentity::try_new("example.codec")
        .unwrap_or_else(|| panic!("package identity must be valid"));

    let module = ExternalSymbolKey::module(
        ExternalSymbolKey::package(package),
        ModulePathKey::try_new(["text", "format"])
            .unwrap_or_else(|| panic!("module path must be valid")),
    )
    .unwrap_or_else(|| panic!("module key must be valid"));

    let declaration = ExternalSymbolKey::named(module, SymbolKind::Function, symbol_name("render"))
        .unwrap_or_else(|| panic!("function key must be valid"));

    ExternalSymbolKey::synthesized(
        declaration,
        SynthesizedSymbolRole::DeclaredGenericTypeParameter,
        Some(SymbolOrdinal::new(0)),
    )
    .unwrap_or_else(|| panic!("synthesized key must be valid"))
}

fn symbol_name(value: &str) -> SymbolName {
    SymbolName::try_new(value).unwrap_or_else(|| panic!("symbol name must be valid"))
}
