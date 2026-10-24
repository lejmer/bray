use super::support::fixtures::custom_index_storage_compilation;
use crate::test_support::{compilation, source_callable_body_key};
use bray_bound_tree::{BoundDependencySubject, RefinementKind, StorageAccessPurpose};
use bray_diagnostics::{
    DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticRelatedLocationKind,
    DiagnosticStorageProjection, DiagnosticStorageRoot,
};
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn normal_reports_follow_calls_and_consumed_futures() {
    for (execution, invocation) in [
        ("", "collect(resource)"),
        ("async ", "await collect(resource)"),
    ] {
        let source = r#"
        module app;
        struct Value
        {
            value: bool;
            func read() -> bool
            {
                return self.value;
            }
        }
        struct Failure
        {
            value: &Value;
            destruct()
            {
                assert(self.value.read());
            }
        }
        struct Resource
        {
            value: Value;
            enter() -> &Value
            {
                return &self.value;
            }
            exit(pos lease: &Value) -> Result<unit, Failure>
            {
                return Error(Failure { value = lease });
            }
        }
        Executionfunc collect(pos resource: &Resource) -> Result<unit, Failure>
        {
            _ = with lease = resource
            {
                return Ok(unit);
            };
            return Ok(unit);
        }
        async func caller(pos resource: &Resource)
        {
            _ = Invocation;
        }
        "#
        .replace("Execution", execution)
        .replace("Invocation", invocation);

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{execution}: {diagnostics:#?}");

        for name in ["collect", "caller"] {
            let summary = crate::compilation::result_dependencies::source_function_dependencies(
                &compilation,
                name,
            );

            assert!(
                summary
                    .completion_reports()
                    .is_some_and(|requirements| !requirements.is_empty()),
                "{execution}{name}: {summary:#?}"
            );

            let key = crate::test_support::source_function_body_key(&compilation, name);

            let lowered = compilation
                .lowered_unit(key)
                .expect("report-producing body lowers");

            assert!(
                !lowered.diagnostics().has_errors(),
                "{:#?}",
                lowered.diagnostics()
            );

            let unit = lowered
                .value()
                .as_ref()
                .expect("checked report-producing body lowers")
                .mir()
                .expect("checked report-producing body has MIR");

            assert!(
                unit.is_valid(),
                "{execution}{name} preserves MIR ownership and edge contracts"
            );

            assert!(
                unit.blocks().iter().any(|block| matches!(
                    block.terminator().kind(),
                    bray_ir::MirTerminatorKind::Return {
                        report: Some(_),
                        ..
                    }
                )) || unit.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    bray_ir::MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::PublishTerminalState {
                            state: bray_ir::MirTaskTerminalState::Completed {
                                report: Some(_),
                                ..
                            },
                            ..
                        }
                    )
                )),
                "{execution}{name} publishes its retained reports"
            );
        }
    }
}

#[test]
fn retained_scoped_exit_errors_keep_borrowed_origins_alive() {
    for (parameter, escapes) in [("Resource", true), ("&Resource", false)] {
        let source = r#"
        module app;
        struct Value
        {
            value: bool;
            func read() -> bool { return self.value; }
        }
        struct Failure
        {
            value: &Value;
            destruct() { assert(self.value.read()); }
        }
        struct Resource
        {
            value: Value;
            enter() -> &Value { return &self.value; }
            exit(pos lease: &Value) -> Result<unit, Failure>
            {
                return Error(Failure { value = lease });
            }
        }
        async func caller(pos resource: ResourceInput) -> Result<unit, Failure>
        {
            _ = with lease = resource
            {
                return Ok(unit);
            };
            return Ok(unit);
        }
        "#
        .replace("ResourceInput", parameter);

        let compilation = compilation(&source);
        let diagnostics = compilation.check_diagnostics();

        assert_eq!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind()
                    == DiagnosticKind::CheckingEscapingStorageDependency),
            escapes,
            "{parameter}: {diagnostics:#?}"
        );

        if !escapes {
            assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
        }
    }
}

