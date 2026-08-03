use std::sync::Arc;

use super::common::{decode_real, decode_tag, read_ids, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_optional_u32, read_string,
    read_symbol_reference, read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceGenericArgument, InterfaceGenericBinding, InterfaceGenericSubstitution,
    InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceSemanticFacts, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceType, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use bray_symbols::{ConstantField, IntegerConstant, IntegerSign, SymbolOrdinal};

pub(super) struct TypeRecordTables<'bytes> {
    pub(super) substitutions: RecordTable<'bytes>,
    pub(super) trait_applications: RecordTable<'bytes>,
    pub(super) callable_instances: RecordTable<'bytes>,
    pub(super) implementation_instances: RecordTable<'bytes>,
    pub(super) types: RecordTable<'bytes>,
}

pub(super) fn decode_type_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<TypeRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());

    let substitutions = RecordTable::read_from(&mut reader, context)?;
    let trait_applications = RecordTable::read_from(&mut reader, context)?;
    let callable_instances = RecordTable::read_from(&mut reader, context)?;
    let implementation_instances = RecordTable::read_from(&mut reader, context)?;
    let types = RecordTable::read_from(&mut reader, context)?;

    validate_record_count(
        section,
        [
            substitutions.len(),
            trait_applications.len(),
            callable_instances.len(),
            implementation_instances.len(),
            types.len(),
        ],
    )?;

    reader.finish().map_err(map_wire_error)?;

    Ok(TypeRecordTables {
        substitutions,
        trait_applications,
        callable_instances,
        implementation_instances,
        types,
    })
}

pub(super) fn decode_types(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let tables = decode_type_tables(section, context)?;

    let substitutions = tables
        .substitutions
        .decode_all(context, |reader, context| {
            decode_substitution(reader, limits, context)
        })?;

    let trait_applications = tables
        .trait_applications
        .decode_all(context, decode_trait_application)?;

    let callable_instances = tables
        .callable_instances
        .decode_all(context, decode_callable_instance)?;

    let implementation_instances = tables
        .implementation_instances
        .decode_all(context, decode_implementation_instance)?;

    let types = tables.types.decode_all(context, |reader, context| {
        decode_type(reader, limits, context)
    })?;

    Ok(InterfaceSemanticFacts::new()
        .with_applications(
            substitutions,
            trait_applications,
            callable_instances,
            implementation_instances,
        )
        .with_values([], types, [], []))
}

pub(super) fn decode_substitution(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceGenericSubstitution, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let binding_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut bindings = context.allocate_items(reader, binding_count)?;

    for _ in 0..binding_count {
        let parameter = read_symbol_reference(reader, context)?;

        let argument = match read_u32(reader)? {
            1 => InterfaceGenericArgument::Type(InterfaceTypeId::new(read_u32(reader)?)),
            2 => {
                InterfaceGenericArgument::Constant(InterfaceConstantTermId::new(read_u32(reader)?))
            }
            _ => return Err(InterfaceValidationError::Malformed),
        };

        bindings.push(InterfaceGenericBinding::new(parameter, argument));
    }

    Ok(InterfaceGenericSubstitution::new(owner, bindings))
}

