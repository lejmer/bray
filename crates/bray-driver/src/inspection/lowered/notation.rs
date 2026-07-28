use std::fmt::Write;

use super::model::{
    InspectionMirEdge, InspectionMirNamedOperand, InspectionMirNamedPlace, InspectionMirOperand,
    InspectionMirOperation, InspectionMirPlace, InspectionMirSource, InspectionMirTerminator,
    InspectionMirUnit,
};

pub(super) fn render_unit(unit: &InspectionMirUnit) -> String {
    let mut output = String::new();

    let _ = writeln!(
        output,
        "mir unit{} {} target \"{}\" abi {} {{",
        unit.unit_id, unit.unit_kind, unit.target, unit.runtime_abi
    );

    let _ = writeln!(output, "    // {}", source_text(&unit.source));
    let _ = writeln!(output, "    entry bb{};", unit.entry);

    if let Some(frame) = &unit.frame {
        let _ = writeln!(
            output,
            "    frame {} abi {} -> {} {{",
            frame.id,
            frame.abi,
            frame.result_type.text()
        );

        for state in &frame.states {
            let storages = state
                .initialized_storages
                .iter()
                .map(|storage| format!("s{storage}"))
                .collect::<Vec<_>>()
                .join(", ");

            let _ = writeln!(
                output,
                "        state{} -> bb{} initialized [{}];",
                state.id, state.entry, storages
            );
        }

        output.push_str("    }\n");
    }

    if !unit.storages.is_empty() {
        output.push('\n');

        for storage in &unit.storages {
            let _ = writeln!(
                output,
                "    storage s{}: {} [{}]; // {}",
                storage.id,
                storage.r#type.text(),
                storage.storage_kind,
                source_text(&storage.source)
            );
        }
    }

    for block in &unit.blocks {
        output.push('\n');

        let parameters = block
            .parameters
            .iter()
            .map(|parameter| {
                unit.values
                    .iter()
                    .find(|value| value.id == *parameter)
                    .map(|value| format!("%v{parameter}: {}", value.r#type.text()))
                    .unwrap_or_else(|| format!("%v{parameter}: <missing>"))
            })
            .collect::<Vec<_>>()
            .join(", ");

        let _ = writeln!(
            output,
            "    bb{}({parameters}): // {} {}",
            block.id,
            block.block_kind,
            source_text(&block.source)
        );

        for operation in &block.operations {
            push_operation(&mut output, operation, unit);
        }

        push_terminator(&mut output, &block.terminator);
    }

    output.push_str("}\n");

    output
}

fn push_operation(
    output: &mut String,
    operation: &InspectionMirOperation,
    unit: &InspectionMirUnit,
) {
    output.push_str("        ");

    if let Some(result) = operation.result {
        let result_type = unit
            .values
            .iter()
            .find(|value| value.id == result)
            .map(|value| value.r#type.text())
            .unwrap_or("<missing>");

        let _ = write!(output, "%v{result}: {result_type} = ");
    }

    output.push_str(operation.operation_kind);
    push_attributes(output, &operation.attributes);
    push_operands(output, &operation.operands);
    push_places(output, &operation.places);
    push_symbols(output, &operation.symbols);
    push_types(output, &operation.types);
    push_semantic_values(output, &operation.semantic_values);

    let _ = writeln!(
        output,
        "; // op{} {}",
        operation.id,
        source_text(&operation.source)
    );
}

fn push_terminator(output: &mut String, terminator: &InspectionMirTerminator) {
    output.push_str("        ");
    output.push_str(terminator.terminator_kind);
    push_attributes(output, &terminator.attributes);
    push_operands(output, &terminator.operands);
    push_places(output, &terminator.places);
    push_symbols(output, &terminator.symbols);
    push_types(output, &terminator.types);
    push_semantic_values(output, &terminator.semantic_values);

    for edge in &terminator.edges {
        push_edge(output, edge);
    }

    let _ = writeln!(output, "; // {}", source_text(&terminator.source));
}

fn push_attributes(
    output: &mut String,
    attributes: &[super::model::InspectionMirAttribute],
) {
    for attribute in attributes {
        let _ = write!(output, " {}={}", attribute.name, attribute.value.text());
    }
}

fn push_operands(output: &mut String, operands: &[InspectionMirNamedOperand]) {
    for operand in operands {
        let _ = write!(
            output,
            " {}={}",
            operand.role,
            operand_text(&operand.operand)
        );
    }
}

fn push_places(output: &mut String, places: &[InspectionMirNamedPlace]) {
    for place in places {
        let _ = write!(output, " {}={}", place.role, place_text(&place.place));
    }
}

fn push_symbols(output: &mut String, symbols: &[super::model::InspectionMirNamedSymbol]) {
    for symbol in symbols {
        let _ = write!(output, " {}={}", symbol.role, symbol.symbol.display_name());
    }
}

fn push_types(output: &mut String, types: &[super::model::InspectionMirNamedType]) {
    for r#type in types {
        let _ = write!(output, " {}={}", r#type.role, r#type.r#type.text());
    }
}

fn push_semantic_values(
    output: &mut String,
    values: &[super::model::InspectionMirSemanticValue],
) {
    for value in values {
        let text = value
            .text
            .as_ref()
            .map(|text| format!("({text})"))
            .unwrap_or_default();

        let _ = write!(
            output,
            " {}={}:{}{}",
            value.role, value.value_kind, value.id, text
        );
    }
}

fn push_edge(output: &mut String, edge: &InspectionMirEdge) {
    let arguments = edge
        .arguments
        .iter()
        .map(operand_text)
        .collect::<Vec<_>>()
        .join(", ");

    let cleanup = edge
        .cleanup_phase
        .map(|phase| format!("[{phase}]"))
        .unwrap_or_default();

    let _ = write!(
        output,
        " {}{}=bb{}({arguments})",
        edge.role, cleanup, edge.target
    );
}

fn operand_text(operand: &InspectionMirOperand) -> String {
    match operand {
        InspectionMirOperand::Value { value } => format!("%v{value}"),
        InspectionMirOperand::Constant { value, r#type } => {
            format!("const {value}: {}", r#type.text())
        }
        InspectionMirOperand::Immediate { value, r#type } => {
            format!("{value}: {}", r#type.text())
        }
        InspectionMirOperand::Copy { place } => format!("copy {}", place_text(place)),
        InspectionMirOperand::Move { place } => format!("move {}", place_text(place)),
    }
}

fn place_text(place: &InspectionMirPlace) -> String {
    let mut text = format!("s{}", place.storage);

    for projection in &place.projections {
        text.push('.');
        text.push_str(projection.projection_kind);

        if let Some(detail) = &projection.detail {
            text.push('(');
            text.push_str(detail);
            text.push(')');
        }
    }

    text
}

fn source_text(source: &InspectionMirSource) -> String {
    match source {
        InspectionMirSource::Source { syntax, synthesis } => {
            let mut text = syntax.text();

            if let Some(synthesis) = synthesis {
                let _ = write!(
                    text,
                    " synthesized {}:{}",
                    synthesis.role, synthesis.ordinal
                );
            }

            text
        }
        InspectionMirSource::ExecutableHost { package, product } => {
            format!("generated host {package}/{product}")
        }
    }
}