#[test]
fn callable_reports_preserve_borrows_absent_from_the_explicit_result_type() {
    for known in [false, true] {
        for local in [false, true] {
            let source = r#"
            module app;
            struct Value<T> { values: [T; 2]; }
            union Failure { Unavailable; }
            struct Incident { value: &bool; }
            struct Resource
            {
                value: &bool;
                enter() -> &bool
                {
                    return self.value;
                }
                exit(pos lease: &bool) -> Result<unit, Incident>
                {
                    return Error(Incident { value = lease });
                }
            }
            func retain(pos input: &bool) -> Result<Value<bool>, Failure>
            {
                let resource = Resource { value = input };
                _ = with lease = resource
                {
                    return Ok({ values = [true, true] });
                };
                return Ok({ values = [true, true] });
            }
            func invoke(pos produce: func(pos input: &bool) -> Result<Value<bool>, Failure>,
                pos input: &bool) -> Result<Value<bool>, Failure>
            {
                let operation: func(pos input: &bool) -> Result<Value<bool>, Failure> = retain;
                Prologue
                return Callee(Argument);
            }
            func caller(pos input: &bool) -> Result<Value<bool>, Failure>
            {
                return invoke(retain, input);
            }
            "#
            .replace(
                "Prologue",
                if local { "let local: bool = true;" } else { "" },
            )
            .replace("Callee", if known { "operation" } else { "produce" })
            .replace("Argument", if local { "&local" } else { "input" });

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            let producer = crate::compilation::result_dependencies::source_function_dependencies(
                &application,
                "retain",
            );

            assert!(
                producer
                    .completion_reports()
                    .is_some_and(|requirements| !requirements.is_empty()),
                "a callback can retain a borrowed Incident despite its explicit Failure type: {producer:#?}, {diagnostics:#?}"
            );

            assert_eq!(
                diagnostics.has_errors(),
                local,
                "known={known}, local={local}: {diagnostics:#?}"
            );

            if local {
                assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn operator_reports_preserve_the_same_operand_borrows_as_method_calls() {
    let callers = [
        (
            r#"
            func caller(pos first: Value, pos second: Value) -> Value
            {
                return Invocation;
            }
            "#,
            Some(DiagnosticKind::CheckingEscapingStorageDependency),
        ),
        (
            r#"
            func caller()
            {
                let mut first = Value { value = true };
                let second = Value { value = false };
                {
                    _ = catch
                    {
                        _ = Invocation;
                        first.value = false;
                        panic("");
                        yield unit;
                    };
                }
            }
            "#,
            Some(DiagnosticKind::CheckingConflictingBorrow),
        ),
        (
            r#"
            func caller()
            {
                let mut first = Value { value = true };
                let second = Value { value = false };
                {
                    _ = catch
                    {
                        _ = Invocation;
                        panic("");
                        yield unit;
                    };
                }
                first.value = false;
            }
            "#,
            None,
        ),
    ];

    for invocation in ["first.add(&second)", "first + second"] {
        for (caller, expected) in callers {
            let source = r#"
        module app;
        struct Value
        {
            mut value: bool;
        }
        struct Incident
        {
            value: &bool;
        }
        struct Resource
        {
            value: &bool;
            enter() -> &bool
            {
                return self.value;
            }
            exit(pos lease: &bool) -> Result<unit, Incident>
            {
                return Error(Incident { value = lease });
            }
        }
        impl ValueAdd = Value(Add<Value>)
        {
            type Output = Value;
            func add(pos rhs: &Value) -> Value
            {
                let resource = Resource { value = &self.value };
                _ = with lease = resource
                {
                    return Value { value = rhs.value };
                };
                return Value { value = rhs.value };
            }
        }
        Caller
        "#
            .replace("Caller", caller)
            .replace("Invocation", invocation);

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            assert_eq!(
                diagnostics.has_errors(),
                expected.is_some(),
                "{invocation}: {diagnostics:#?}"
            );

            if let Some(expected) = expected {
                // A future conflicting write can also make the retained output
                // inadmissible at invocation, before storage flow reaches that write.
                if expected == DiagnosticKind::CheckingConflictingBorrow
                    && diagnostics.iter().any(|diagnostic| {
                        diagnostic.kind() == DiagnosticKind::CheckingEscapingStorageDependency
                    })
                {
                    assert_goal_state_diagnostic_kind(
                        &diagnostics,
                        DiagnosticKind::CheckingEscapingStorageDependency,
                    );
                } else {
                    assert_goal_state_diagnostic_kind(&diagnostics, expected);
                }
            }
        }
    }
}

#[test]
fn conversion_reports_retain_borrows_across_composite_inputs() {
    for invocation in [
        "source as Target",
        "(source, second) as (Target, Target)",
        "[source, second] as [Target; 2]",
        "present as Target?",
    ] {
        for local in [false, true] {
            let input = if local {
                "let value = true; let input = &value;"
            } else {
                ""
            };

            let parameter = if local { "" } else { "pos input: &bool" };

            let source = r#"
            module app;
            struct Source
            {
                value: &bool;
            }
            struct Target
            {
                value: &bool;
            }
            struct Incident
            {
                value: &bool;
            }
            struct Resource
            {
                value: &bool;
                enter() -> &bool
                {
                    return self.value;
                }
                exit(pos lease: &bool) -> Result<unit, Incident>
                {
                    return Error(Incident { value = lease });
                }
            }
            impl SourceConversion = Source(ConvertTo<Target>)
            {
                consume func convert() -> Target
                {
                    let resource = Resource { value = self.value };
                    _ = with lease = resource
                    {
                        return Target { value = self.value };
                    };
                    return Target { value = self.value };
                }
            }
            func caller(Parameter)
            {
                Input
                let source = Source { value = input };
                let second = Source { value = input };
                let present: Source? = Source { value = input };
                _ = Invocation;
            }
            "#
            .replace("Parameter", parameter)
            .replace("Input", input)
            .replace("Invocation", invocation);

            let application = compilation(&source);
            let diagnostics = application.check_diagnostics();

            if diagnostics.has_errors() != local {
                let summary = crate::compilation::result_dependencies::source_function_dependencies(
                    &application,
                    "caller",
                );

                let contracts = application
                    .dependency_contracts(crate::test_support::source_function_body_key(
                        &application,
                        "caller",
                    ))
                    .expect("checked caller publishes dependency contracts");

                let reports = contracts
                    .value()
                    .calls()
                    .iter()
                    .map(|(site, call)| (site, call.reports()))
                    .collect::<Vec<_>>();

                panic!(
                    "{invocation}, local={local}: {diagnostics:#?}, summary={summary:#?}, reports={reports:#?}"
                );
            }

            if local {
                assert_goal_state_diagnostic_kind(
                    &diagnostics,
                    DiagnosticKind::CheckingEscapingStorageDependency,
                );
            }
        }
    }
}

#[test]
fn custom_index_reports_retain_the_selector_borrow() {
    let application = compilation(
        r#"
    module app;
    struct Indexed
    {
        value: i32;
    }
    struct Incident
    {
        value: &i32;
    }
    struct Resource
    {
        value: &i32;
        enter() -> &i32
        {
            return self.value;
        }
        exit(pos lease: &i32) -> Result<unit, Incident>
        {
            return Error(Incident { value = lease });
        }
    }
    impl ElementAccess = Indexed(ElementIndex<i32>)
    {
        type Output = i32;
        func index(pos selector: &i32) -> &i32
        {
            let resource = Resource { value = selector };
            _ = with lease = resource
            {
                return &self.value;
            };
            return &self.value;
        }
    }
    func caller(pos indexed: &Indexed, pos selector: i32) -> i32
    {
        return indexed[selector];
    }
    "#,
    );

    let diagnostics = application.check_diagnostics();

    if !diagnostics.has_errors() {
        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            "caller",
        );

        let contracts = application
            .dependency_contracts(crate::test_support::source_function_body_key(
                &application,
                "caller",
            ))
            .expect("checked caller publishes dependency contracts");

        let reports = contracts
            .value()
            .calls()
            .iter()
            .map(|(site, call)| (site, call.reports()))
            .collect::<Vec<_>>();

        panic!(
            "selector report must retain its caller-owned storage, summary={summary:#?}, reports={reports:#?}"
        );
    }

    assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::CheckingEscapingStorageDependency,
    );
}

#[test]
fn repeated_scoped_exit_paths_preserve_later_parameter_uses() {
    let compilation = compilation(
        r#"
        module app;
        struct Failure
        {
            pending: Future<unit>;
        }
        struct Capability
        {
            outcome: Result<unit, Failure>;
        }
        async func pending() {}
        async func prepare(pos first: bool, pos second: bool) -> Result<Capability, Failure>
        {
            if first
            {
                return Error(Failure { pending = pending() });
            }

            let outcome: Result<unit, Failure> = if second
            {
                yield Error(Failure { pending = pending() });
            }
            else
            {
                yield Ok(unit);
            };

            return Ok(Capability { outcome = outcome });
        }
        struct Scope
        {
            entry: Result<Capability, Failure>;
            consume enter() -> Result<Capability, Failure>
            {
                return self.entry;
            }
            exit(pos capability: Capability) -> Result<unit, Failure>
            {
                return capability.outcome;
            }
        }
        async func caller(pos first: bool, pos second: bool)
        {
            let scope = Scope { entry = await prepare(first, second) };
            let outcome: Result<i32, Failure> = with capability = scope
            {
                yield 42;
            };

            match consume outcome
            {
                case Ok(value)
                {
                    assert(!first && !second && value == 42);
                }
                case Error(_)
                {
                    assert(first || second);
                }
            }
        }
        "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

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

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone()
        ))
    );

    let dependencies = compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::ResultDependencyComponent(
            key.clone(),
        ))
        .unwrap()
        .expect("the shared component owns refinement prerequisites");

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

