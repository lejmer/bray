use super::facts::section;
use super::model::EncodedSemanticSection;
use crate::semantic::codec::common::{
    write_count, write_symbol_reference, write_symbol_references,
};
use crate::semantic::codec::record::encode_record_table;
use crate::semantic::model::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateOperation,
    InterfaceImplementationReference, InterfaceTemplateReference,
};
use crate::tag::WireTag;
use crate::wire::WireEncoder;
use crate::{InterfaceSectionTag, InterfaceSemanticFacts};

pub(super) fn encode_templates(facts: &InterfaceSemanticFacts) -> EncodedSemanticSection {
    let mut encoder = WireEncoder::new();

    encoder.write_u32(super::super::DECLARATION_TEMPLATE_FORMAT_VERSION);

    encode_record_table(
        &mut encoder,
        &facts.checked_templates,
        |encoder, template| {
            encode_template(encoder, template);
        },
    );

    encode_record_table(
        &mut encoder,
        &facts.declaration_templates,
        |encoder, declaration| {
            encode_declaration_template(encoder, declaration);
        },
    );

    section(
        InterfaceSectionTag::DeclarationTemplates,
        facts.checked_templates.len() + facts.declaration_templates.len(),
        encoder,
    )
}

fn encode_declaration_template(
    encoder: &mut WireEncoder,
    declaration: &crate::InterfaceDeclarationTemplate,
) {
    write_symbol_reference(encoder, declaration.owner());
    encoder.write_u32(declaration.kind().to_wire());
    encoder.write_u32(declaration.ordinal().raw());
    encoder.write_u32(declaration.entity().raw());
}

pub(crate) fn encode_template_payload(template: &InterfaceCheckedTemplate) -> Vec<u8> {
    let mut encoder = WireEncoder::new();

    encode_template(&mut encoder, template);

    encoder.into_bytes()
}

fn encode_template(encoder: &mut WireEncoder, template: &InterfaceCheckedTemplate) {
    encoder.write_u32(template.kind().to_wire());
    write_count(encoder, template.inputs().len());

    for input in template.inputs() {
        encode_input_kind(encoder, input.kind());
        encoder.write_u32(input.ty().raw());
    }

    write_count(encoder, template.nodes().len());

    for node in template.nodes() {
        encode_operation(encoder, node.operation());
        encoder.write_u32(node.ty().raw());
    }

    write_count(encoder, template.temporaries().len());

    for temporary in template.temporaries() {
        encoder.write_u32(temporary.initializer().raw());
        encoder.write_u32(temporary.ty().raw());
        encoder.write_u32(temporary.dependency_contract().raw());
    }

    encoder.write_u32(template.result().raw());

    let behavior = template.behavior();

    write_symbol_references(encoder, behavior.effects());
    write_symbol_references(encoder, behavior.capabilities());
    write_symbol_references(encoder, behavior.trusted_obligations());
    write_symbol_references(encoder, behavior.execution_requirements());

    write_count(encoder, behavior.lifecycle_obligations().len());

    for obligation in behavior.lifecycle_obligations() {
        encoder.write_u32(obligation.to_wire());
    }

    encoder.write_u32(behavior.dependency_contract().raw());
    encoder.write_u32(behavior.current_run_cancellation().to_wire());
    write_count(encoder, behavior.witnesses().len());

    for witness in behavior.witnesses() {
        encode_implementation_reference(encoder, witness);
    }
}

fn encode_input_kind(encoder: &mut WireEncoder, kind: &InterfaceCheckedTemplateInputKind) {
    match kind {
        InterfaceCheckedTemplateInputKind::GenericType(parameter) => {
            encoder.write_u32(1);
            write_symbol_reference(encoder, parameter);
        }
        InterfaceCheckedTemplateInputKind::GenericConstant(parameter) => {
            encoder.write_u32(2);
            write_symbol_reference(encoder, parameter);
        }
        InterfaceCheckedTemplateInputKind::Receiver => encoder.write_u32(3),
        InterfaceCheckedTemplateInputKind::Parameter(ordinal) => {
            encoder.write_u32(4);
            encoder.write_u32(ordinal.raw());
        }
        InterfaceCheckedTemplateInputKind::PostconditionResult => encoder.write_u32(5),
    }
}

