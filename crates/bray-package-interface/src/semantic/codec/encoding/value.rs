use super::bundle::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::common::{
    write_count, write_optional_u32, write_string, write_symbol_reference,
};
use crate::semantic::codec::record::encode_record_table;
use crate::semantic::model::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantValueKind,
    InterfaceGenericArgument, InterfaceType,
};
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemantics};
use bray_symbols::{BorrowKind, IntegerConstant, IntegerSign, RealConstantBits};

pub(super) fn encode_types(semantics: &InterfaceSemantics) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encode_record_table(
        &mut encoder,
        &semantics.substitutions,
        |encoder, substitution| {
            write_symbol_reference(encoder, &substitution.owner);
            write_count(encoder, substitution.bindings.len());

            for binding in &*substitution.bindings {
                write_symbol_reference(encoder, &binding.parameter);

                match binding.argument {
                    InterfaceGenericArgument::Type(id) => {
                        encoder.write_u32(1);
                        encoder.write_u32(id.raw());
                    }
                    InterfaceGenericArgument::Constant(id) => {
                        encoder.write_u32(2);
                        encoder.write_u32(id.raw());
                    }
                }
            }
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.trait_applications,
        |encoder, application| {
            write_symbol_reference(encoder, &application.definition);
            encoder.write_u32(application.substitution.raw());
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.callable_instances,
        |encoder, instance| {
            write_symbol_reference(encoder, &instance.definition);
            encoder.write_u32(instance.substitution.raw());
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.implementation_instances,
        |encoder, instance| {
            write_symbol_reference(encoder, &instance.definition);
            encoder.write_u32(instance.substitution.raw());
        },
    );

    encode_record_table(&mut encoder, &semantics.types, encode_type);

    section(
        InterfaceSectionTag::SemanticTypes,
        semantics.substitutions.len()
            + semantics.trait_applications.len()
            + semantics.callable_instances.len()
            + semantics.implementation_instances.len()
            + semantics.types.len(),
        encoder,
    )
}

pub(super) fn encode_type(encoder: &mut WireEncoder, ty: &InterfaceType) {
    match ty {
        InterfaceType::Named {
            definition,
            substitution,
        } => {
            encoder.write_u32(1);
            write_symbol_reference(encoder, definition);
            encoder.write_u32(substitution.raw());
        }
        InterfaceType::TypeParameter(parameter) => {
            encoder.write_u32(2);
            write_symbol_reference(encoder, parameter);
        }
        InterfaceType::ContextualSelf(context) => {
            encoder.write_u32(12);
            write_symbol_reference(encoder, context);
        }
        InterfaceType::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => {
            encoder.write_u32(3);
            encoder.write_u32(subject.raw());
            encoder.write_u32(application.raw());

            write_symbol_reference(encoder, member);
        }
        InterfaceType::Tuple(elements) => {
            encoder.write_u32(4);
            write_ids(encoder, elements, |id| id.raw());
        }
        InterfaceType::Array { element, length } => {
            encoder.write_u32(5);
            encoder.write_u32(element.raw());
            encoder.write_u32(length.raw());
        }
        InterfaceType::FlexibleArray(element) => write_tagged_id(encoder, 14, element.raw()),
        InterfaceType::Slice(element) => write_tagged_id(encoder, 6, element.raw()),
        InterfaceType::Generator(element) => write_tagged_id(encoder, 13, element.raw()),
        InterfaceType::Nullable(target) => write_tagged_id(encoder, 7, target.raw()),
        InterfaceType::Borrow { kind, target } => {
            encoder.write_u32(8);

            encoder.write_u32(match kind {
                BorrowKind::Shared => 1,
                BorrowKind::Mutable => 2,
            });

            encoder.write_u32(target.raw());
        }
        InterfaceType::TraitView(application) => write_tagged_id(encoder, 9, application.raw()),
        InterfaceType::OwnedIndirection { storage, target } => {
            encoder.write_u32(10);
            encoder.write_u32(storage.raw());
            encoder.write_u32(target.raw());
        }
        InterfaceType::Callable {
            parameters,
            variadic,
            result,
            constness,
            trust,
            abi,
            invocation_behavior,
            deferred_execution_behavior,
        } => {
            encoder.write_u32(11);

            write_count(encoder, parameters.len());

            for parameter in &**parameters {
                write_string(encoder, &parameter.name);

                encoder.write_u32(parameter.position.to_wire());
                encoder.write_u32(parameter.mode.to_wire());
                encoder.write_u32(parameter.ty.raw());
            }

            encoder.write_u32(u32::from(*variadic));
            encoder.write_u32(result.raw());
            encoder.write_u32((*constness).to_wire());
            encoder.write_u32((*trust).to_wire());
            encoder.write_u32((*abi).to_wire());
            super::contract::encode_callable_behavior(encoder, invocation_behavior);

            match deferred_execution_behavior {
                Some(behavior) => {
                    encoder.write_u32(1);
                    super::contract::encode_callable_behavior(encoder, behavior);
                }
                None => encoder.write_u32(0),
            }
        }
    }
}

pub(super) fn encode_constants(semantics: &InterfaceSemantics) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encode_record_table(
        &mut encoder,
        &semantics.constant_values,
        |encoder, value| {
            encoder.write_u32(value.ty.raw());
            encode_constant_value(encoder, &value.kind);
        },
    );

    encode_record_table(
        &mut encoder,
        &semantics.constant_terms,
        encode_constant_term,
    );

    section(
        InterfaceSectionTag::Constants,
        semantics.constant_values.len() + semantics.constant_terms.len(),
        encoder,
    )
}

pub(super) fn encode_constant_value(encoder: &mut WireEncoder, kind: &InterfaceConstantValueKind) {
    match kind {
        InterfaceConstantValueKind::Boolean(value) => {
            encoder.write_u32(1);
            encoder.write_u32(u32::from(*value));
        }
        InterfaceConstantValueKind::Character(value) => write_tagged_id(encoder, 2, *value as u32),
        InterfaceConstantValueKind::Integer(value) => {
            encoder.write_u32(3);
            encode_integer(encoder, value);
        }
        InterfaceConstantValueKind::Real(value) => {
            encoder.write_u32(4);
            encode_real(encoder, *value);
        }
        InterfaceConstantValueKind::Complex { real, imaginary } => {
            encoder.write_u32(5);

            encode_real(encoder, *real);
            encode_real(encoder, *imaginary);
        }
        InterfaceConstantValueKind::String(value) => {
            encoder.write_u32(6);
            write_string(encoder, value);
        }
        InterfaceConstantValueKind::Unit => encoder.write_u32(7),
        InterfaceConstantValueKind::NullableAbsent => encoder.write_u32(8),
        InterfaceConstantValueKind::NullablePresent(value) => {
            write_tagged_id(encoder, 9, value.raw());
        }
        InterfaceConstantValueKind::Tuple(values) => {
            encoder.write_u32(10);
            write_ids(encoder, values, |id| id.raw());
        }
        InterfaceConstantValueKind::Array(values) => {
            encoder.write_u32(11);
            write_ids(encoder, values, |id| id.raw());
        }
        InterfaceConstantValueKind::Product(values) => {
            encoder.write_u32(12);
            write_constant_fields(encoder, values, |id| id.raw());
        }
        InterfaceConstantValueKind::Union { variant, fields } => {
            encoder.write_u32(13);

            write_symbol_reference(encoder, variant);
            write_constant_fields(encoder, fields, |id| id.raw());
        }
    }
}

pub(super) fn encode_constant_term(encoder: &mut WireEncoder, term: &InterfaceConstantTerm) {
    match term {
        InterfaceConstantTerm::Typed { term, ty } => {
            encoder.write_u32(17);
            encoder.write_u32(term.raw());
            encoder.write_u32(ty.raw());
        }
        InterfaceConstantTerm::Value(id) => write_tagged_id(encoder, 1, id.raw()),
        InterfaceConstantTerm::IntegerLiteral { ty, value } => {
            encoder.write_u32(9);
            encoder.write_u32((*ty).to_wire());

            encode_integer(encoder, value);
        }
        InterfaceConstantTerm::Parameter(parameter) => {
            encoder.write_u32(2);
            write_symbol_reference(encoder, parameter);
        }
        InterfaceConstantTerm::CallableArgument(ordinal) => {
            write_tagged_id(encoder, 18, ordinal.raw());
        }
        InterfaceConstantTerm::TargetProperty(record) => {
            encoder.write_u32(3);
            write_symbol_reference(encoder, record);
        }
        InterfaceConstantTerm::Unary { operation, operand } => {
            encoder.write_u32(4);
            encoder.write_u32((*operation).to_wire());
            encoder.write_u32(operand.raw());
        }
        InterfaceConstantTerm::Binary {
            operation,
            left,
            right,
        } => {
            encoder.write_u32(5);
            encoder.write_u32((*operation).to_wire());
            encoder.write_u32(left.raw());
            encoder.write_u32(right.raw());
        }
        InterfaceConstantTerm::Conversion { operand, target } => {
            encoder.write_u32(10);
            encoder.write_u32(operand.raw());
            encoder.write_u32(target.raw());
        }
        InterfaceConstantTerm::NullablePresent(value) => {
            write_tagged_id(encoder, 11, value.raw());
        }
        InterfaceConstantTerm::Tuple(values) => {
            encoder.write_u32(12);
            write_ids(encoder, values, |id| id.raw());
        }
        InterfaceConstantTerm::Array(values) => {
            encoder.write_u32(13);
            write_ids(encoder, values, |id| id.raw());
        }
        InterfaceConstantTerm::Product(fields) => {
            encoder.write_u32(14);
            write_constant_fields(encoder, fields, |id| id.raw());
        }
        InterfaceConstantTerm::Union { variant, fields } => {
            encoder.write_u32(15);
            write_symbol_reference(encoder, variant);
            write_constant_fields(encoder, fields, |id| id.raw());
        }
        InterfaceConstantTerm::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => {
            encoder.write_u32(6);

            write_symbol_reference(encoder, definition);

            encoder.write_u32(substitution.raw());

            write_optional_u32(
                encoder,
                selected_implementation.map(|implementation| implementation.raw()),
            );
        }
        InterfaceConstantTerm::Call {
            callable,
            selected_implementation,
            arguments,
        } => {
            encoder.write_u32(7);
            encoder.write_u32(callable.raw());

            write_optional_u32(
                encoder,
                selected_implementation.map(|implementation| implementation.raw()),
            );

            write_ids(encoder, arguments, |id| id.raw());
        }
        InterfaceConstantTerm::PredicateCall {
            predicate,
            substitution,
            arguments,
        } => {
            encoder.write_u32(16);
            write_symbol_reference(encoder, predicate);
            encoder.write_u32(substitution.raw());
            write_ids(encoder, arguments, |id| id.raw());
        }
        InterfaceConstantTerm::Projection { subject, kind } => {
            encoder.write_u32(8);
            encoder.write_u32(subject.raw());

            encode_constant_projection(encoder, kind);
        }
    }
}

pub(super) fn encode_integer(encoder: &mut WireEncoder, value: &IntegerConstant) {
    encoder.write_u32(match value.sign() {
        IntegerSign::NonNegative => 1,
        IntegerSign::Negative => 2,
    });

    write_count(encoder, value.magnitude().len());
    encoder.write_bytes(value.magnitude());
}

pub(super) fn encode_constant_projection(
    encoder: &mut WireEncoder,
    projection: &InterfaceConstantProjection,
) {
    match projection {
        InterfaceConstantProjection::TupleElement(ordinal) => {
            write_tagged_id(encoder, 1, ordinal.raw());
        }
        InterfaceConstantProjection::ArrayElement(term) => {
            write_tagged_id(encoder, 2, term.raw());
        }
        InterfaceConstantProjection::ProductField(field) => {
            encoder.write_u32(3);
            write_symbol_reference(encoder, field);
        }
        InterfaceConstantProjection::UnionPayloadField(field) => {
            encoder.write_u32(4);
            write_symbol_reference(encoder, field);
        }
        InterfaceConstantProjection::NullableValue => encoder.write_u32(5),
    }
}

pub(super) fn encode_real(encoder: &mut WireEncoder, value: RealConstantBits) {
    match value {
        RealConstantBits::Binary16(bits) => {
            encoder.write_u32(1);
            encoder.write_u16(bits);
        }
        RealConstantBits::Binary32(bits) => write_tagged_id(encoder, 2, bits),
        RealConstantBits::Binary64(bits) => {
            encoder.write_u32(3);
            encoder.write_u64(bits);
        }
        RealConstantBits::Binary128(bits) => {
            encoder.write_u32(4);
            encoder.write_bytes(&bits);
        }
    }
}

fn write_ids<T>(encoder: &mut WireEncoder, values: &[T], raw: impl Fn(&T) -> u32) {
    write_count(encoder, values.len());

    for value in values {
        encoder.write_u32(raw(value));
    }
}

fn write_constant_fields<V>(
    encoder: &mut WireEncoder,
    fields: &[bray_symbols::ConstantField<crate::InterfaceSymbolReference, V>],
    raw: impl Fn(&V) -> u32,
) {
    write_count(encoder, fields.len());

    for field in fields {
        write_symbol_reference(encoder, field.field());
        encoder.write_u32(raw(field.value()));
    }
}

pub(super) fn write_tagged_id(encoder: &mut WireEncoder, tag: u32, id: u32) {
    encoder.write_u32(tag);
    encoder.write_u32(id);
}
