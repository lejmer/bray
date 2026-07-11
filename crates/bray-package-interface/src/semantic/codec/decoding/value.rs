use super::common::{decode_real, decode_tag, read_ids, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_optional_u32, read_string,
    read_symbol_reference, read_u32,
};
use crate::semantic::model::{
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceDependencyContractId, InterfaceGenericArgument, InterfaceGenericBinding,
    InterfaceGenericSubstitution, InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceSemanticFacts, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceType, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use bray_symbols::{IntegerConstant, IntegerSign, SymbolOrdinal};

pub(super) fn decode_types(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let substitution_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let trait_application_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let callable_instance_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let implementation_instance_count =
        read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let type_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(
        section,
        [
            substitution_count,
            trait_application_count,
            callable_instance_count,
            implementation_instance_count,
            type_count,
        ],
    )?;

    let mut substitutions = Vec::with_capacity(substitution_count);

    for _ in 0..substitution_count {
        let owner = read_symbol_reference(&mut reader, context)?;
        let binding_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
        let mut bindings = Vec::with_capacity(binding_count);

        for _ in 0..binding_count {
            let parameter = read_symbol_reference(&mut reader, context)?;
            let argument = match read_u32(&mut reader)? {
                1 => InterfaceGenericArgument::Type(InterfaceTypeId::new(read_u32(&mut reader)?)),
                2 => InterfaceGenericArgument::Constant(InterfaceConstantTermId::new(read_u32(
                    &mut reader,
                )?)),
                _ => return Err(InterfaceValidationError::Malformed),
            };

            bindings.push(InterfaceGenericBinding::new(parameter, argument));
        }

        substitutions.push(InterfaceGenericSubstitution::new(owner, bindings));
    }

    let mut trait_applications = Vec::with_capacity(trait_application_count);

    for _ in 0..trait_application_count {
        trait_applications.push(InterfaceTraitApplication::new(
            read_symbol_reference(&mut reader, context)?,
            InterfaceGenericSubstitutionId::new(read_u32(&mut reader)?),
        ));
    }

    let mut callable_instances = Vec::with_capacity(callable_instance_count);

    for _ in 0..callable_instance_count {
        callable_instances.push(InterfaceCallableInstance::new(
            read_symbol_reference(&mut reader, context)?,
            InterfaceGenericSubstitutionId::new(read_u32(&mut reader)?),
        ));
    }

    let mut implementation_instances = Vec::with_capacity(implementation_instance_count);

    for _ in 0..implementation_instance_count {
        implementation_instances.push(InterfaceImplementationInstance::new(
            read_symbol_reference(&mut reader, context)?,
            InterfaceGenericSubstitutionId::new(read_u32(&mut reader)?),
        ));
    }

    let mut types = Vec::with_capacity(type_count);

    for _ in 0..type_count {
        types.push(decode_type(&mut reader, limits, context)?);
    }

    reader.finish().map_err(map_wire_error)?;

    Ok(InterfaceSemanticFacts::new()
        .with_applications(
            substitutions,
            trait_applications,
            callable_instances,
            implementation_instances,
        )
        .with_values([], types, [], []))
}

pub(super) fn decode_type(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceType, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceType::Named {
            definition: read_symbol_reference(reader, context)?,
            substitution: InterfaceGenericSubstitutionId::new(read_u32(reader)?),
        }),
        2 => Ok(InterfaceType::TypeParameter(read_symbol_reference(
            reader, context,
        )?)),
        3 => Ok(InterfaceType::AssociatedTypeProjection {
            application: InterfaceTraitApplicationId::new(read_u32(reader)?),
            member: read_symbol_reference(reader, context)?,
        }),
        4 => Ok(InterfaceType::Tuple(read_ids(
            reader,
            limits,
            InterfaceTypeId::new,
        )?)),
        5 => Ok(InterfaceType::Array {
            element: InterfaceTypeId::new(read_u32(reader)?),
            length: InterfaceConstantTermId::new(read_u32(reader)?),
        }),
        6 => Ok(InterfaceType::Slice(InterfaceTypeId::new(read_u32(
            reader,
        )?))),
        7 => Ok(InterfaceType::Nullable(InterfaceTypeId::new(read_u32(
            reader,
        )?))),
        8 => Ok(InterfaceType::Borrow {
            kind: decode_tag(read_u32(reader)?)?,
            target: InterfaceTypeId::new(read_u32(reader)?),
        }),
        9 => Ok(InterfaceType::TraitView(InterfaceTraitApplicationId::new(
            read_u32(reader)?,
        ))),
        10 => Ok(InterfaceType::OwnedIndirection {
            storage: InterfaceTypeId::new(read_u32(reader)?),
            target: InterfaceTypeId::new(read_u32(reader)?),
        }),
        11 => decode_callable_type(reader, limits),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_callable_type(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceType, InterfaceValidationError> {
    let parameter_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut parameters = Vec::with_capacity(parameter_count);

    for _ in 0..parameter_count {
        parameters.push(InterfaceCallableParameter::new(
            read_string(reader, limits)?,
            decode_tag(read_u32(reader)?)?,
            decode_tag(read_u32(reader)?)?,
            InterfaceTypeId::new(read_u32(reader)?),
        ));
    }

    Ok(InterfaceType::Callable {
        parameters: parameters.into(),
        result: InterfaceTypeId::new(read_u32(reader)?),
        constness: decode_tag(read_u32(reader)?)?,
        execution: decode_tag(read_u32(reader)?)?,
        trust: decode_tag(read_u32(reader)?)?,
        abi: decode_tag(read_u32(reader)?)?,
        dependency_contract: InterfaceDependencyContractId::new(read_u32(reader)?),
    })
}

pub(super) fn decode_constants(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let value_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let term_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(section, [value_count, term_count])?;

    let mut values = Vec::with_capacity(value_count);

    for _ in 0..value_count {
        values.push(InterfaceConstantValue::new(
            InterfaceTypeId::new(read_u32(&mut reader)?),
            decode_constant_value(&mut reader, limits, context)?,
        ));
    }

    let mut terms = Vec::with_capacity(term_count);

    for _ in 0..term_count {
        terms.push(decode_constant_term(&mut reader, limits, context)?);
    }

    reader.finish().map_err(map_wire_error)?;

    facts.constant_values = values.into();
    facts.constant_terms = terms.into();

    Ok(())
}

pub(super) fn decode_constant_value(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceConstantValueKind, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => match read_u32(reader)? {
            0 => Ok(InterfaceConstantValueKind::Boolean(false)),
            1 => Ok(InterfaceConstantValueKind::Boolean(true)),
            _ => Err(InterfaceValidationError::Malformed),
        },
        2 => char::from_u32(read_u32(reader)?)
            .map(InterfaceConstantValueKind::Character)
            .ok_or(InterfaceValidationError::Malformed),
        3 => {
            let sign = match read_u32(reader)? {
                1 => IntegerSign::NonNegative,
                2 => IntegerSign::Negative,
                _ => return Err(InterfaceValidationError::Malformed),
            };
            let length = read_count(reader, limits, InterfaceLimit::BlobLength)?;
            let magnitude = reader.read_bytes(length).map_err(map_wire_error)?;
            let integer = IntegerConstant::new(sign, magnitude.iter().copied());

            if integer.sign() != sign || integer.magnitude() != magnitude {
                return Err(InterfaceValidationError::Malformed);
            }

            Ok(InterfaceConstantValueKind::Integer(integer))
        }
        4 => Ok(InterfaceConstantValueKind::Real(decode_real(reader)?)),
        5 => Ok(InterfaceConstantValueKind::Complex {
            real: decode_real(reader)?,
            imaginary: decode_real(reader)?,
        }),
        6 => Ok(InterfaceConstantValueKind::String(read_string(
            reader, limits,
        )?)),
        7 => Ok(InterfaceConstantValueKind::Unit),
        8 => Ok(InterfaceConstantValueKind::NullableAbsent),
        9 => Ok(InterfaceConstantValueKind::NullablePresent(
            InterfaceConstantValueId::new(read_u32(reader)?),
        )),
        10 => Ok(InterfaceConstantValueKind::Tuple(read_ids(
            reader,
            limits,
            InterfaceConstantValueId::new,
        )?)),
        11 => Ok(InterfaceConstantValueKind::Array(read_ids(
            reader,
            limits,
            InterfaceConstantValueId::new,
        )?)),
        12 => Ok(InterfaceConstantValueKind::Product(read_ids(
            reader,
            limits,
            InterfaceConstantValueId::new,
        )?)),
        13 => Ok(InterfaceConstantValueKind::Union {
            variant: read_symbol_reference(reader, context)?,
            fields: read_ids(reader, limits, InterfaceConstantValueId::new)?,
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_constant_term(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceConstantTerm, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceConstantTerm::Value(InterfaceConstantValueId::new(
            read_u32(reader)?,
        ))),
        2 => Ok(InterfaceConstantTerm::Parameter(read_symbol_reference(
            reader, context,
        )?)),
        3 => Ok(InterfaceConstantTerm::TargetFact(read_symbol_reference(
            reader, context,
        )?)),
        4 => Ok(InterfaceConstantTerm::Unary {
            operation: decode_tag(read_u32(reader)?)?,
            operand: InterfaceConstantTermId::new(read_u32(reader)?),
        }),
        5 => Ok(InterfaceConstantTerm::Binary {
            operation: decode_tag(read_u32(reader)?)?,
            left: InterfaceConstantTermId::new(read_u32(reader)?),
            right: InterfaceConstantTermId::new(read_u32(reader)?),
        }),
        6 => Ok(InterfaceConstantTerm::DefinitionApplication {
            definition: read_symbol_reference(reader, context)?,
            substitution: InterfaceGenericSubstitutionId::new(read_u32(reader)?),
            selected_implementation: read_optional_u32(reader)?
                .map(InterfaceImplementationInstanceId::new),
        }),
        7 => Ok(InterfaceConstantTerm::Call {
            callable: InterfaceCallableInstanceId::new(read_u32(reader)?),
            arguments: read_ids(reader, limits, InterfaceConstantTermId::new)?,
        }),
        8 => Ok(InterfaceConstantTerm::Projection {
            subject: InterfaceConstantTermId::new(read_u32(reader)?),
            kind: decode_constant_projection(reader, context)?,
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_constant_projection(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceConstantProjection, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceConstantProjection::TupleElement(
            SymbolOrdinal::new(read_u32(reader)?),
        )),
        2 => Ok(InterfaceConstantProjection::ArrayElement(
            InterfaceConstantTermId::new(read_u32(reader)?),
        )),
        3 => Ok(InterfaceConstantProjection::ProductField(
            read_symbol_reference(reader, context)?,
        )),
        4 => Ok(InterfaceConstantProjection::UnionPayloadField(
            read_symbol_reference(reader, context)?,
        )),
        5 => Ok(InterfaceConstantProjection::NullableValue),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        AnySymbolId, CallableAbi, ConstantSymbolId, ExternalSymbolKey, FunctionSymbolId,
        PackageIdentity, SemanticValueStore, StructSymbolId, SymbolId, SymbolKind, SymbolName,
    };

    use super::super::decode_semantic_facts;
    use crate::semantic::codec::encode_semantic_facts;
    use crate::{
        DependencyInterfaceId, InterfaceAbiDependency, InterfaceCallableContract,
        InterfaceConstantTerm, InterfaceConstantValue, InterfaceConstantValueId,
        InterfaceConstantValueKind, InterfaceConstraint, InterfaceDependencyContract,
        InterfaceDependencyContractId, InterfaceGenericSubstitution,
        InterfaceGenericSubstitutionId, InterfacePredicateSummary, InterfaceSemanticFacts,
        InterfaceSourceProvenance, InterfaceSymbolReference, InterfaceSymbolResolver,
        InterfaceTargetFactDependency, InterfaceType, InterfaceTypeId, InterfaceValidationError,
        InterfaceValidationLimits, ValidatedInterfaceSection,
    };

    struct Resolver {
        symbols: Vec<AnySymbolId>,
    }

    impl InterfaceSymbolResolver for Resolver {
        fn resolve(&self, reference: &InterfaceSymbolReference) -> Option<AnySymbolId> {
            let InterfaceSymbolReference::Local(id) = reference else {
                return None;
            };

            self.symbols.get(id.to_index()?).copied()
        }
    }

    #[test]
    fn semantic_sections_round_trip_and_intern_without_persisting_local_value_ids() {
        let facts = facts();
        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts, 3, 0, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));
        let views: Vec<_> = sections
            .iter()
            .map(|section| {
                ValidatedInterfaceSection::for_test(
                    section.tag(),
                    section.record_count(),
                    section.payload(),
                )
            })
            .collect();

        let decoded = decode_semantic_facts(&views, 3, 0, limits)
            .unwrap_or_else(|error| panic!("semantic decoding failed: {error:?}"));

        assert_eq!(decoded, facts);

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));
        let resolver = resolver();
        let imported = decoded
            .intern(&store, &resolver)
            .unwrap_or_else(|error| panic!("semantic interning failed: {error:?}"));

        assert_eq!(imported.types().len(), 1);
        assert_eq!(imported.constant_values().len(), 1);
        assert_eq!(imported.constant_terms().len(), 1);
        assert_eq!(imported.dependency_contracts().len(), 1);
        assert_eq!(imported.constraints().len(), 1);
        assert_eq!(imported.callable_contracts().len(), 1);
    }

    #[test]
    fn semantic_decoding_rejects_unknown_tags_and_declared_count_mismatches() {
        let limits = InterfaceValidationLimits::default();
        let sections = encode_semantic_facts(&facts(), 3, 0, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));
        let mut owned: Vec<_> = sections
            .iter()
            .map(|section| {
                (
                    section.tag(),
                    section.record_count(),
                    section.payload().to_vec(),
                )
            })
            .collect();

        let Some(types_index) = owned
            .iter_mut()
            .position(|(tag, _, _)| *tag == crate::InterfaceSectionTag::SemanticTypes)
        else {
            panic!("semantic type section must be encoded");
        };

        owned[types_index].2[32..36].copy_from_slice(&99_u32.to_le_bytes());

        assert_eq!(
            decode_owned(&owned, limits),
            Err(InterfaceValidationError::Malformed)
        );

        owned[types_index].2[32..36].copy_from_slice(&1_u32.to_le_bytes());
        owned[types_index].1 += 1;

        assert_eq!(
            decode_owned(&owned, limits),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn semantic_strings_and_graph_depth_obey_loader_limits() {
        let limits = InterfaceValidationLimits::default().with_string_length(3);

        assert_eq!(
            encode_semantic_facts(&facts(), 3, 0, limits),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::StringLength,
                actual: 11,
                maximum: 3,
            })
        );
    }

    #[test]
    fn dependency_symbol_references_round_trip_with_stable_external_keys() {
        let limits = InterfaceValidationLimits::default();
        let mut facts = facts();
        let package = PackageIdentity::try_new("dependency.package")
            .unwrap_or_else(|| panic!("dependency package identity must be valid"));
        let owner = ExternalSymbolKey::package(package);
        let name = SymbolName::try_new("pointer_width")
            .unwrap_or_else(|| panic!("target fact name must be valid"));
        let key = ExternalSymbolKey::named(owner, SymbolKind::Constant, name)
            .unwrap_or_else(|| panic!("constant external key must be valid"));
        let dependency_fact = InterfaceSymbolReference::Dependency {
            dependency: DependencyInterfaceId::new(0),
            key,
        };
        let abi_dependencies = facts.abi_dependencies().to_vec();

        facts = facts.with_target_dependencies(
            [InterfaceTargetFactDependency::new(
                dependency_fact,
                InterfaceConstantValueId::new(0),
            )],
            abi_dependencies,
        );

        let sections = encode_semantic_facts(&facts, 3, 1, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));
        let views: Vec<_> = sections
            .iter()
            .map(|section| {
                ValidatedInterfaceSection::for_test(
                    section.tag(),
                    section.record_count(),
                    section.payload(),
                )
            })
            .collect();

        assert_eq!(decode_semantic_facts(&views, 3, 1, limits), Ok(facts));

        let constrained_limits = limits.with_external_reference_count(2);

        assert_eq!(
            decode_semantic_facts(&views, 3, 1, constrained_limits),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::ExternalReferenceCount,
                actual: 4,
                maximum: 2,
            })
        );
    }

    #[test]
    fn noncanonical_surface_fact_order_is_rejected_before_encoding() {
        let base = facts();
        let constraint = base.constraints()[0].clone();
        let facts = base.clone().with_contracts(
            [constraint.clone(), constraint],
            base.callable_contracts().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(&facts, 3, 0, InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    fn facts() -> InterfaceSemanticFacts {
        let struct_reference = local(0);
        let constant_reference = local(1);
        let function_reference = local(2);
        let provenance =
            InterfaceSourceProvenance::try_new(function_reference.clone(), "source.bray", 4, 12)
                .unwrap_or_else(|| panic!("ordered source provenance must be valid"));

        InterfaceSemanticFacts::new()
            .with_applications(
                [InterfaceGenericSubstitution::new(
                    struct_reference.clone(),
                    [],
                )],
                [],
                [],
                [],
            )
            .with_values(
                [InterfaceDependencyContract::new([])],
                [InterfaceType::Named {
                    definition: struct_reference.clone(),
                    substitution: InterfaceGenericSubstitutionId::new(0),
                }],
                [InterfaceConstantValue::new(
                    InterfaceTypeId::new(0),
                    InterfaceConstantValueKind::Boolean(true),
                )],
                [InterfaceConstantTerm::Value(InterfaceConstantValueId::new(
                    0,
                ))],
            )
            .with_contracts(
                [InterfaceConstraint::new(
                    struct_reference,
                    bray_symbols::SymbolOrdinal::new(0),
                    InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0)),
                )],
                [InterfaceCallableContract::new(
                    function_reference.clone(),
                    [],
                    [],
                    InterfaceDependencyContractId::new(0),
                )],
            )
            .with_target_dependencies(
                [InterfaceTargetFactDependency::new(
                    constant_reference,
                    InterfaceConstantValueId::new(0),
                )],
                [InterfaceAbiDependency::new(
                    function_reference,
                    CallableAbi::Bray,
                )],
            )
            .with_provenance([provenance])
    }

    fn resolver() -> Resolver {
        Resolver {
            symbols: vec![
                StructSymbolId::from_symbol_id(SymbolId::new(0)).into(),
                ConstantSymbolId::from_symbol_id(SymbolId::new(1)).into(),
                FunctionSymbolId::from_symbol_id(SymbolId::new(2)).into(),
            ],
        }
    }

    fn local(raw: u32) -> InterfaceSymbolReference {
        InterfaceSymbolReference::Local(bray_symbols::InterfaceSymbolId::new(raw))
    }

    fn decode_owned(
        owned: &[(crate::InterfaceSectionTag, u64, Vec<u8>)],
        limits: InterfaceValidationLimits,
    ) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
        let views: Vec<_> = owned
            .iter()
            .map(|(tag, count, payload)| ValidatedInterfaceSection::for_test(*tag, *count, payload))
            .collect();

        decode_semantic_facts(&views, 3, 0, limits)
    }
}
