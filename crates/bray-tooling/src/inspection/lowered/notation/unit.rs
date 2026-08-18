use std::fmt::Write;

use super::super::model::{InspectionMirBlock, InspectionMirUnit};
use super::support::{source_text, value_type};

pub(crate) fn render_unit(unit: &InspectionMirUnit) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "mir {} unit{} {{", unit.unit_kind, unit.unit_id);
    let _ = writeln!(output, "    target = \"{}\";", unit.target);
    let _ = writeln!(output, "    runtime_abi = {};", unit.runtime_abi);
    let _ = writeln!(output, "    // source: {}", source_text(&unit.source));

    if let Some(frame) = &unit.frame {
        output.push('\n');

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
                .map(|storage| format!("slot{storage}"))
                .collect::<Vec<_>>()
                .join(", ");

            let _ = writeln!(
                output,
                "        state {} -> bb{} live [{}];",
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
                "    {} slot{}: {};",
                storage.storage_kind,
                storage.id,
                storage.r#type.text()
            );
        }
    }

    for block in &unit.blocks {
        output.push('\n');
        push_block(&mut output, block, unit);
    }

    output.push_str("}\n");

    output
}

fn push_block(output: &mut String, block: &InspectionMirBlock, unit: &InspectionMirUnit) {
    let parameters = block
        .parameters
        .iter()
        .map(|parameter| {
            let ty = value_type(unit, *parameter).unwrap_or("<missing>");

            format!("%{parameter}: {ty}")
        })
        .collect::<Vec<_>>()
        .join(", ");

    let role = if block.id == unit.entry {
        String::from(" entry")
    } else if block.block_kind == "ordinary" {
        String::new()
    } else {
        format!(" {}", block.block_kind)
    };

    let parameters = if parameters.is_empty() {
        String::new()
    } else {
        format!("({parameters})")
    };

    let _ = writeln!(
        output,
        "    bb{}{}{parameters}: // source: {}",
        block.id,
        role,
        source_text(&block.source)
    );

    for operation in &block.operations {
        super::operation::render(output, operation, unit, &block.source);
    }

    super::terminator::render(output, &block.terminator, &block.source);
}
