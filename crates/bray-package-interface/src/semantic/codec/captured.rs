use std::sync::Arc;

use bray_symbols::{ CapturedValueOperation as Op, CapturedValueTemplate as Value};

use super::common::{SemanticDecodeContext, read_count, read_u32, write_count, read_optional_u32, write_optional_u32,
    read_string, write_string, read_symbol_reference, write_symbol_reference};
use crate::{InterfaceConstantTermId, InterfaceDependencyProjection,
    InterfaceLimit, InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
    InterfaceStaticReference, InterfaceGenericSubstitutionId, InterfaceImplementationInstanceId};
use crate::wire::{WireEncoder, WireReader};

type Binding = crate::InterfaceNormalOutcome;
type Template = Value<InterfaceConstantTermId, InterfaceDependencyProjection, InterfaceStaticReference, crate::InterfaceCapturedCallable, crate::InterfaceSymbolReference>;

pub(super) fn encode_values(encoder: &mut WireEncoder, bindings: &[Binding]) {
    write_count(encoder, bindings.len());

    for binding in bindings {
        super::encoding::contract::encode_optional_requirements(encoder, binding.report.as_deref());
        super::encoding::contract::encode_dependency_effects(encoder, &binding.effects);
        super::ownership::encode_owned_result(encoder, &binding.ownership);
        write_count(encoder, binding.carriers.len());

        for carrier in binding.carriers.iter() {
            encoder.write_u32(carrier.guarantee().raw());
            encoder.write_u32(u32::from(carrier.support().is_some()));

            if let Some(support) = carrier.support() {
                encode_support_owners(encoder, support.owners());
                write_count(encoder, support.addresses().len());

                for (address, owners) in support.addresses() {
                    encoder.write_u32(address.raw());
                    encode_support_owners(encoder, owners);
                }
            }

            super::encoding::contract::encode_dependency_projections(encoder, carrier.path());
        }

        super::encoding::contract::encode_dependency_storage(encoder, &binding.references, &binding.stored_values);
        write_count(encoder, binding.inactive_payloads.len());

        for payload in binding.inactive_payloads.iter() {
            super::encoding::contract::encode_dependency_subject(encoder, payload);
        }

        write_optional_u32(encoder, binding.guard.map(|term| term.raw()));
        write_count(encoder, binding.bindings.len());

        for (destination, value) in binding.bindings.iter() {
            super::encoding::contract::encode_dependency_subject(encoder, destination);
            encode_value(encoder, value);
        }
    }
}

fn encode_support_owners(
    encoder: &mut WireEncoder,
    owners: &std::collections::BTreeSet<crate::InterfaceDependencySubject>,
) {
    write_count(encoder, owners.len());

    for owner in owners {
        super::encoding::contract::encode_dependency_subject(encoder, owner);
    }
}

fn encode_value(encoder: &mut WireEncoder, value: &Template) {
    let mut pending = vec![value];

    while let Some(value) = pending.pop() {
        match value {
            Value::Input(input) => { encoder.write_u32(0); encoder.write_u32(input.raw()); }
            Value::Data(input) => { encoder.write_u32(4); encoder.write_u32(input.raw()); }
            Value::Static(root) => {
                encoder.write_u32(5);
                encode_static_reference(encoder, root);
            }
            Value::Deferred { alternatives, unknown } => {
                encoder.write_u32(6);
                encoder.write_u32(u32::from(*unknown));
                write_count(encoder, alternatives.len());

                for alternative in alternatives.iter() {
                    write_optional_u32(encoder, alternative.guard.map(|term| term.raw()));
                    write_optional_u32(encoder, alternative.call.as_ref().map(|call| call.instance.raw()));

                    if let Some(call) = &alternative.call {
                        encoder.write_u32(call.signature.raw());
                        write_optional_u32(encoder, call.receiver.map(|ty| ty.raw()));
                        write_optional_u32(encoder, call.requirement.map(|(ty, _)| ty.raw()));
                        if let Some((_, app)) = call.requirement { encoder.write_u32(app.raw()); }
                        write_optional_u32(encoder, call.dispatch.map(|w| w.raw()));
                        write_count(encoder, call.witnesses.len());

                        for witness in &*call.witnesses { encoder.write_u32(witness.raw()); }
                    }

                    write_count(encoder, alternative.captures.len());
                    write_count(encoder, alternative.owned_inputs.len());

                    for ordinal in alternative.owned_inputs.iter() { encoder.write_u32(ordinal.raw()); }
                }

                pending.extend(alternatives.iter().rev().flat_map(|alternative| alternative.captures.iter().rev()));
            }
            Value::Unknown => encoder.write_u32(1),
            Value::Tagged(variant, payload) => {
                encoder.write_u32(7);
                write_symbol_reference(encoder, variant);
                pending.push(payload);
            }
            Value::Operation(operation, values) => {
                encoder.write_u32(2);

                encoder.write_u32(match operation {
                    Op::Select(_) => 0, Op::EmptyPrefix => 1, Op::Append => 2,
                    Op::Last => 3, Op::Remainder => 4, Op::Read => 5, Op::Reference => 6, Op::Owned => 7, Op::Scalar => 8,
                    Op::OwningStorage(_) => 9,
                    Op::Add => 10, Op::Subtract => 11,
                    Op::Inline => 12,
                    Op::Present => 13,
                });

                if let Op::Select(field) | Op::OwningStorage(field) = operation {
                    super::encoding::contract::encode_dependency_projections(encoder, std::slice::from_ref(field));
                }

                pending.extend(values.iter().rev());
            }
            Value::Aggregate { base, fields } => {
                encoder.write_u32(3);
                write_count(encoder, fields.len());

                for (field, _) in fields.iter() {
                    super::encoding::contract::encode_dependency_projections(encoder, std::slice::from_ref(field));
                }

                pending.extend(fields.iter().rev().map(|(_, value)| value));
                pending.push(base);
            }
        }
    }
}

