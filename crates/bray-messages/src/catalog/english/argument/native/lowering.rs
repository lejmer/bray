pub(super) fn lowering_plan_name(plan: &str) -> &'static str {
    match plan {
        "analysis" => "execution analysis",
        "suspension" => "suspension",
        "task_operation" => "task operation",
        "scope_exit" => "scope exit",
        "storage_disposition" => "storage disposition",
        "cancellation_phase" => "cancellation phase",
        "lifecycle_phase" => "lifecycle phase",
        _ => "lowering input",
    }
}

pub(super) fn lowering_plan_failure_state(cause: &str) -> &'static str {
    match cause {
        "missing" => "missing",
        "duplicate" => "duplicated",
        "unexpected" => "outside the expected plan set",
        "recovered" => "incomplete after error recovery",
        "unavailable_root_access" => "missing the root storage access required for cleanup",
        "unavailable_cleanup_shape" => "missing the type's cleanup requirements",
        "unavailable_partial_cleanup" => {
            "missing cleanup for partially initialized or moved storage"
        }
        "unavailable_cleanup_order" => "blocked by cyclic lifecycle dependencies",
        "contradictory" => "inconsistent with the checked program",
        "out_of_order" => "in the wrong semantic order",
        _ => "invalid",
    }
}