fn encode_operation(encoder: &mut WireEncoder, operation: &InterfaceCheckedTemplateOperation) {
    match operation {
        InterfaceCheckedTemplateOperation::Input(input) => {
            write_tagged_template_id(encoder, 1, input.raw());
        }
        InterfaceCheckedTemplateOperation::Constant { term, usage } => {
            write_tagged_template_id(encoder, 2, term.raw());
            encoder.write_u64(usage.aggregate_elements());
            encoder.write_u64(usage.literal_bytes());
            encoder.write_u64(usage.expansions());
        }
        InterfaceCheckedTemplateOperation::Unary { operation, operand } => {
            encoder.write_u32(12);
            encoder.write_u32(operation.to_wire());
            encoder.write_u32(operand.raw());
        }
        InterfaceCheckedTemplateOperation::Binary {
            operation,
            left,
            right,
        } => {
            encoder.write_u32(13);
            encoder.write_u32(operation.to_wire());
            encoder.write_u32(left.raw());
            encoder.write_u32(right.raw());
        }
        InterfaceCheckedTemplateOperation::Declaration(declaration) => {
            encoder.write_u32(3);
            encode_template_reference(encoder, declaration);
        }
        InterfaceCheckedTemplateOperation::Call {
            callable,
            substitution,
            arguments,
            implementation,
        } => {
            encoder.write_u32(4);
            encode_template_reference(encoder, callable);
            encoder.write_u32(substitution.raw());
            write_node_ids(encoder, arguments);

            match implementation {
                Some((implementation, substitution)) => {
                    encoder.write_u32(1);
                    encode_implementation_reference(encoder, implementation);
                    encoder.write_u32(substitution.raw());
                }
                None => encoder.write_u32(0),
            }
        }
        InterfaceCheckedTemplateOperation::Convert { value, target } => {
            encoder.write_u32(5);
            encoder.write_u32(value.raw());
            encoder.write_u32(target.raw());
        }
        InterfaceCheckedTemplateOperation::Tuple(elements) => {
            encoder.write_u32(6);
            write_node_ids(encoder, elements);
        }
        InterfaceCheckedTemplateOperation::Array(elements) => {
            encoder.write_u32(7);
            write_node_ids(encoder, elements);
        }
        InterfaceCheckedTemplateOperation::Project { subject, member } => {
            encoder.write_u32(8);
            encoder.write_u32(subject.raw());
            encode_template_reference(encoder, member);
        }
        InterfaceCheckedTemplateOperation::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            encoder.write_u32(9);
            encoder.write_u32(condition.raw());
            encoder.write_u32(when_true.raw());
            encoder.write_u32(when_false.raw());
        }
        InterfaceCheckedTemplateOperation::ShortCircuit { kind, left, right } => {
            encoder.write_u32(10);
            encoder.write_u32(kind.to_wire());
            encoder.write_u32(left.raw());
            encoder.write_u32(right.raw());
        }
        InterfaceCheckedTemplateOperation::Temporary(temporary) => {
            write_tagged_template_id(encoder, 11, temporary.raw());
        }
    }
}

fn write_tagged_template_id(encoder: &mut WireEncoder, tag: u32, id: u32) {
    encoder.write_u32(tag);
    encoder.write_u32(id);
}

fn write_node_ids(encoder: &mut WireEncoder, nodes: &[bray_bound_tree::CheckedTemplateNodeId]) {
    write_count(encoder, nodes.len());

    for node in nodes {
        encoder.write_u32(node.raw());
    }
}

fn encode_template_reference(encoder: &mut WireEncoder, reference: &InterfaceTemplateReference) {
    match reference {
        InterfaceTemplateReference::Symbol(symbol) => {
            encoder.write_u32(1);
            write_symbol_reference(encoder, symbol);
        }
        InterfaceTemplateReference::Support(entity) => {
            encoder.write_u32(2);
            encoder.write_u32(entity.raw());
        }
    }
}

fn encode_implementation_reference(
    encoder: &mut WireEncoder,
    reference: &InterfaceImplementationReference,
) {
    match reference {
        InterfaceImplementationReference::Symbol(symbol) => {
            encoder.write_u32(1);
            write_symbol_reference(encoder, symbol);
        }
        InterfaceImplementationReference::Support(entity) => {
            encoder.write_u32(2);
            encoder.write_u32(entity.raw());
        }
    }
}
