use super::support::fixtures::custom_index_storage_compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{BoundDependencySubject, RefinementKind, StorageAccessPurpose};
use bray_diagnostics::{
    DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticRelatedLocationKind,
    DiagnosticStorageProjection, DiagnosticStorageRoot,
};
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn storage_flow_rejects_mutation_without_parameter_authority() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32)\n",
        "{\n",
        "    value = 2;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(analysis.value().operations().iter().any(|operation| {
        operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
    }));

    assert!(
        analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingMissingMutationAuthority)
            .next()
            .is_some()
    );

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingMissingMutationAuthority,
    );
}

#[test]
fn storage_flow_rejects_mutable_borrow_without_parameter_authority() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32)\n",
        "{\n",
        "    let borrowed: & mut i32 = & mut value;\n",
        "    borrowed;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(analysis.value().operations().iter().any(|operation| {
        operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
    }));

    assert!(
        analysis
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingMissingMutationAuthority)
            .next()
            .is_some()
    );
}

#[test]
fn storage_flow_accepts_mutation_through_mutable_borrow_parameter() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: & mut i32)\n",
        "{\n",
        "    value = 2;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(
        analysis.value().operations().iter().all(|operation| {
            !matches!(
                operation.status(),
                bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
                    | bray_bound_tree::StorageOperationStatus::ConflictingBorrow
            )
        }),
        "{analysis:?}"
    );
}

#[test]
fn storage_flow_rejects_mutation_through_shared_borrow_parameter() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: &i32)\n",
        "{\n",
        "    value = 2;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(analysis.value().operations().iter().any(|operation| {
        operation.status() == bray_bound_tree::StorageOperationStatus::MissingMutationAuthority
    }));
}

#[test]
fn storage_flow_rejects_use_after_move() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Resource\n",
        "{\n",
        "    value: i32;\n",
        "}\n",
        "func main(resource: Resource)\n",
        "{\n",
        "    let moved: Resource = resource;\n",
        "    resource;\n",
        "    moved;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(
        analysis
            .value()
            .operations()
            .iter()
            .any(|operation| operation.status() == bray_bound_tree::StorageOperationStatus::Moved),
        "{analysis:?}"
    );

    let diagnostic = analysis
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
        .next()
        .unwrap_or_else(|| panic!("use after move must produce a diagnostic"));

    bray_testing::assert_goal_state_diagnostic(diagnostic);

    assert_goal_state_diagnostic_kind(
        analysis.diagnostics(),
        DiagnosticKind::CheckingUseOfMovedStorage,
    );

    assert!(
        diagnostic
            .related_locations()
            .iter()
            .any(|location| location.kind() == DiagnosticRelatedLocationKind::MoveOrigin)
    );
}

#[test]
fn storage_flow_rejects_moving_a_value_through_a_shared_borrow() {
    let compilation = custom_index_storage_compilation(
        r#"func exercise(pos values: Values)
{
    let moved: Item = values[0];
    moved;
}
"#,
    );

    let flow = compilation
        .storage_flow(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("borrowed move storage flow must publish: {error:?}"));

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingMissingStorageOwnership,
    );

    let diagnostic = flow
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingMissingStorageOwnership)
        .next()
        .unwrap_or_else(|| panic!("custom-index move must retain its diagnostic"));

    let access = diagnostic
        .args()
        .iter()
        .find_map(|arg| match (arg.name(), arg.value()) {
            (DiagnosticArgName::StorageAccess, DiagnosticArgValue::StorageAccess(access)) => {
                Some(access)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("custom-index move must retain its exact access"));

    assert_eq!(access.root(), DiagnosticStorageRoot::BorrowedStorage);
}

#[test]
fn storage_flow_reports_every_branch_move_origin() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Resource\n",
        "{\n",
        "    value: i32;\n",
        "}\n",
        "func main(pos condition: bool, pos resource: Resource)\n",
        "{\n",
        "    if condition\n",
        "    {\n",
        "        let first: Resource = resource;\n",
        "        first;\n",
        "    }\n",
        "    else\n",
        "    {\n",
        "        let second: Resource = resource;\n",
        "        second;\n",
        "    }\n",
        "    resource;\n",
        "}\n",
    ));

    let diagnostic = compilation
        .check_diagnostics()
        .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
        .next()
        .unwrap_or_else(|| panic!("branch moves must produce a use-after-move diagnostic"));

    assert_eq!(diagnostic.related_locations().len(), 2, "{diagnostic:#?}");
    bray_testing::assert_goal_state_diagnostic(diagnostic);
}

#[test]
fn storage_flow_diagnostics_retain_nested_projection_paths() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Resource { value: i32; }\n",
        "struct Container { inner: Resource; }\n",
        "func main(pos container: Container)\n",
        "{\n",
        "    let moved: Resource = container.inner;\n",
        "    container.inner;\n",
        "    moved;\n",
        "}\n",
    ));

    let flow = compilation
        .storage_flow(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("nested storage flow must publish: {error:?}"));

    let diagnostic = flow
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
        .next()
        .unwrap_or_else(|| panic!("nested use after move must be diagnosed"));

    let access = diagnostic
        .args()
        .iter()
        .find_map(|arg| match (arg.name(), arg.value()) {
            (DiagnosticArgName::StorageAccess, DiagnosticArgValue::StorageAccess(access)) => {
                Some(access)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("nested use must retain its exact storage access"));

    assert!(access.projections().iter().any(|projection| {
        matches!(projection, DiagnosticStorageProjection::ProductField(name) if name == "inner")
    }));

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingUseOfMovedStorage,
    );
}

