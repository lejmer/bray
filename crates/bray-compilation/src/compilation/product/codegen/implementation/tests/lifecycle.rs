use super::support::artifacts::{generated_artifacts, generated_artifacts_of_kind_with_options};
use super::support::runtime::{
    runtime_native_plan, runtime_native_plan_for_product,
    runtime_native_plan_for_sources_target_with_platform_overrides,
};
use crate::SelectedTarget;

use bray_codegen::{BackendArtifactKind, OptimizationLevel};
use bray_ir::{MirOperationKind, MirTerminatorKind, MirUnitKey};
use bray_symbols::ProductKind;

#[test]
fn scoped_selected_calls_emit_native_artifacts() {
    for (enter, exit, result, returned) in [
        (
            "enter() -> bool { return true; }",
            "exit(pos lease: bool) {}",
            "bool",
            "value",
        ),
        (
            "enter() -> Result<bool, i32> { return Ok(true); }",
            "exit(pos lease: bool) -> Result<unit, i32> { return Ok(unit); }",
            "Result<bool, i32>",
            "try value",
        ),
    ] {
        let source = format!(
            r#"
            module app;
            struct Resource {{ tag: bool; }}
            impl Resource {{ {enter} {exit} }}
            func scoped(pos resource: Resource) -> {result}
            {{
                return with lease = resource {{ yield lease; }};
            }}
            func main() -> Result<unit, i32>
            {{
                let resource: Resource = {{ tag = true }};
                let value = scoped(resource);
                let _ = {returned};

                return Ok(unit);
            }}
        "#
        );

        let (backend, plan) = runtime_native_plan(&source);

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn nested_cleanup_native_fixture_emits() {
    let (backend, plan) = runtime_native_plan_for_product(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/composition/cleanup/main.bray"
        )),
        ProductKind::Test,
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn custom_storage_cleanup_native_fixture_emits() {
    let (backend, plan) = runtime_native_plan(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    )));

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn buffer_destruction_emits_ordinary_and_abandoned_element_paths() {
    let source = r#"
        module std.test;
        using internal std.memory;

        struct Resource
        {
            value: i32;

            finalize() {}
            destruct() {}
        }

        func dispose(pos buffer: std.memory.RawBuffer<Resource>) {}
        func dispose_bytes(pos buffer: std.memory.RawBuffer<u8>) {}
    "#;

    let (backend, codegen) = super::support::compilation::codegen_configuration();

    let package = bray_symbols::PackageIdentity::try_new("std").unwrap();

    let request = crate::CompilationRequest::with_options(
        package.clone(),
        [
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/std.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/memory.bray"
            )),
            crate::test_support::RUNTIME_MEMORY_SOURCE,
            crate::test_support::RUNTIME_TEXT_SOURCE,
            source,
        ]
        .iter()
        .enumerate()
        .map(|(index, source)| {
            crate::test_support::source_input(source, u32::try_from(index).unwrap())
        })
        .collect(),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        ),
    )
    .with_standard_library_source_authority();

    let compilation = crate::Compilation::load_with_codegen(request, codegen).unwrap();

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let archive = bray_testing::TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = super::support::runtime::runtime_artifact(&compilation, archive.path());
    let product = bray_symbols::ProductIdentity::try_new(package, "buffer").unwrap();

    let plan = compilation
        .native_product_plan(
            product,
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            None,
        )
        .unwrap();

    let mut observed = 0;

    for mir in plan
        .units()
        .iter()
        .flat_map(bray_codegen::CodegenUnit::mir_units)
    {
        for operation in mir.operations() {
            let MirOperationKind::Memory(memory) = operation.kind() else {
                continue;
            };

            let bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { element } =
                memory.kind()
            else {
                continue;
            };

            assert!(memory.abandoned_cleanup().is_some());

            assert!(operation.kind().helper_references().contains(
                &bray_ir::MirHelperReference::Destroy(
                    element,
                    bray_ir::MirDestructionDisposition::Dynamic
                )
            ));

            observed += 1;
        }
    }

    assert_eq!(observed, 2);
    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn replacement_cleanup_helpers_emit_checked_outcomes() {
    for lifecycle in ["destruct() {}", "finalize() {} destruct() {}"] {
        let source = format!(
            "module app; struct Resource {{ value: i32; {lifecycle} }} \
                 func main() {{ let mut value: Resource = Resource {{ value = 1 }}; \
                 value = Resource {{ value = 2 }}; }}"
        );

        let (backend, plan) = runtime_native_plan(&source);

        for mir in plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
        {
            if !matches!(mir.key(), MirUnitKey::GeneratedLifecycle(_)) {
                continue;
            }

            for block in mir.blocks() {
                let Some(operation) = block.operations().last().and_then(|id| mir.operation(*id))
                else {
                    continue;
                };

                if matches!(operation.kind(), MirOperationKind::Call(call) if call.may_propagate_panic())
                {
                    assert!(matches!(
                        block.terminator().kind(),
                        MirTerminatorKind::CheckCallOutcome { .. }
                    ));
                }
            }
        }

        for symbol in plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
        {
            let bray_codegen::CodegenSymbolKey::Instance(instance) = symbol.key() else {
                continue;
            };

            let MirUnitKey::GeneratedLifecycle(lifecycle) = instance.template() else {
                continue;
            };

            let signature = symbol.signature();

            assert!(!signature.has_panic_report_context());

            assert_eq!(
                signature.parameters().len(),
                if matches!(
                    lifecycle.role(),
                    bray_ir::MirGeneratedLifecycleRole::Destroy(
                        bray_ir::MirDestructionDisposition::Dynamic
                    )
                ) {
                    4
                } else {
                    3
                },
                "structural cleanup borrows the enclosing state and report"
            );
        }

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn library_source_bodies_are_not_skipped_by_compiler_known_names() {
    for module in ["app", "std.memory"] {
        let source = format!("module {module}; func allocate() -> i32 {{ return 3; }}");

        let (backend, plan) = runtime_native_plan_for_product(&source, ProductKind::Library);

        assert!(!plan.units().is_empty());
        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn synchronous_finalizer_can_propagate_run_result_cancellation() {
    let source = r#"
            module app;
            struct Guard {
                finalize() {
                    let outcome: RunResult<unit> = Cancelled;
                    try outcome;
                }
                destruct() {}
            }
            async func main() {
                let mut value = Guard {};
                value = Guard {};
            }
        "#;

    for configuration in [
        crate::BuildConfiguration::Development,
        crate::BuildConfiguration::Release,
    ] {
        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
            &[source],
            ProductKind::Executable,
            SelectedTarget::baseline(),
            &[],
            [],
            [],
            configuration,
        );

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn ordinary_async_finalizers_emit_admitted_frame_composition() {
    let (backend, plan) = runtime_native_plan(
        r#"
        module app;

        using core.atomic;

        static COUNT: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);
        async func record()
        {
            let previous = core.atomic.load<usize, 1>(&COUNT);
            core.atomic.store<usize, 4>(&COUNT, previous + 1);
        }
        struct Owner
        {
            value: usize;
            async finalize()
            {
                await record();
            }
        }
        func rejected() -> usize { panic("later input"); }
        async func accept(pos owner: Owner, number: usize = rejected()) {}
        async func completed() -> Owner { return Owner { value = 3 }; }
        async func main()
        {
            let mut owner = Owner { value = 1 };
            owner = Owner { value = 2 };
            let result = await completed();
            let failure = catch { await accept(Owner { value = 4 }); yield unit; };
        }
    "#,
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn inactive_future_cleanup_emits_without_entering_child_body() {
    let (backend, plan) = runtime_native_plan(
        r#"
        module app;
        struct Large
        {
            values: [u64; 1024];
        }

        struct Owner
        {
            value: u32;

            async finalize()
            {
                await release();
            }
        }

        async func release() {}

        async func child(pos owner: Owner) -> Large
        {
            panic("inactive body entered");
        }

        async func main()
        {
            let mut pending = child(Owner { value = 1 });
            pending = child(Owner { value = 2 });
            child(Owner { value = 3 });
        }
    "#,
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn retained_finalizer_errors_with_inactive_futures_emit_native_artifacts() {
    let source = r#"
        module app;

        struct Capture
        {
            destruct()
            {
            }
        }

        async func body(pos capture: Capture)
        {
            panic("inactive body entered");
        }

        struct Failure
        {
            pending: Future<unit>;

            finalize()
            {
                panic("retained error must abandon finalization");
            }

            destruct()
            {
            }
        }

        struct Owner
        {
            async finalize() -> Result<unit, Failure>
            {
                return Error(Failure { pending = body(Capture {}) });
            }

            destruct()
            {
            }
        }

        async func main()
        {
            let owner = Owner {};

            panic("original failure");
        }
        "#;

    let (backend, plan) = runtime_native_plan(source);

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn source_destructor_parts_reuse_the_owner_cleanup_continuation() {
    for (execution, finalizer) in [
        ("", "finalize() {}"),
        ("async ", "async finalize() { await pause(); }"),
    ] {
        let source = format!(
            r#"
            module app;

            async func pause()
            {{
            }}

            struct Child
            {{
                value: u32;

                {finalizer}
            }}

            struct Owner
            {{
                child: Child;

                destruct()
                {{
                }}
            }}

            {execution}func main()
            {{
                let owner = Owner
                {{
                    child = Child {{ value = 1 }}
                }};
            }}
            "#,
        );

        let (backend, plan) = runtime_native_plan(&source);

        if execution.is_empty() {
            assert!(
                plan.units()
                    .iter()
                    .flat_map(|unit| unit.instances())
                    .all(|instance| instance.mir().frame_descriptor().is_none()),
                "synchronous destructor parts do not add an async frame"
            );
        }

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
fn awaited_owners_retain_cleanup_before_the_enclosing_expression_accepts_them() {
    let (backend, plan) = runtime_native_plan(
        r#"
        module app;
        async func pending() {}
        struct Failure
        {
            child: Future<unit>;
        }
        async func produce() -> Result<i32, Failure>
        {
            return Error(Failure { child = pending() });
        }
        async func main()
        {
            let outcome: Result<i32, Failure> = if true
            {
                yield await produce();
            }
            else
            {
                yield Ok(0);
            };

            match consume outcome
            {
                case Ok(_) {}
                case Error(_) {}
            }
        }
        "#,
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn scoped_exit_errors_with_inactive_captures_plan_native_cleanup() {
    for execution in ["", "async "] {
        for entry_execution in ["", "async "] {
            for (entry_type, entry_value) in [
                ("Capability", "capability"),
                ("Result<Capability, Failure>", "Ok(capability)"),
            ] {
                for body in ["panic(\"\");", "yield unit;"] {
                    let entry_await = if entry_execution.is_empty() {
                        ""
                    } else {
                        "await pending();"
                    };

                    let source = format!(
                        r#"
            module app;

            using core.atomic;
            static COUNT: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);
            async func pending() {{}}
            @layout(stable, align = 2048)
            struct Failure
            {{
                marker: usize;
                child: Future<unit>;
                finalize() {{}}
                destruct()
                {{
                    assert(self.marker == 619);
                }}
            }}
            struct Capability
            {{
                outcome: Result<unit, Failure>;
            }}
            struct Resource
            {{
                capability: {entry_type};
                {entry_execution}consume enter() -> {entry_type}
                {{
                    {entry_await}
                    core.atomic.fetch_add<usize, 0>(&COUNT, value = 1);
                    return self.capability;
                }}
                {execution}exit(pos lease: Capability) -> Result<unit, Failure>
                {{
                    return lease.outcome;
                }}
            }}
            async func main()
            {{
                let _ = catch
                {{
                    let capability = Capability
                    {{
                        outcome = Error(Failure {{ marker = 619, child = pending() }})
                    }};
                    let resource = Resource {{ capability = {entry_value} }};
                    let _ = with lease = resource
                    {{
                        {body}
                    }};

                    yield unit;
                }};
            }}
            "#,
                    );

                    let (_, plan) = runtime_native_plan(&source);

                    for instance in plan.units().iter().flat_map(|unit| unit.instances()) {
                        for operation in instance.mir().operations() {
                            let call = match operation.kind() {
                                MirOperationKind::Call(call)
                                | MirOperationKind::Async(
                                    bray_ir::MirAsyncOperation::CreateFrame {
                                        initializer: bray_ir::MirFrameInitializer::Callable(call),
                                        ..
                                    },
                                ) => call,
                                _ => continue,
                            };

                            let bray_ir::MirCallTarget::Direct(reference) = call.target() else {
                                continue;
                            };

                            if reference.instance().definition().callable_symbol().kind()
                                != bray_symbols::SymbolKind::ScopeExit
                            {
                                continue;
                            }

                            assert!(instance.mir().operations().iter().any(|operation| matches!(
                                operation.kind(),
                                MirOperationKind::AdmitOutgoing {
                                    owner: bray_ir::MirOutgoingOwner::Invocation { callable, result }, ..
                                } if callable == reference && *result == call.result()
                            )), "selected exit owns admission before it can run");
                        }
                    }

                    assert!(
                        !plan.units().is_empty(),
                        "{execution}, {entry_execution}, {entry_type}, {body}"
                    );
                }
            }
        }
    }
}

#[test]
fn moving_aligned_union_values_does_not_clear_dead_payload_bytes() {
    let (backend, plan) = runtime_native_plan_for_product(
        r#"
        module app;
        @layout(stable, align = 2048)
        struct Payload
        {
            marker: usize;
        }
        func transfer(pos value: Result<unit, Payload>) -> Result<unit, Payload>
        {
            return value;
        }
        "#,
        ProductKind::Library,
    );

    let options = plan.options().with_optimization(OptimizationLevel::None);

    let artifacts = generated_artifacts_of_kind_with_options(
        &backend,
        &plan,
        BackendArtifactKind::BackendIr,
        &options,
    );

    let text = artifacts
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes))
        .collect::<String>();

    assert!(text.contains("@llvm.memmove"), "{text}");
    assert!(!text.contains("load %bray.union."), "{text}");

    assert!(
        !text.lines().any(|line| {
            line.trim_start().starts_with("store %bray.union.") && line.contains("zeroinitializer")
        }),
        "{text}"
    );
}
