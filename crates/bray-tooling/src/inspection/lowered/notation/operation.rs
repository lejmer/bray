use std::fmt::Write;

use super::super::model::{InspectionMirOperation, InspectionMirSource, InspectionMirUnit};
use super::support::{
    attribute_text, operand_for, operand_text, place_for, place_text, source_annotation,
    type_for,
};

pub(super) fn render(
    output: &mut String,
    operation: &InspectionMirOperation,
    unit: &InspectionMirUnit,
    block_source: &InspectionMirSource,
    include_source: bool,
) {
    let prefix = operation
        .result
        .map(|result| {
            let result_type = unit
                .values
                .iter()
                .find(|value| value.id == result)
                .map(|value| value.r#type.text())
                .unwrap_or("<missing>");

            format!("%{result}: {result_type} = ")
        })
        .unwrap_or_default();

    let source = source_annotation(&operation.source, block_source, include_source);

    let _ = writeln!(
        output,
        "        {prefix}{};{source}",
        operation_text(operation),
    );
}

fn operation_text(operation: &InspectionMirOperation) -> String {
    match operation.operation_kind {
        "anonymous_callable" | "declared_callable" => callable_operation(operation),
        "store" => store_operation(operation),
        "borrow" => borrow_operation(operation),
        "unary" => unary_operation(operation),
        "binary" => binary_operation(operation),
        "aggregate" => aggregate_operation(operation),
        "construct" => construct_operation(operation),
        "convert" | "numeric_conversion" => conversion_operation(operation),
        "pattern_projection" => pattern_projection_operation(operation),
        "generator" => generator_operation(operation),
        "call" => call_operation(operation),
        "memory" => memory_operation(operation),
        "text" => text_operation(operation),
        "panic_report" => panic_report_operation(operation),
        "finalize" => unary_place_operation(operation, "finalize"),
        "destroy" => unary_place_operation(operation, "destroy"),
        "cleanup" => cleanup_operation(operation),
        "create_frame"
        | "move_inactive_frame"
        | "resume_frame"
        | "compose_awaited_frame"
        | "commit_awaited_completion"
        | "start_task"
        | "request_task_cancellation"
        | "observe_current_run_cancellation"
        | "resolve_task"
        | "publish_terminal_state"
        | "execute_cleanup_broadcast"
        | "execute_lifecycle_resolution"
        | "transfer_cleanup_incident"
        | "destroy_terminal_task" => async_operation(operation),
        "materialize_static"
        | "select_test_entry"
        | "execute_root"
        | "observe_root_terminal"
        | "resolve_root_terminal"
        | "report_cleanup_incidents"
        | "begin_static_cleanup"
        | "structured_shutdown" => host_operation(operation),
        _ => generic_operation(operation),
    }
}

fn callable_operation(operation: &InspectionMirOperation) -> String {
    operation
        .symbols
        .iter()
        .find(|symbol| symbol.role == "callable")
        .map(|symbol| format!("callable @{}", symbol.symbol.display_name()))
        .unwrap_or_else(|| generic_operation(operation))
}

fn store_operation(operation: &InspectionMirOperation) -> String {
    let destination = place_for(operation, "destination");
    let value = operand_for(operation, "value");
    let mode = attribute_text(operation, "mode").unwrap_or_else(|| String::from("store"));

    format!(
        "{} {destination} <- {value}",
        match mode.as_str() {
            "initialize" => "init",
            "assign" => "assign",
            _ => "store",
        }
    )
}

fn borrow_operation(operation: &InspectionMirOperation) -> String {
    let kind = match attribute_text(operation, "borrow_kind").as_deref() {
        Some("mutable") => "mut",
        _ => "shared",
    };

    format!("borrow {kind} {}", place_for(operation, "place"))
}

fn unary_operation(operation: &InspectionMirOperation) -> String {
    let operator = match attribute_text(operation, "operator").as_deref() {
        Some("negate") => "-",
        Some("not") => "!",
        Some("bitwise_not") => "~",
        _ => "unary",
    };

    let operand = operand_for(operation, "operand");

    if operator == "unary" {
        format!("unary({operand})")
    } else {
        format!("{operator}{operand}")
    }
}

fn binary_operation(operation: &InspectionMirOperation) -> String {
    let operator = match attribute_text(operation, "operator").as_deref() {
        Some("add") => "+",
        Some("subtract") => "-",
        Some("multiply") => "*",
        Some("divide") => "/",
        Some("remainder") => "%",
        Some("equal") => "==",
        Some("not_equal") => "!=",
        Some("less_than") => "<",
        Some("less_than_or_equal") => "<=",
        Some("greater_than") => ">",
        Some("greater_than_or_equal") => ">=",
        Some("bitwise_and") => "&",
        Some("bitwise_or") => "|",
        Some("bitwise_xor") => "^",
        Some("shift_left") => "<<",
        Some("shift_right") => ">>",
        _ => "?",
    };

    let left = operand_for(operation, "left");
    let right = operand_for(operation, "right");

    format!("{left} {operator} {right}")
}

fn aggregate_operation(operation: &InspectionMirOperation) -> String {
    let kind = attribute_text(operation, "aggregate_kind").unwrap_or_else(|| "aggregate".into());

    let operands = operation
        .operands
        .iter()
        .map(|operand| operand_text(&operand.operand))
        .collect::<Vec<_>>();

    match kind.as_str() {
        "range" if operands.len() == 2 => format!("{}..{}", operands[0], operands[1]),
        "nullable_present" if operands.len() == 1 => format!("some({})", operands[0]),
        "repeated_array" if operands.len() == 2 => {
            format!("[{}; {}]", operands[0], operands[1])
        }
        _ => format!("{kind}({})", operands.join(", ")),
    }
}

fn construct_operation(operation: &InspectionMirOperation) -> String {
    let target = operation
        .symbols
        .iter()
        .find(|symbol| symbol.role == "target")
        .map(|symbol| format!("@{}", symbol.symbol.display_name()))
        .unwrap_or_else(|| String::from("<target>"));

    let inputs = operation
        .operands
        .iter()
        .map(|operand| operand_text(&operand.operand))
        .collect::<Vec<_>>()
        .join(", ");

    format!("construct {target}({inputs})")
}

fn conversion_operation(operation: &InspectionMirOperation) -> String {
    let operand = operand_for(operation, "operand");

    if operation.operation_kind == "numeric_conversion" {
        return format!("truncate({operand})");
    }

    let target = type_for(operation, "conversion_target")
        .or_else(|| type_for(operation, "conversion[0]_target"))
        .unwrap_or("<target>");

    format!("{operand} as {target}")
}

fn pattern_projection_operation(operation: &InspectionMirOperation) -> String {
    let subject = operand_for(operation, "subject");
    let action = attribute_text(operation, "operation").unwrap_or_else(|| "project".into());
    let projection = attribute_text(operation, "projection").unwrap_or_else(|| "value".into());

    let detail = operation
        .symbols
        .iter()
        .find(|symbol| symbol.role == "projected_field")
        .map(|symbol| format!(".{}", symbol.symbol.display_name()))
        .or_else(|| {
            attribute_text(operation, "projection_ordinal").map(|ordinal| {
                match projection.as_str() {
                    "tuple_element" => format!(".{ordinal}"),
                    "element_from_end" => format!("[end - {ordinal}]"),
                    _ => format!("[{ordinal}]"),
                }
            })
        })
        .unwrap_or_default();

    format!("{action} {subject}{detail} [{projection}]")
}

fn generator_operation(operation: &InspectionMirOperation) -> String {
    let step = attribute_text(operation, "step").unwrap_or_else(|| "step".into());
    let destination = place_for(operation, "destination");

    match step.as_str() {
        "begin" => {
            let kind =
                attribute_text(operation, "generator_kind").unwrap_or_else(|| "general".into());

            format!("generator {kind} begin {destination}")
        }
        "push" => format!(
            "generator push {destination} <- {}",
            operand_for(operation, "value")
        ),
        "finish" => format!("generator finish {destination}"),
        "cleanup_broadcast" => format!("generator cleanup {destination}"),
        "destroy" => format!("generator destroy {destination}"),
        _ => generic_operation(operation),
    }
}

fn call_operation(operation: &InspectionMirOperation) -> String {
    let target = operation
        .symbols
        .iter()
        .find(|symbol| symbol.role == "callee")
        .map(|symbol| format!("@{}", symbol.symbol.display_name()))
        .or_else(|| {
            operation
                .operands
                .iter()
                .find(|operand| operand.role == "callee")
                .map(|operand| operand_text(&operand.operand))
        })
        .unwrap_or_else(|| String::from("<callee>"));

    let arguments = operation
        .operands
        .iter()
        .filter(|operand| operand.role == "receiver" || operand.role.starts_with("argument["))
        .map(|operand| operand_text(&operand.operand))
        .collect::<Vec<_>>()
        .join(", ");

    let prefix = match attribute_text(operation, "result_mode").as_deref() {
        Some("lazy_future") => "async ",
        _ => "",
    };

    format!("{prefix}call {target}({arguments})")
}

fn memory_operation(operation: &InspectionMirOperation) -> String {
    let name = attribute_text(operation, "memory_operation").unwrap_or_else(|| "operation".into());

    let name = match name.as_str() {
        "read" => "load",
        "write" => "store",
        "address" => "address_of",
        other => other,
    };

    let operands = operation
        .operands
        .iter()
        .map(|operand| operand_text(&operand.operand))
        .collect::<Vec<_>>()
        .join(", ");

    let order = if let Some(order) = attribute_text(operation, "memory_order") {
        format!(", order = {order}")
    } else if let Some(success) = attribute_text(operation, "success_order") {
        let failure = attribute_text(operation, "failure_order")
            .map(|failure| format!(", failure_order = {failure}"))
            .unwrap_or_default();

        format!(", success_order = {success}{failure}")
    } else {
        String::new()
    };

    format!("memory.{name}({operands}{order})")
}

fn text_operation(operation: &InspectionMirOperation) -> String {
    let name = attribute_text(operation, "operation").unwrap_or_else(|| "operation".into());

    let operands = operation
        .operands
        .iter()
        .map(|operand| operand_text(&operand.operand))
        .collect::<Vec<_>>()
        .join(", ");

    format!("text.{name}({operands})")
}

fn panic_report_operation(operation: &InspectionMirOperation) -> String {
    let kind = attribute_text(operation, "cause").unwrap_or_else(|| "message".into());

    let message = operation
        .operands
        .iter()
        .find(|operand| operand.role == "message")
        .map(|operand| format!("({})", operand_text(&operand.operand)))
        .unwrap_or_default();

    format!("panic_report {kind}{message}")
}

fn unary_place_operation(operation: &InspectionMirOperation, name: &str) -> String {
    format!("{name} {}", place_for(operation, "place"))
}

fn cleanup_operation(operation: &InspectionMirOperation) -> String {
    let phase = attribute_text(operation, "phase").unwrap_or_else(|| "cleanup".into());

    format!("cleanup[{phase}] {}", place_for(operation, "place"))
}

fn async_operation(operation: &InspectionMirOperation) -> String {
    match operation.operation_kind {
        "create_frame" => {
            if attribute_text(operation, "initializer").as_deref() == Some("task_observation") {
                let task = operand_for(operation, "task");
                let completion = type_for(operation, "completion").unwrap_or("<completion>");
                let result = type_for(operation, "result").unwrap_or("<result>");

                let cancellation = attribute_text(operation, "request_cancellation")
                    .unwrap_or_else(|| "false".into());

                format!(
                    "async observe_task {task} -> {result} completion {completion} cancel={cancellation}"
                )
            } else {
                let target = operation
                    .symbols
                    .iter()
                    .find(|symbol| symbol.role == "callee")
                    .map(|symbol| format!("@{}", symbol.symbol.display_name()))
                    .unwrap_or_else(|| String::from("<frame>"));

                format!("async create_frame {target}")
            }
        }
        "move_inactive_frame" => format!(
            "async move_frame {} -> {}",
            place_for(operation, "source"),
            place_for(operation, "destination")
        ),
        "resume_frame" => format!(
            "async resume_frame state {}",
            attribute_text(operation, "state").unwrap_or_else(|| "<missing>".into())
        ),
        "compose_awaited_frame" => {
            format!("async await {}", operand_for(operation, "frame"))
        }
        "commit_awaited_completion" => String::from("async commit_awaited_completion"),
        "start_task" => format!("async start_task {}", operand_for(operation, "frame")),
        "request_task_cancellation" => {
            format!("async cancel_task {}", operand_for(operation, "task"))
        }
        "observe_current_run_cancellation" => String::from("async cancellation_requested"),
        "resolve_task" => format!("async resolve_task {}", operand_for(operation, "task")),
        "publish_terminal_state" => {
            let state = attribute_text(operation, "state").unwrap_or_else(|| "<missing>".into());

            let value = operation
                .operands
                .iter()
                .find(|operand| operand.role == "value" || operand.role == "report")
                .map(|operand| format!(" {}", operand_text(&operand.operand)))
                .unwrap_or_default();

            format!("async publish_terminal {state}{value}")
        }
        "execute_cleanup_broadcast" => String::from("async execute_cleanup_broadcast"),
        "execute_lifecycle_resolution" => String::from("async execute_lifecycle_resolution"),
        "transfer_cleanup_incident" => format!(
            "async transfer_cleanup_incident {}",
            operand_for(operation, "incident")
        ),
        "destroy_terminal_task" => format!(
            "async destroy_terminal_task {}",
            operand_for(operation, "task")
        ),
        _ => generic_operation(operation),
    }
}

fn host_operation(operation: &InspectionMirOperation) -> String {
    match operation.operation_kind {
        "materialize_static" => format!(
            "host materialize_static slot{}",
            attribute_text(operation, "storage").unwrap_or_else(|| "<missing>".into())
        ),
        "select_test_entry" => format!(
            "host select_test_entry {}",
            attribute_text(operation, "entry").unwrap_or_else(|| "<missing>".into())
        ),
        "execute_root" => format!(
            "host execute_root {}",
            attribute_text(operation, "root_kind").unwrap_or_else(|| "<missing>".into())
        ),
        "observe_root_terminal" => String::from("host observe_root_terminal"),
        "resolve_root_terminal" => String::from("host resolve_root_terminal"),
        "report_cleanup_incidents" => String::from("host report_cleanup_incidents"),
        "begin_static_cleanup" => String::from("host begin_static_cleanup"),
        "structured_shutdown" => String::from("host structured_shutdown"),
        _ => generic_operation(operation),
    }
}

fn generic_operation(operation: &InspectionMirOperation) -> String {
    let kind = operation.operation_kind.replace('_', ".");
    let details = detail_items(operation).join(", ");

    format!("{kind}({details})")
}

fn detail_items(operation: &InspectionMirOperation) -> Vec<String> {
    let mut details = Vec::new();

    details.extend(
        operation
            .operands
            .iter()
            .map(|operand| format!("{}: {}", operand.role, operand_text(&operand.operand))),
    );

    details.extend(
        operation
            .places
            .iter()
            .map(|place| format!("{}: {}", place.role, place_text(&place.place))),
    );

    details.extend(
        operation
            .symbols
            .iter()
            .map(|symbol| format!("{}: @{}", symbol.role, symbol.symbol.display_name())),
    );

    details.extend(
        operation
            .types
            .iter()
            .map(|r#type| format!("{}: {}", r#type.role, r#type.r#type.text())),
    );

    details.extend(
        operation
            .semantic_values
            .iter()
            .map(|value| format!("{}: {}", value.role, super::support::semantic_text(value))),
    );

    details.extend(
        operation
            .attributes
            .iter()
            .map(|attribute| format!("{}: {}", attribute.name, attribute.value.text())),
    );

    details
}

#[cfg(test)]
mod tests {
    use super::super::super::model::{
        InspectionMirAttribute, InspectionMirNamedOperand, InspectionMirOperand,
        InspectionMirOperation, InspectionMirSource,
    };
    use super::operation_text;

    fn operation(
        operation_kind: &'static str,
        attributes: Vec<InspectionMirAttribute>,
        operands: Vec<InspectionMirNamedOperand>,
    ) -> InspectionMirOperation {
        InspectionMirOperation {
            id: 0,
            operation_kind,
            result: None,
            source: InspectionMirSource::GeneratedLifecycle { role: "test" },
            attributes,
            operands,
            places: Vec::new(),
            symbols: Vec::new(),
            types: Vec::new(),
            semantic_values: Vec::new(),
        }
    }

    fn attribute(name: &str, value: &str) -> InspectionMirAttribute {
        InspectionMirAttribute {
            name: name.into(),
            value: value.into(),
        }
    }

    fn value(role: &str, value: u32) -> InspectionMirNamedOperand {
        InspectionMirNamedOperand {
            role: role.into(),
            operand: InspectionMirOperand::Value { value },
        }
    }

    #[test]
    fn pattern_indices_use_index_not_member_notation() {
        let operation = operation(
            "pattern_projection",
            vec![
                attribute("operation", "observe"),
                attribute("projection", "element_from_start"),
                attribute("projection_ordinal", "2"),
            ],
            vec![value("subject", 7)],
        );

        assert_eq!(operation_text(&operation), "observe %7[2] [element_from_start]");
    }

    #[test]
    fn tuple_projections_use_member_notation() {
        let operation = operation(
            "pattern_projection",
            vec![
                attribute("operation", "observe"),
                attribute("projection", "tuple_element"),
                attribute("projection_ordinal", "2"),
            ],
            vec![value("subject", 7)],
        );

        assert_eq!(operation_text(&operation), "observe %7.2 [tuple_element]");
    }

    #[test]
    fn compare_exchange_text_preserves_both_orderings() {
        let operation = operation(
            "memory",
            vec![
                attribute("memory_operation", "atomic_compare_exchange"),
                attribute("success_order", "release"),
                attribute("failure_order", "acquire"),
            ],
            vec![value("operand[0]", 3)],
        );

        assert_eq!(
            operation_text(&operation),
            "memory.atomic_compare_exchange(%3, success_order = release, failure_order = acquire)"
        );
    }

    #[test]
    fn task_observation_frames_show_the_observed_task() {
        let operation = operation(
            "create_frame",
            vec![attribute("initializer", "task_observation")],
            vec![value("task", 4)],
        );

        assert_eq!(
            operation_text(&operation),
            "async observe_task %4 -> <result> completion <completion> cancel=false"
        );
    }

    #[test]
    fn message_less_assertions_have_no_placeholder_operand() {
        let operation = operation(
            "panic_report",
            vec![attribute("cause", "assertion")],
            Vec::new(),
        );

        assert_eq!(operation_text(&operation), "panic_report assertion");
    }
}