#[test]
fn storage_flow_reports_every_conflicting_borrow_origin() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let mut value: i32 = 1;\n",
        "    let first: &i32 = &value;\n",
        "    let second: &i32 = &value;\n",
        "    let exclusive: & mut i32 = & mut value;\n",
        "    first;\n",
        "    second;\n",
        "    exclusive;\n",
        "}\n",
    ));

    let diagnostic = compilation
        .check_diagnostics()
        .by_kind(DiagnosticKind::CheckingConflictingBorrow)
        .next()
        .unwrap_or_else(|| panic!("overlapping borrows must produce a diagnostic"));

    assert_eq!(diagnostic.related_locations().len(), 2, "{diagnostic:#?}");
    bray_testing::assert_goal_state_diagnostic(diagnostic);
}

#[test]
fn storage_flow_allows_assignment_to_reinitialize_moved_storage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Resource\n",
        "{\n",
        "    value: i32;\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let mut resource: Resource = Resource { value = 1 };\n",
        "    let moved: Resource = resource;\n",
        "    resource = Resource { value = 2 };\n",
        "    resource;\n",
        "    moved;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(
        analysis.value().operations().iter().all(|operation| {
            !matches!(
                operation.status(),
                bray_bound_tree::StorageOperationStatus::Uninitialized
                    | bray_bound_tree::StorageOperationStatus::Moved
            )
        }),
        "{analysis:?}"
    );
}

#[test]
fn storage_flow_copies_copyable_value_transfers() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32) -> i32\n",
        "{\n",
        "    let copied: i32 = value;\n",
        "    value;\n",
        "    return copied;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.storage_flow(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("storage-flow checking must publish: {error:?}"),
    };

    assert!(
        analysis
            .value()
            .operations()
            .iter()
            .any(|operation| operation.purpose() == StorageAccessPurpose::Copy),
        "{analysis:?}"
    );

    assert!(
        analysis
            .value()
            .operations()
            .iter()
            .all(|operation| operation.status() != bray_bound_tree::StorageOperationStatus::Moved),
        "{analysis:?}"
    );
}

#[test]
fn liveness_converges_conservatively_across_branches_and_loops() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(pos condition: bool, pos value: i32) -> i32\n",
        "{\n",
        "    let selected: i32 = if condition\n",
        "    {\n",
        "        yield value;\n",
        "    }\n",
        "    else\n",
        "    {\n",
        "        yield value;\n",
        "    };\n",
        "    loop\n",
        "    {\n",
        "        if condition\n",
        "        {\n",
        "            break;\n",
        "        }\n",
        "        value;\n",
        "    }\n",
        "    return selected;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.liveness(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("cyclic liveness analysis must converge: {error:?}"),
    };

    assert!(!analysis.value().last_uses().is_empty(), "{analysis:#?}");
}

#[test]
fn refinements_are_lazy_cached_and_retain_branch_conditions() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(pos condition: bool)\n",
        "{\n",
        "    if condition\n",
        "    {\n",
        "        condition;\n",
        "    }\n",
        "    else\n",
        "    {\n",
        "        condition;\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.body_semantics.is_published(&key),
        Ok(false)
    );

    let first = match compilation.refinements(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("refinement analysis must publish: {error:?}"),
    };

    assert!(
        first.value().occurrences().iter().any(|occurrence| {
            occurrence.refinements().iter().any(|refinement| {
                matches!(
                    refinement.kind(),
                    RefinementKind::Condition { value: true, .. }
                )
            })
        }),
        "{first:?}"
    );

    let second = match compilation.refinements(key.clone()) {
        Ok(analysis) => analysis,
        Err(error) => panic!("repeated refinement analysis must publish: {error:?}"),
    };

    assert_eq!(first.snapshot_address(), second.snapshot_address());

    let dependencies = match compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::BodySemantics(key.clone()))
    {
        Ok(Some(dependencies)) => dependencies,
        Ok(None) => panic!("published refinements must retain dependencies"),
        Err(error) => panic!("refinement dependencies must be readable: {error:?}"),
    };

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::CheckedPatterns(
            key.clone()
        ))
    );

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::StoragePlan(key)));
}

#[test]
fn liveness_retains_storage_required_beyond_nested_scope_exits() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32) -> i32\n",
        "{\n",
        "    {\n",
        "        value;\n",
        "    }\n",
        "    return value;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let analysis = match compilation.liveness(key) {
        Ok(analysis) => analysis,
        Err(error) => panic!("scope liveness analysis must publish: {error:?}"),
    };

    assert!(
        analysis
            .value()
            .live_across_scopes()
            .iter()
            .any(|entry| { matches!(entry.subject(), BoundDependencySubject::Storage(_)) })
    );
}