#[test]
fn native_callbacks_consume_only_their_implicit_report_outputs() {
    for native in [false, true] {
        let source = r#"
            module app;

            trusted func invoke(pos callback: NativeAbi func(pos value: u32) -> u32, pos value: u32) -> u32 NativeCapability
            {
                return callback(value);
            }
        "#.replace("NativeAbi", if native { "@abi(c)" } else { "" })
            .replace("NativeCapability", if native { "uses(foreign_call)" } else { "" });

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "native={native}: {diagnostics:#?}"
        );

        let contracts = application
            .dependency_contracts(crate::test_support::source_function_body_key(
                &application,
                "invoke",
            ))
            .unwrap();

        let calls = contracts.value().calls().values().collect::<Vec<_>>();

        let [call] = calls.as_slice() else {
            panic!("callback invocation retains its single call contract");
        };

        assert_eq!(call.reports().completed().is_none(), native);
        assert_eq!(call.reports().abnormal().is_none(), native);

        assert!(
            !call.result().requirements().is_empty(),
            "explicit result dependencies remain visible: {call:#?}"
        );
    }
}

#[test]
fn non_cancelling_calls_keep_panicking_reports_inside_the_catch_owner() {
    let application = compilation(
        r#"
        module app;
        struct Value
        {
            mut value: bool;
        }
        struct Incident
        {
            value: &bool;
        }
        struct Resource
        {
            value: &bool;
            enter() -> &bool
            {
                return self.value;
            }
            exit(pos lease: &bool) -> Result<unit, Incident>
            {
                return Error(Incident { value = lease });
            }
        }
        func collect(pos value: &Value, pos fail: bool)
        {
            let resource = Resource { value = &value.value };
            _ = with lease = resource
            {
                if fail
                {
                    panic("");
                }
                return;
            };
        }
        func caller()
        {
            let mut value = Value { value = true };
            {
                _ = catch
                {
                    collect(&value, true);
                    panic("");
                    yield unit;
                };
            }
            value.value = false;
        }
        "#,
    );

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
}