pub(super) fn encode_static_reference(encoder: &mut WireEncoder, root: &InterfaceStaticReference) {
    write_symbol_reference(encoder, &root.declaration);
    encoder.write_u32(root.substitution.raw());
    write_count(encoder, root.selected_witnesses.len());

    for witness in root.selected_witnesses.iter() { encoder.write_u32(witness.raw()); }

    write_string(encoder, root.target.as_str());
    encoder.write_u32(u32::from(root.closed));
}

pub(super) fn decode_static_reference(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceStaticReference, InterfaceValidationError> {
    let declaration = read_symbol_reference(reader, context)?;
    let substitution = InterfaceGenericSubstitutionId::new(read_u32(reader)?);
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut witnesses = context.allocate_items(reader, count)?;

    for _ in 0..count { witnesses.push(InterfaceImplementationInstanceId::new(read_u32(reader)?)); }

    let target = bray_target::TargetIdentity::try_new(read_string(reader, context)?).ok_or_else(||
        super::invalid_value(InterfaceValidationField::Identity))?;

    let closed = match read_u32(reader)? {
        0 => false, 1 => true,
        raw => return Err(super::invalid_discriminant(InterfaceValidationField::Dependency, raw)),
    };

    Ok(InterfaceStaticReference::new(declaration, substitution, witnesses, target, closed))
}

pub(super) fn decode_values(reader: &mut WireReader<'_>, limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<Binding>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut values = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let report = super::decoding::contract::decode_optional_requirements(reader, limits, context)?.map(Arc::from);
        let effects = super::decoding::contract::decode_dependency_effects(reader, limits, context)?;
        let ownership = super::ownership::decode_owned_result(reader, limits, context)?;
        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut carriers = context.allocate_items(reader, count)?;

        for _ in 0..count {
            let guarantee = InterfaceConstantTermId::new(read_u32(reader)?);

            let support = match read_u32(reader)? {
                0 => None,
                1 => {
                    let owners = decode_support_owners(reader, limits, context)?;
                    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                    let mut addresses = context.allocate_items(reader, count)?;

                    for _ in 0..count {
                        let address = InterfaceConstantTermId::new(read_u32(reader)?);

                        if addresses.last().is_some_and(|(last, _)| *last >= address) {
                            return Err(super::invalid_value(InterfaceValidationField::Dependency));
                        }

                        addresses.push((address, decode_support_owners(reader, limits, context)?));
                    }

                    Some(bray_symbols::DependencyWitnessSupport::new(owners, addresses))
                }
                raw => return Err(super::invalid_discriminant(InterfaceValidationField::Dependency, raw)),
            };

            carriers.push(bray_symbols::DependencyWitnessCarrier::new(guarantee,
                super::decoding::contract::decode_dependency_projections(reader, limits, context)?)
                .with_support(support));

        }

        let (references, stored_values) = super::decoding::contract::decode_dependency_storage(reader, limits, context)?;

        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut inactive_payloads = context.allocate_items(reader, count)?;

        for _ in 0..count {
            inactive_payloads.push(super::decoding::contract::decode_dependency_subject(reader, limits, context)?);
        }

        let guard = read_optional_u32(reader)?.map(InterfaceConstantTermId::new);
        let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
        let mut bindings = context.allocate_items(reader, count)?;

        for _ in 0..count {
            bindings.push((super::decoding::contract::decode_dependency_subject(reader, limits, context)?,
                decode_value(reader, limits, context, 1)?));
        }

        values.push(Binding {
            guard, report, effects: effects.into(), bindings: bindings.into(), ownership, carriers: carriers.into(),
            references: references.into(), stored_values: stored_values.into(),
            inactive_payloads: inactive_payloads.into(),
        });
    }

    Ok(values)
}