pub(super) fn decode_trait_application(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceTraitApplication, InterfaceValidationError> {
    Ok(InterfaceTraitApplication::new(
        read_symbol_reference(reader, context)?,
        InterfaceGenericSubstitutionId::new(read_u32(reader)?),
    ))
}

pub(super) fn decode_callable_instance(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallableInstance, InterfaceValidationError> {
    Ok(InterfaceCallableInstance::new(
        read_symbol_reference(reader, context)?,
        InterfaceGenericSubstitutionId::new(read_u32(reader)?),
    ))
}

pub(super) fn decode_implementation_instance(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceImplementationInstance, InterfaceValidationError> {
    Ok(InterfaceImplementationInstance::new(
        read_symbol_reference(reader, context)?,
        InterfaceGenericSubstitutionId::new(read_u32(reader)?),
    ))
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
        3 => Ok(InterfaceType::TypeValuedMemberProjection {
            subject: InterfaceTypeId::new(read_u32(reader)?),
            application: InterfaceTraitApplicationId::new(read_u32(reader)?),
            member: read_symbol_reference(reader, context)?,
        }),
        4 => Ok(InterfaceType::Tuple(read_ids(
            reader,
            context,
            InterfaceTypeId::new,
        )?)),
        5 => Ok(InterfaceType::Array {
            element: InterfaceTypeId::new(read_u32(reader)?),
            length: InterfaceConstantTermId::new(read_u32(reader)?),
        }),
        6 => Ok(InterfaceType::Slice(InterfaceTypeId::new(read_u32(
            reader,
        )?))),
        13 => Ok(InterfaceType::Generator(InterfaceTypeId::new(read_u32(
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
        11 => decode_callable_type(reader, limits, context),
        12 => Ok(InterfaceType::ContextualSelf(read_symbol_reference(
            reader, context,
        )?)),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_callable_type(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceType, InterfaceValidationError> {
    let parameter_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;

    let mut parameters = context.allocate_items(reader, parameter_count)?;

    for _ in 0..parameter_count {
        parameters.push(InterfaceCallableParameter::new(
            read_string(reader, context)?,
            decode_tag(read_u32(reader)?)?,
            decode_tag(read_u32(reader)?)?,
            InterfaceTypeId::new(read_u32(reader)?),
        ));
    }

    Ok(InterfaceType::Callable {
        parameters: parameters.into(),
        result: InterfaceTypeId::new(read_u32(reader)?),
        constness: decode_tag(read_u32(reader)?)?,
        trust: decode_tag(read_u32(reader)?)?,
        abi: decode_tag(read_u32(reader)?)?,
        invocation_behavior: super::contract::decode_callable_behavior(reader, limits, context)?,
        deferred_execution_behavior: match read_u32(reader)? {
            0 => None,
            1 => Some(super::contract::decode_callable_behavior(
                reader, limits, context,
            )?),
            _ => return Err(InterfaceValidationError::Malformed),
        },
    })
}

pub(super) struct ConstantRecordTables<'bytes> {
    pub(super) values: RecordTable<'bytes>,
    pub(super) terms: RecordTable<'bytes>,
}

pub(super) fn decode_constant_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<ConstantRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());

    let values = RecordTable::read_from(&mut reader, context)?;
    let terms = RecordTable::read_from(&mut reader, context)?;

    validate_record_count(section, [values.len(), terms.len()])?;

    reader.finish().map_err(map_wire_error)?;

    Ok(ConstantRecordTables { values, terms })
}

pub(super) fn decode_constants(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_constant_tables(section, context)?;

    let values = tables.values.decode_all(context, |reader, context| {
        decode_constant_value_record(reader, limits, context)
    })?;

    let terms = tables.terms.decode_all(context, decode_constant_term)?;

    facts.constant_values = values.into();
    facts.constant_terms = terms.into();

    Ok(())
}

pub(super) fn decode_constant_value_record(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceConstantValue, InterfaceValidationError> {
    Ok(InterfaceConstantValue::new(
        InterfaceTypeId::new(read_u32(reader)?),
        decode_constant_value(reader, limits, context)?,
    ))
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
        3 => decode_integer(reader, limits).map(InterfaceConstantValueKind::Integer),
        4 => Ok(InterfaceConstantValueKind::Real(decode_real(reader)?)),
        5 => Ok(InterfaceConstantValueKind::Complex {
            real: decode_real(reader)?,
            imaginary: decode_real(reader)?,
        }),
        6 => Ok(InterfaceConstantValueKind::String(read_string(
            reader, context,
        )?)),
        7 => Ok(InterfaceConstantValueKind::Unit),
        8 => Ok(InterfaceConstantValueKind::NullableAbsent),
        9 => Ok(InterfaceConstantValueKind::NullablePresent(
            InterfaceConstantValueId::new(read_u32(reader)?),
        )),
        10 => Ok(InterfaceConstantValueKind::Tuple(read_ids(
            reader,
            context,
            InterfaceConstantValueId::new,
        )?)),
        11 => Ok(InterfaceConstantValueKind::Array(read_ids(
            reader,
            context,
            InterfaceConstantValueId::new,
        )?)),
        12 => Ok(InterfaceConstantValueKind::Product(read_constant_fields(
            reader,
            context,
            InterfaceConstantValueId::new,
        )?)),
        13 => Ok(InterfaceConstantValueKind::Union {
            variant: read_symbol_reference(reader, context)?,
            fields: read_constant_fields(reader, context, InterfaceConstantValueId::new)?,
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_constant_term(
    reader: &mut WireReader<'_>,
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
            selected_implementation: read_optional_u32(reader)?
                .map(InterfaceImplementationInstanceId::new),
            arguments: read_ids(reader, context, InterfaceConstantTermId::new)?,
        }),
        8 => Ok(InterfaceConstantTerm::Projection {
            subject: InterfaceConstantTermId::new(read_u32(reader)?),
            kind: decode_constant_projection(reader, context)?,
        }),
        9 => Ok(InterfaceConstantTerm::IntegerLiteral {
            ty: decode_tag(read_u32(reader)?)?,
            value: decode_integer(reader, context.limits())?,
        }),
        10 => Ok(InterfaceConstantTerm::Conversion {
            operand: InterfaceConstantTermId::new(read_u32(reader)?),
            target: InterfaceTypeId::new(read_u32(reader)?),
        }),
        11 => Ok(InterfaceConstantTerm::NullablePresent(
            InterfaceConstantTermId::new(read_u32(reader)?),
        )),
        12 => Ok(InterfaceConstantTerm::Tuple(read_ids(
            reader,
            context,
            InterfaceConstantTermId::new,
        )?)),
        13 => Ok(InterfaceConstantTerm::Array(read_ids(
            reader,
            context,
            InterfaceConstantTermId::new,
        )?)),
        14 => Ok(InterfaceConstantTerm::Product(read_constant_fields(
            reader,
            context,
            InterfaceConstantTermId::new,
        )?)),
        15 => Ok(InterfaceConstantTerm::Union {
            variant: read_symbol_reference(reader, context)?,
            fields: read_constant_fields(reader, context, InterfaceConstantTermId::new)?,
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn read_constant_fields<V>(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
    value: impl Fn(u32) -> V,
) -> Result<Arc<[ConstantField<crate::InterfaceSymbolReference, V>]>, InterfaceValidationError> {
    let count = read_count(reader, context.limits(), InterfaceLimit::RecordCount)?;
    let mut fields = context.allocate_items(reader, count)?;

    for _ in 0..count {
        fields.push(ConstantField::new(
            read_symbol_reference(reader, context)?,
            value(read_u32(reader)?),
        ));
    }

    Ok(fields.into())
}

pub(super) fn decode_integer(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<IntegerConstant, InterfaceValidationError> {
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

    Ok(integer)
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
        AnySymbolId, CallableAbi, CallableConstness, CallableExecution, CallableParameterMode,
        CallablePosition, CallableTrust, ConstantField, ConstantSymbolId, ConstantTermData,
        ConstantValueKind, ExternalSymbolKey, FunctionSymbolId, InherentImplementationSymbolId,
        IntegerConstant, IntegerSign, ModuleSymbolId, NamedTypeSymbolId, PackageIdentity,
        PackageSymbolId, SelfTypeContext, SemanticValueStore, StructFieldSymbolId, StructSymbolId,
        SymbolId, SymbolKind, SymbolName, SymbolOrdinal, TargetSizedIntegerType, TraitSymbolId,
        TypeData, UnionPayloadFieldSymbolId, UnionSymbolId, UnionVariantSymbolId,
    };

    use super::super::decode_semantic_facts;
    use super::super::test_support::{
        OwnedSection, encoded_section_views, interface_surface, local_by_kind as symbol_reference,
        owned_section_views, owned_sections, record_range,
    };
    use crate::semantic::codec::encode_semantic_facts;
    use crate::test_support::{module_key as test_module_key, named_key};
    use crate::{
        DependencyInterfaceId, InterfaceAbiDependency, InterfaceCallableContract,
        InterfaceCallableParameter, InterfaceConstantTerm, InterfaceConstantValue,
        InterfaceConstantValueId, InterfaceConstantValueKind, InterfaceConstraint,
        InterfaceDependencyContract, InterfaceDependencyContractId, InterfaceGenericSubstitution,
        InterfaceGenericSubstitutionId, InterfaceImplementationRecord, InterfacePredicateSummary,
        InterfaceSemanticFacts, InterfaceSemanticInternError, InterfaceSourceProvenance,
        InterfaceSymbolReference, InterfaceSymbolResolver, InterfaceTargetFactDependency,
        InterfaceTraitApplication, InterfaceTraitApplicationId, InterfaceType, InterfaceTypeId,
        InterfaceValidationError, InterfaceValidationLimits,
    };

    struct Resolver {
        symbols: Vec<AnySymbolId>,
        keys: Vec<ExternalSymbolKey>,
    }

    impl InterfaceSymbolResolver for Resolver {
        fn resolve(&self, reference: &InterfaceSymbolReference) -> Option<AnySymbolId> {
            let InterfaceSymbolReference::Local(id) = reference else {
                return None;
            };

            self.symbols.get(id.to_index()?).copied()
        }

        fn external_key(&self, reference: &InterfaceSymbolReference) -> Option<ExternalSymbolKey> {
            let InterfaceSymbolReference::Local(id) = reference else {
                return None;
            };

            self.keys.get(id.to_index()?).cloned()
        }
    }

    #[test]
    fn semantic_sections_round_trip_and_intern_without_persisting_local_value_ids() {
        let (surface, facts) = fixture();

        let limits = InterfaceValidationLimits::default();

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));

        let views = encoded_section_views(&sections);

        let decoded = decode_semantic_facts(&views, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic decoding failed: {error:?}"));

        assert_eq!(decoded, facts);

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let resolver = resolver(&surface);

        let imported = decoded
            .intern(&store, &resolver)
            .unwrap_or_else(|error| panic!("semantic interning failed: {error:?}"));

        assert_eq!(imported.types().len(), 4);
        assert_eq!(imported.constant_values().len(), 1);
        assert_eq!(imported.constant_terms().len(), 1);
        assert_eq!(imported.dependency_contracts().len(), 1);
        assert_eq!(imported.constraints().len(), 1);
        assert_eq!(imported.callable_contracts().len(), 1);

        let callable_contract = imported.callable_contracts()[0].contract();

        assert_eq!(callable_contract.invocation_preconditions().len(), 1);
        assert_eq!(callable_contract.static_constraints().len(), 1);

        assert_eq!(
            callable_contract.normal_completion_postconditions().len(),
            1
        );

        assert_eq!(
            callable_contract
                .deferred_execution_behavior()
                .map(|behavior| behavior.current_run_cancellation()),
            Some(bray_symbols::CurrentRunCancellation::MayEnter)
        );

        let contextual = match store.type_data(imported.types()[1]) {
            Ok(contextual) => contextual,
            Err(error) => panic!("contextual Self type must be interned: {error:?}"),
        };

        assert_eq!(
            contextual.as_ref(),
            &TypeData::ContextualSelf(SelfTypeContext::NamedType(NamedTypeSymbolId::Struct(
                StructSymbolId::from_symbol_id(symbol_id(&surface, SymbolKind::Struct))
            )))
        );

        let generator = match store.type_data(imported.types()[2]) {
            Ok(generator) => generator,
            Err(error) => panic!("generator type must be interned: {error:?}"),
        };

        assert_eq!(
            generator.as_ref(),
            &TypeData::Generator(imported.types()[0])
        );

        let callable = match store.type_data(imported.types()[3]) {
            Ok(callable) => callable,
            Err(error) => panic!("callable type must be interned: {error:?}"),
        };

        let TypeData::Callable(callable) = callable.as_ref() else {
            panic!("fourth imported type must be callable");
        };

        assert_eq!(callable.execution(), CallableExecution::Asynchronous);

        assert!(
            callable
                .dependency_contracts()
                .deferred_execution()
                .is_some()
        );

        let deferred = callable
            .phase_behaviors()
            .deferred_execution()
            .unwrap_or_else(|| panic!("async callable type must retain deferred behavior"));

        assert_eq!(
            deferred.current_run_cancellation(),
            bray_symbols::CurrentRunCancellation::MayEnter
        );

        assert_eq!(
            deferred.lifecycle_obligations(),
            [bray_symbols::LifecycleObligationKind::Finalization]
        );

        assert_eq!(deferred.execution_requirements().len(), 1);
    }

    #[test]
    fn target_sized_integer_terms_reject_invalid_types_and_round_trip() {
        let surface = interface_surface(package_identity(), [], []);

        let facts = InterfaceSemanticFacts::new().with_values(
            [],
            [],
            [],
            [InterfaceConstantTerm::IntegerLiteral {
                ty: TargetSizedIntegerType::Usize,
                value: IntegerConstant::new(IntegerSign::NonNegative, [4]),
            }],
        );

        let limits = InterfaceValidationLimits::default();

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));

        let views = encoded_section_views(&sections);

        let decoded = decode_semantic_facts(&views, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic decoding failed: {error:?}"));

        assert_eq!(decoded, facts);

        let mut malformed = owned_sections(&sections);

        let Some((_, _, constants)) = malformed
            .iter_mut()
            .find(|(tag, _, _)| *tag == crate::InterfaceSectionTag::Constants)
        else {
            panic!("constant section must be encoded");
        };

        constants[12..16].copy_from_slice(&3_u32.to_le_bytes());

        assert_eq!(
            decode_owned(&malformed, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let imported = decoded
            .intern(&store, &resolver(&surface))
            .unwrap_or_else(|error| panic!("semantic interning failed: {error:?}"));

        let term = store
            .constant_term_data(imported.constant_terms()[0])
            .unwrap_or_else(|error| panic!("constant term must be interned: {error:?}"));

        assert!(matches!(
            term.as_ref(),
            ConstantTermData::IntegerLiteral {
                ty: TargetSizedIntegerType::Usize,
                value,
            } if value.magnitude() == [4]
        ));
    }

    #[test]
    fn scalar_conversion_terms_round_trip_and_intern() {
        let surface = interface_surface(package_identity(), [], []);

        let facts = InterfaceSemanticFacts::new().with_values(
            [],
            [InterfaceType::Tuple([].into())],
            [InterfaceConstantValue::new(
                InterfaceTypeId::new(0),
                InterfaceConstantValueKind::Unit,
            )],
            [
                InterfaceConstantTerm::Value(InterfaceConstantValueId::new(0)),
                InterfaceConstantTerm::Conversion {
                    operand: crate::InterfaceConstantTermId::new(0),
                    target: InterfaceTypeId::new(0),
                },
            ],
        );

        let limits = InterfaceValidationLimits::default();

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));

        let views = encoded_section_views(&sections);

        let decoded = decode_semantic_facts(&views, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic decoding failed: {error:?}"));

        assert_eq!(decoded, facts);

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let imported = decoded
            .intern(&store, &resolver(&surface))
            .unwrap_or_else(|error| panic!("semantic interning failed: {error:?}"));

        let term = store
            .constant_term_data(imported.constant_terms()[1])
            .unwrap_or_else(|error| panic!("constant term must be interned: {error:?}"));

        assert!(matches!(
            term.as_ref(),
            ConstantTermData::Conversion { operand, target }
                if *operand == imported.constant_terms()[0] && *target == imported.types()[0]
        ));
    }

    #[test]
    fn aggregate_values_and_open_terms_round_trip_with_field_identities() {
        let surface = semantic_surface([]);
        let structure_field = symbol_reference(&surface, SymbolKind::StructField);
        let variant = symbol_reference(&surface, SymbolKind::UnionVariant);
        let payload_field = symbol_reference(&surface, SymbolKind::UnionPayloadField);

        let facts = InterfaceSemanticFacts::new().with_values(
            [],
            [InterfaceType::Tuple([].into())],
            [
                InterfaceConstantValue::new(
                    InterfaceTypeId::new(0),
                    InterfaceConstantValueKind::Unit,
                ),
                InterfaceConstantValue::new(
                    InterfaceTypeId::new(0),
                    InterfaceConstantValueKind::Product(
                        [ConstantField::new(
                            structure_field.clone(),
                            InterfaceConstantValueId::new(0),
                        )]
                        .into(),
                    ),
                ),
                InterfaceConstantValue::new(
                    InterfaceTypeId::new(0),
                    InterfaceConstantValueKind::Union {
                        variant: variant.clone(),
                        fields: [ConstantField::new(
                            payload_field.clone(),
                            InterfaceConstantValueId::new(0),
                        )]
                        .into(),
                    },
                ),
            ],
            [
                InterfaceConstantTerm::Value(InterfaceConstantValueId::new(0)),
                InterfaceConstantTerm::Product(
                    [ConstantField::new(
                        structure_field,
                        crate::InterfaceConstantTermId::new(0),
                    )]
                    .into(),
                ),
                InterfaceConstantTerm::Union {
                    variant,
                    fields: [ConstantField::new(
                        payload_field,
                        crate::InterfaceConstantTermId::new(0),
                    )]
                    .into(),
                },
            ],
        );

        let limits = InterfaceValidationLimits::default();

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("aggregate semantic encoding failed: {error:?}"));

        let views = encoded_section_views(&sections);

        let decoded = decode_semantic_facts(&views, &surface, limits)
            .unwrap_or_else(|error| panic!("aggregate semantic decoding failed: {error:?}"));

        assert_eq!(decoded, facts);

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let imported = decoded
            .intern(&store, &resolver(&surface))
            .unwrap_or_else(|error| panic!("aggregate semantic interning failed: {error:?}"));

        let product = store
            .constant_value_data(imported.constant_values()[1])
            .unwrap_or_else(|error| panic!("product constant must be interned: {error:?}"));

        let ConstantValueKind::Product(fields) = product.kind() else {
            panic!("second aggregate constant must remain a product");
        };

        assert!(matches!(
            fields.as_ref(),
            [field] if field.field().kind() == SymbolKind::StructField
                && *field.value() == imported.constant_values()[0]
        ));

        let union = store
            .constant_term_data(imported.constant_terms()[2])
            .unwrap_or_else(|error| panic!("union term must be interned: {error:?}"));

        assert!(matches!(
            union.as_ref(),
            ConstantTermData::Union { variant, fields }
                if variant.kind() == SymbolKind::UnionVariant
                    && matches!(
                        fields.as_ref(),
                        [field] if field.field().kind() == SymbolKind::UnionPayloadField
                            && *field.value() == imported.constant_terms()[0]
                    )
        ));
    }

    #[test]
    fn semantic_interning_rejects_wrong_symbol_categories() {
        let (surface, facts) = fixture();

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let mut resolver = resolver(&surface);

        let function = symbol_reference(&surface, SymbolKind::Function);

        let InterfaceSymbolReference::Local(function_id) = function else {
            panic!("test function must be local");
        };

        resolver.symbols[function_id
            .to_index()
            .unwrap_or_else(|| panic!("test function ID must fit the host index"))] =
            StructSymbolId::from_symbol_id(SymbolId::new(99)).into();

        assert_eq!(
            facts.intern(&store, &resolver),
            Err(InterfaceSemanticInternError::InvalidSymbolKind(function))
        );
    }

    #[test]
    fn semantic_interning_rejects_trait_applications_on_inherent_implementations() {
        let (surface, facts) = fixture();

        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store creation failed: {error:?}"));

        let resolver = resolver(&surface);

        let structure = symbol_reference(&surface, SymbolKind::Struct);
        let implementation = symbol_reference(&surface, SymbolKind::InherentImplementation);
        let trait_definition = symbol_reference(&surface, SymbolKind::Trait);

        let facts = facts
            .with_applications(
                [
                    InterfaceGenericSubstitution::new(structure, []),
                    InterfaceGenericSubstitution::new(trait_definition.clone(), []),
                ],
                [InterfaceTraitApplication::new(
                    trait_definition,
                    InterfaceGenericSubstitutionId::new(1),
                )],
                [],
                [],
            )
            .with_implementations(
                [InterfaceImplementationRecord::new(
                    implementation.clone(),
                    InterfaceTypeId::new(0),
                    Some(InterfaceTraitApplicationId::new(0)),
                )],
                [],
            );

        assert_eq!(
            facts.intern(&store, &resolver),
            Err(InterfaceSemanticInternError::InvalidSymbolKind(
                implementation
            ))
        );
    }

    #[test]
    fn semantic_decoding_rejects_unknown_tags_and_declared_count_mismatches() {
        let (surface, facts) = fixture();

        let limits = InterfaceValidationLimits::default();

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));

        let mut owned = owned_sections(&sections);

        let Some(types_index) = owned
            .iter_mut()
            .position(|(tag, _, _)| *tag == crate::InterfaceSectionTag::SemanticTypes)
        else {
            panic!("semantic type section must be encoded");
        };

        let type_range = record_range(&owned[types_index].2, 4, 0);

        owned[types_index].2[type_range.start..type_range.start + 4]
            .copy_from_slice(&99_u32.to_le_bytes());

        assert_eq!(
            decode_owned(&owned, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );

        owned[types_index].2[type_range.start..type_range.start + 4]
            .copy_from_slice(&1_u32.to_le_bytes());

        owned[types_index].1 += 1;

        assert_eq!(
            decode_owned(&owned, &surface, limits),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn semantic_strings_and_graph_depth_obey_loader_limits() {
        let (surface, facts) = fixture();

        let limits = InterfaceValidationLimits::default().with_string_length(3);

        assert_eq!(
            encode_semantic_facts(&facts, &surface, limits),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::StringLength,
                actual: 11,
                maximum: 3,
            })
        );
    }

    #[test]
    fn cyclic_structural_type_graphs_are_rejected() {
        let surface = interface_surface(package_identity(), [], []);

        let facts = InterfaceSemanticFacts::new().with_values(
            [],
            [InterfaceType::Nullable(InterfaceTypeId::new(0))],
            [],
            [],
        );

        assert_eq!(
            encode_semantic_facts(&facts, &surface, InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn dependency_symbol_references_round_trip_with_stable_external_keys() {
        let limits = InterfaceValidationLimits::default();

        let package = PackageIdentity::try_new("dependency.package")
            .unwrap_or_else(|| panic!("dependency package identity must be valid"));

        let surface = semantic_surface([package.clone()]);
        let mut facts = facts(&surface);

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
                symbol_reference(&surface, SymbolKind::Function),
                dependency_fact,
                InterfaceConstantValueId::new(0),
            )],
            abi_dependencies,
        );

        let sections = encode_semantic_facts(&facts, &surface, limits)
            .unwrap_or_else(|error| panic!("semantic encoding failed: {error:?}"));

        let views = encoded_section_views(&sections);

        assert_eq!(decode_semantic_facts(&views, &surface, limits), Ok(facts));

        let constrained_limits = limits.with_external_reference_count(1);

        assert_eq!(
            decode_semantic_facts(&views, &surface, constrained_limits),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::ExternalReferenceCount,
                actual: 2,
                maximum: 1,
            })
        );
    }

    #[test]
    fn noncanonical_surface_fact_order_is_rejected_before_encoding() {
        let (surface, base) = fixture();

        let constraint = base.constraints()[0].clone();

        let facts = base.clone().with_contracts(
            [constraint.clone(), constraint],
            base.callable_contracts().iter().cloned(),
        );

        assert_eq!(
            encode_semantic_facts(&facts, &surface, InterfaceValidationLimits::default()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    fn fixture() -> (crate::PackageInterfaceSurface, InterfaceSemanticFacts) {
        let surface = semantic_surface([]);
        let facts = facts(&surface);

        (surface, facts)
    }

    fn facts(surface: &crate::PackageInterfaceSurface) -> InterfaceSemanticFacts {
        let struct_reference = symbol_reference(surface, SymbolKind::Struct);
        let constant_reference = symbol_reference(surface, SymbolKind::Constant);
        let function_reference = symbol_reference(surface, SymbolKind::Function);

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
                [
                    InterfaceType::Named {
                        definition: struct_reference.clone(),
                        substitution: InterfaceGenericSubstitutionId::new(0),
                    },
                    InterfaceType::ContextualSelf(struct_reference.clone()),
                    InterfaceType::Generator(InterfaceTypeId::new(0)),
                    InterfaceType::Callable {
                        parameters: [InterfaceCallableParameter::new(
                            "arg",
                            CallablePosition::PositionalOrNamed,
                            CallableParameterMode::Immutable,
                            InterfaceTypeId::new(0),
                        )]
                        .into(),
                        result: InterfaceTypeId::new(0),
                        constness: CallableConstness::Runtime,
                        trust: CallableTrust::Safe,
                        abi: CallableAbi::Bray,
                        invocation_behavior: crate::test_support::callable_phase_behavior(),
                        deferred_execution_behavior: Some(
                            crate::InterfaceCallablePhaseBehavior::new(
                                [],
                                [],
                                [],
                                [function_reference.clone()],
                                [bray_symbols::LifecycleObligationKind::Finalization],
                                InterfaceDependencyContractId::new(0),
                                bray_symbols::CurrentRunCancellation::MayEnter,
                            ),
                        ),
                    },
                ],
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
                    SymbolOrdinal::new(0),
                    InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0)),
                )],
                [InterfaceCallableContract::new(
                    function_reference.clone(),
                    [
                        crate::InterfaceCallableContractClause::new(
                            SymbolOrdinal::new(0),
                            bray_symbols::CallableContractClauseKind::Requires,
                            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0)),
                        ),
                        crate::InterfaceCallableContractClause::new(
                            SymbolOrdinal::new(1),
                            bray_symbols::CallableContractClauseKind::Ensures,
                            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0)),
                        ),
                        crate::InterfaceCallableContractClause::new(
                            SymbolOrdinal::new(2),
                            bray_symbols::CallableContractClauseKind::Static,
                            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0)),
                        ),
                    ],
                    crate::test_support::callable_phase_behavior(),
                    Some(crate::InterfaceCallablePhaseBehavior::new(
                        [],
                        [],
                        [],
                        [function_reference.clone()],
                        [bray_symbols::LifecycleObligationKind::Finalization],
                        InterfaceDependencyContractId::new(0),
                        bray_symbols::CurrentRunCancellation::MayEnter,
                    )),
                )],
            )
            .with_target_dependencies(
                [InterfaceTargetFactDependency::new(
                    function_reference.clone(),
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

    fn semantic_surface(
        dependencies: impl IntoIterator<Item = PackageIdentity>,
    ) -> crate::PackageInterfaceSurface {
        let module = test_module_key(package_identity(), "semantic");
        let structure = named_key(module.clone(), SymbolKind::Struct, "record");
        let union = named_key(module.clone(), SymbolKind::Union, "choice");
        let variant = named_key(union.clone(), SymbolKind::UnionVariant, "some");

        let symbols = [
            structure.clone(),
            named_key(structure, SymbolKind::StructField, "value"),
            union,
            variant.clone(),
            named_key(variant, SymbolKind::UnionPayloadField, "value"),
            named_key(module.clone(), SymbolKind::Constant, "answer"),
            named_key(module.clone(), SymbolKind::Function, "run"),
            named_key(module.clone(), SymbolKind::Trait, "contract"),
            ExternalSymbolKey::ordinal(
                module,
                SymbolKind::InherentImplementation,
                SymbolOrdinal::new(0),
            )
            .unwrap_or_else(|| panic!("test implementation key must be valid")),
        ];

        interface_surface(package_identity(), symbols, dependencies)
    }

    fn package_identity() -> PackageIdentity {
        PackageIdentity::try_new("example.semantic")
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }

    fn symbol_id(surface: &crate::PackageInterfaceSurface, kind: SymbolKind) -> SymbolId {
        let InterfaceSymbolReference::Local(id) = symbol_reference(surface, kind) else {
            panic!("test symbol must be local");
        };

        SymbolId::new(id.raw())
    }

    fn resolver(surface: &crate::PackageInterfaceSurface) -> Resolver {
        let keys = surface
            .symbols()
            .symbols()
            .iter()
            .map(|identity| identity.key().clone())
            .collect();

        let symbols = surface
            .symbols()
            .symbols()
            .iter()
            .map(|identity| {
                let id = SymbolId::new(identity.id().raw());

                match identity.kind() {
                    SymbolKind::Package => PackageSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Module => ModuleSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Struct => StructSymbolId::from_symbol_id(id).into(),
                    SymbolKind::StructField => StructFieldSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Union => UnionSymbolId::from_symbol_id(id).into(),
                    SymbolKind::UnionVariant => UnionVariantSymbolId::from_symbol_id(id).into(),
                    SymbolKind::UnionPayloadField => {
                        UnionPayloadFieldSymbolId::from_symbol_id(id).into()
                    }
                    SymbolKind::Constant => ConstantSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Function => FunctionSymbolId::from_symbol_id(id).into(),
                    SymbolKind::Trait => TraitSymbolId::from_symbol_id(id).into(),
                    SymbolKind::InherentImplementation => {
                        InherentImplementationSymbolId::from_symbol_id(id).into()
                    }
                    kind => panic!("unexpected test symbol kind: {kind:?}"),
                }
            })
            .collect();

        Resolver { symbols, keys }
    }

    fn decode_owned(
        owned: &[OwnedSection],
        surface: &crate::PackageInterfaceSurface,
        limits: InterfaceValidationLimits,
    ) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
        let views = owned_section_views(owned);

        decode_semantic_facts(&views, surface, limits)
    }
}