#[test]
fn optional_conversion_children_preserve_possible_normal_completion() {
    for (source, target, expected) in [
        ("Doomed?", "Target?", true),
        ("[Doomed; 0]", "[Target; 0]", true),
        ("[Doomed; 1]", "[Target; 1]", false),
        ("Doomed", "Target", false),
    ] {
        let source = r#"
        module app;
        struct Doomed {}
        struct Target {}
        impl DoomedConversion = Doomed(ConvertTo<Target>)
        {
            consume func convert() -> Target
            {
                panic("");
            }
        }
        func caller(pos value: Input) -> Output
        {
            return value as Output;
        }
        "#
        .replace("Input", source)
        .replace("Output", target);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{source}: {diagnostics:#?}");

        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            "caller",
        );

        assert_eq!(
            summary.normal_completion(),
            expected,
            "{source}: {summary:#?}"
        );
    }
}

#[test]
fn operation_cleanup_native_fixture_lowers_its_checked_inputs() {
    let application = compilation(include_str!(
        "../../../../../../../xtask/fixtures/native-execution/operation-cleanup.bray"
    ));

    let diagnostics = application.check_diagnostics();

    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");

    let key = crate::test_support::source_function_body_key(&application, "main");
    let lowered = application.lowered_unit(key).unwrap();

    let mir = lowered
        .value()
        .as_ref()
        .unwrap()
        .mir()
        .expect("checked fixture produces MIR");

    assert!(
        mir.is_valid(),
        "operation cleanup fixture must publish valid MIR: {mir:#?}"
    );

    let mut defaults = 0;

    for block in mir.blocks() {
        if block.operations().iter().any(|operation| {
            matches!(
                mir.operation(*operation).unwrap().kind(),
                bray_ir::MirOperationKind::Call(call)
                    if matches!(call.target(), bray_ir::MirCallTarget::DefaultValue { .. })
            )
        }) {
            defaults += 1;

            assert!(
                matches!(
                    block.terminator().kind(),
                    bray_ir::MirTerminatorKind::CheckCallOutcome {
                        completed_report: Some(_),
                        ..
                    }
                ),
                "each report-producing default retains its own checked input outcome"
            );
        }
    }

    assert_eq!(
        defaults, 3,
        "callable, struct and union defaults are covered"
    );
}