fn decode_support_owners(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<std::collections::BTreeSet<crate::InterfaceDependencySubject>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut owners = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let owner = super::decoding::contract::decode_dependency_subject(reader, limits, context)?;

        if owners.last().is_some_and(|last| last >= &owner) {
            return Err(super::invalid_value(InterfaceValidationField::Dependency));
        }

        owners.push(owner);
    }

    Ok(owners.into_iter().collect())
}

fn decode_value(reader: &mut WireReader<'_>, limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext, depth: u64,
) -> Result<Template, InterfaceValidationError> {
    limits.check(InterfaceLimit::SemanticTypeDepth, depth)?;

    Ok(match read_u32(reader)? {
        0 => Value::Input(InterfaceConstantTermId::new(read_u32(reader)?)),
        4 => Value::Data(InterfaceConstantTermId::new(read_u32(reader)?)),
        5 => Value::Static(decode_static_reference(reader, limits, context)?),
        6 => {
            let unknown = match read_u32(reader)? {
                0 => false, 1 => true,
                raw => return Err(super::invalid_discriminant(InterfaceValidationField::Dependency, raw)),
            };

            let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
            let mut headers = context.allocate_items(reader, count)?;

            for _ in 0..count {
                let guard = read_optional_u32(reader)?.map(InterfaceConstantTermId::new);

                let call = if let Some(instance) = read_optional_u32(reader)? {
                    let signature = crate::InterfaceTypeId::new(read_u32(reader)?);

                    let receiver = read_optional_u32(reader)?.map(crate::InterfaceTypeId::new);

                    let requirement = match read_optional_u32(reader)? {
                        Some(ty) => Some((crate::InterfaceTypeId::new(ty), crate::InterfaceTraitApplicationId::new(read_u32(reader)?))),
                        None => None,
                    };

                    let dispatch = read_optional_u32(reader)?.map(InterfaceImplementationInstanceId::new);
                    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                    let mut witnesses = context.allocate_items(reader, count)?;

                    for _ in 0..count { witnesses.push(InterfaceImplementationInstanceId::new(read_u32(reader)?)); }

                    Some(Arc::new(crate::InterfaceCapturedCallable { instance: crate::InterfaceCallableInstanceId::new(instance),
                        signature, receiver, requirement, dispatch, witnesses: witnesses.into() }))
                } else { None };

                let captures = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                let mut owned_inputs = context.allocate_items(reader, count)?;

                for _ in 0..count { owned_inputs.push(bray_symbols::SymbolOrdinal::new(read_u32(reader)?)); }

                headers.push((guard, call, captures, owned_inputs));
            }

            let mut alternatives = context.allocate_items(reader, count)?;

            for (guard, call, count, owned_inputs) in headers {
                let mut captures = context.allocate_items(reader, count)?;

                for _ in 0..count { captures.push(decode_value(reader, limits, context, depth + 1)?); }

                alternatives.push(bray_symbols::CapturedDeferredInvocation { guard, call, captures: captures.into(), owned_inputs: owned_inputs.into() });
            }

            Value::Deferred { alternatives: alternatives.into(), unknown }
        }
        1 => Value::Unknown,
        7 => Value::Tagged(read_symbol_reference(reader, context)?,
            Arc::new(decode_value(reader, limits, context, depth + 1)?)),
        2 => {
            let operation = match read_u32(reader)? {
                0 => Op::Select(decode_projection(reader, limits, context)?),
                1 => Op::EmptyPrefix, 2 => Op::Append, 3 => Op::Last, 4 => Op::Remainder, 5 => Op::Read,
                6 => Op::Reference, 7 => Op::Owned, 8 => Op::Scalar,
                9 => Op::OwningStorage(decode_projection(reader, limits, context)?),
                10 => Op::Add, 11 => Op::Subtract,
                12 => Op::Inline,
                13 => Op::Present,
                raw => return Err(super::invalid_discriminant(InterfaceValidationField::Dependency, raw)),
            };

            let mut values = context.allocate_items(reader, operation.operand_count())?;

            for _ in 0..operation.operand_count() { values.push(decode_value(reader, limits, context, depth + 1)?); }

            Value::Operation(operation, values.into())
        }
        3 => {
            let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
            let mut fields = context.allocate_items(reader, count)?;

            for _ in 0..count { fields.push(decode_projection(reader, limits, context)?); }

            let base = Arc::new(decode_value(reader, limits, context, depth + 1)?);
            let mut values = context.allocate_items(reader, count)?;

            for field in fields { values.push((field, decode_value(reader, limits, context, depth + 1)?)); }

            Value::Aggregate { base, fields: values.into() }
        }
        raw => return Err(super::invalid_discriminant(InterfaceValidationField::Dependency, raw)),
    })
}

fn decode_projection(reader: &mut WireReader<'_>, limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDependencyProjection, InterfaceValidationError> {
    let fields = super::decoding::contract::decode_dependency_projections(reader, limits, context)?;

    if fields.len() != 1 { return Err(super::invalid_value(InterfaceValidationField::Dependency)); }

    Ok(fields.into_iter().next().expect("validated value selector has one projection"))
}
