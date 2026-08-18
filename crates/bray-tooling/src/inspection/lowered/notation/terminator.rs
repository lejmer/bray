use std::fmt::Write;

use super::super::model::{InspectionMirSource, InspectionMirTerminator};
use super::support::{
    attribute_text, cleanup_edge_text_for, edge_for, edge_text, edge_text_for, operand_for,
    operand_text, optional_operand, place_for, semantic_for, semantic_text, source_annotation,
    symbol_for, type_for,
};

pub(super) fn render(
    output: &mut String,
    terminator: &InspectionMirTerminator,
    block_source: &InspectionMirSource,
) {
    if terminator.terminator_kind == "switch" {
        render_switch(output, terminator, block_source);

        return;
    }

    let source = source_annotation(&terminator.source, block_source);

    let _ = writeln!(output, "        {};{}", terminator_text(terminator), source);
}

fn render_switch(
    output: &mut String,
    terminator: &InspectionMirTerminator,
    block_source: &InspectionMirSource,
) {
    let _ = writeln!(
        output,
        "        switch {} {{",
        operand_for(terminator, "discriminant")
    );

    for edge in terminator
        .edges
        .iter()
        .filter(|edge| edge.role.starts_with("case["))
    {
        let value = semantic_for(terminator, &edge.role)
            .map(semantic_text)
            .unwrap_or_else(|| edge.role.clone());

        let _ = writeln!(
            output,
            "            case {value}: goto {};",
            edge_text(edge)
        );
    }

    if let Some(edge) = edge_for(terminator, "otherwise") {
        let _ = writeln!(output, "            default: goto {};", edge_text(edge));
    }

    let source = source_annotation(&terminator.source, block_source);
    let _ = writeln!(output, "        }}{source}");
}

fn terminator_text(terminator: &InspectionMirTerminator) -> String {
    match terminator.terminator_kind {
        "goto" => format!("goto {}", edge_text_for(terminator, "target")),
        "branch" => format!(
            "branch {} -> {} else {}",
            operand_for(terminator, "condition"),
            edge_text_for(terminator, "then"),
            edge_text_for(terminator, "else")
        ),
        "pattern_branch" => format!(
            "match {} when {} -> {} else {}",
            operand_for(terminator, "subject"),
            pattern_text(terminator),
            edge_text_for(terminator, "matched"),
            edge_text_for(terminator, "unmatched")
        ),
        "iterate" => format!(
            "iterate {} using {} element {} -> item {} exhausted {}",
            place_for(terminator, "cursor"),
            symbol_for(terminator, "next"),
            type_for(terminator, "element").unwrap_or("<missing>"),
            edge_text_for(terminator, "item"),
            edge_text_for(terminator, "exhausted")
        ),
        "range_iterate" => format!(
            "iterate range {} element {} -> item {} exhausted {}",
            place_for(terminator, "cursor"),
            type_for(terminator, "element").unwrap_or("<missing>"),
            edge_text_for(terminator, "item"),
            edge_text_for(terminator, "exhausted")
        ),
        "inline_assembly" => generic_terminator(terminator, "inline_assembly"),
        "return" => format!("return{}", optional_operand(terminator, "value")),
        "unreachable" => String::from("unreachable"),
        "suspend" => format!(
            "suspend {} state {} resume {} cancel {}",
            attribute_text(terminator, "kind").unwrap_or_else(|| "awaited".into()),
            attribute_text(terminator, "resume_state").unwrap_or_else(|| "<missing>".into()),
            edge_text_for(terminator, "resume"),
            cleanup_edge_text_for(terminator, "cancellation")
        ),
        "forward_run_result" => format!(
            "forward {} completed {} panicked {} cancelled {}",
            operand_for(terminator, "result"),
            edge_text_for(terminator, "completed"),
            cleanup_edge_text_for(terminator, "panicked"),
            cleanup_edge_text_for(terminator, "cancelled")
        ),
        "begin_cleanup" => format!("begin {}", cleanup_edge_text_for(terminator, "cleanup")),
        "continue_cleanup" => format!("continue {}", cleanup_edge_text_for(terminator, "cleanup")),
        "panic" => format!(
            "panic {} {}",
            operand_for(terminator, "report"),
            cleanup_edge_text_for(terminator, "cleanup")
        ),
        "propagate_panic" => format!("propagate panic {}", operand_for(terminator, "report")),
        "propagate_cancellation" => String::from("propagate cancellation"),
        "cancel_current_run" => format!(
            "cancel current run {}",
            cleanup_edge_text_for(terminator, "cleanup")
        ),
        _ => generic_terminator(terminator, terminator.terminator_kind),
    }
}

fn generic_terminator(terminator: &InspectionMirTerminator, name: &str) -> String {
    let mut details = Vec::new();

    details.extend(
        terminator
            .operands
            .iter()
            .map(|operand| format!("{}: {}", operand.role, operand_text(&operand.operand))),
    );

    details.extend(terminator.places.iter().map(|place| {
        format!(
            "{}: {}",
            place.role,
            super::support::place_text(&place.place)
        )
    }));

    details.extend(
        terminator
            .edges
            .iter()
            .map(|edge| format!("{}: {}", edge.role, edge_text(edge))),
    );

    details.extend(
        terminator
            .attributes
            .iter()
            .map(|attribute| format!("{}: {}", attribute.name, attribute.value.text())),
    );

    details.extend(
        terminator
            .symbols
            .iter()
            .map(|symbol| format!("{}: @{}", symbol.role, symbol.symbol.display_name())),
    );

    details.extend(
        terminator
            .types
            .iter()
            .map(|r#type| format!("{}: {}", r#type.role, r#type.r#type.text())),
    );

    details.extend(
        terminator
            .semantic_values
            .iter()
            .map(|value| format!("{}: {}", value.role, semantic_text(value))),
    );

    format!("{}({})", name.replace('_', "."), details.join(", "))
}

fn pattern_text(terminator: &InspectionMirTerminator) -> String {
    match attribute_text(terminator, "predicate").as_deref() {
        Some("literal") | Some("constant") => semantic_for(terminator, "predicate")
            .map(semantic_text)
            .unwrap_or_else(|| String::from("constant")),
        Some("nullable_absent") => String::from("none"),
        Some("nullable_present") => String::from("some"),
        Some("active_union_variant") => symbol_for(terminator, "variant"),
        Some("product_shape") => symbol_for(terminator, "product"),
        Some("tuple_shape") => format!(
            "tuple({})",
            attribute_text(terminator, "arity").unwrap_or_else(|| "<missing>".into())
        ),
        Some("array_shape") => format!(
            "array({})",
            attribute_text(terminator, "length").unwrap_or_else(|| "<missing>".into())
        ),
        Some("owned_target") => String::from("owned"),
        _ => String::from("<predicate>"),
    }
}