#[test]
fn explicit_conversions_consume_noncopyable_source_storage() {
    for (value, target) in [
        ("Source {}", "Target"),
        ("(Source {}, Source {})", "(Target, Target)"),
        ("[Source {}]", "[Target; 1]"),
        ("present()", "Target?"),
        ("Source {}", "Source"),
    ] {
        let source = r#"
        module app;
        struct Source
        {
            destruct() {}
        }
        struct Target {}
        impl Conversion = Source(ConvertTo<Target>)
        {
            consume func convert() -> Target
            {
                return Target {};
            }
        }
        func present() -> Source?
        {
            return Source {};
        }
        func caller()
        {
            let value = Input;
            let converted = value as Output;
            let moved_again = value;
        }
        "#
        .replace("Input", value)
        .replace("Output", target);

        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingUseOfMovedStorage),
            "conversion must consume its owned source: {source} {diagnostics:#?}"
        );
    }
}

#[test]
fn assignment_publication_distinguishes_owned_slots_from_borrowed_input_referents() {
    let compilation = compilation(
        r#"
            module app;

            struct Counter
            {
                mut count: u8;
            }

            func borrowed(pos value: &mut Counter)
            {
                value.count = 1;
            }

            func owned(pos mut value: Counter)
            {
                value.count = 1;
            }
        "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    for (name, borrowed) in [("borrowed", true), ("owned", false)] {
        let key = crate::test_support::source_function_body_key(&compilation, name);
        let bound = compilation.bound_unit(key.clone()).unwrap();
        let flow = compilation.storage_flow(key).unwrap();

        let assignment = bound
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, bray_bound_tree::BoundExpression::Assignment(_)).then_some(id)
            })
            .unwrap();

        let target = flow
            .value()
            .assignment_target(assignment)
            .expect("a settled assignment must retain its established origin");

        assert_eq!(
            matches!(
                target.origin,
                bray_bound_tree::StorageOrigin::Entry { layer: 1, .. }
            ),
            borrowed
        );

        assert_eq!(target.storage_path().is_none(), borrowed);
        assert!(!target.previous);
    }
}
