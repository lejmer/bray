use super::support::artifacts::{
    assert_source_emits_valid_native_units, generated_artifacts,
    generated_artifacts_of_kind_with_options,
};
use super::support::runtime::{
    runtime_native_plan, runtime_native_plan_for_product,
    runtime_native_plan_for_sources_target_with_platform_overrides,
};
use crate::SelectedTarget;

use bray_codegen::{BackendArtifactKind, OptimizationLevel};
use bray_ir::{MirOperationKind, MirTerminatorKind, MirUnitKey};
use bray_symbols::ProductKind;

#[test]
#[ignore = "tier:native"]
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
                _ = {returned};

                return Ok(unit);
            }}
        "#
        );

        let (backend, plan) = runtime_native_plan(&source);

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
fn custom_storage_cleanup_native_fixture_emits() {
    let (backend, plan) = runtime_native_plan(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../xtask/fixtures/native-execution/guarded-part-cleanup.bray"
    )));

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
#[ignore = "tier:native"]
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

        using core.atomic;
        static COUNT: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);

        async func record()
        {
            let previous = core.atomic.load<usize, 1>(&COUNT);
            core.atomic.store<usize, 4>(&COUNT, previous + 1);
        }

        struct Suspending
        {
            async finalize() { await record(); }
        }

        async func dispose_suspending(pos buffer: std.memory.RawBuffer<Suspending>) {}
        async func dispose_boxed(pos owner: box Suspending) {}
        async func dispose_tasks(pos buffer: std.memory.RawBuffer<Task<unit>>) {}
        async func dispose_futures(pos buffer: std.memory.RawBuffer<Future<unit>>) {}

        struct Sibling
        {
            task: Task<unit>;
            tail: Suspending;
        }

        struct Group
        {
            first: Task<unit>;
            middle: [Sibling; 2];
            last: Suspending;
        }

        async func dispose_groups(pos buffer: std.memory.RawBuffer<Group>) {}

        trusted async func construct_suspending() -> Result<unit, std.memory.MemoryLayoutError>
        {
            let buffer: std.memory.RawBuffer<Suspending> = try trusted std.memory.RawBuffer<Suspending>(capacity = 1);

            return Ok(unit);
        }

        struct Node
        {
            children: std.memory.RawBuffer<Node>;
            async finalize() { await record(); }
        }

        async func dispose_recursive(pos buffer: std.memory.RawBuffer<Node>) {}

        async func dispose_generic<T>(pos buffer: std.memory.RawBuffer<T>) {}
        async func dispose_generic_owner(pos buffer: std.memory.RawBuffer<Suspending>)
        {
            await dispose_generic<Suspending>(buffer);
        }

        trusted async func release_generic<T>(pos buffer: &mut std.memory.RawBuffer<T>)
            uses(manual_alloc)
        {
            trusted std.memory.raw_buffer_release<T>(buffer);
        }

        trusted async func release_generic_owner(pos buffer: &mut std.memory.RawBuffer<Suspending>)
        {
            await trusted release_generic<Suspending>(buffer);
        }

        trusted async func release_generic_bytes(pos buffer: &mut std.memory.RawBuffer<u8>)
        {
            await trusted release_generic<u8>(buffer);
        }

        trusted async func release_suspending(pos buffer: &mut std.memory.RawBuffer<Suspending>)
            uses(manual_alloc)
        {
            trusted std.memory.raw_buffer_release<Suspending>(buffer);
        }

        trusted async func replace_suspending(
            pos destination: &mut std.memory.RawBuffer<Suspending>,
            pos source: &mut std.memory.RawBuffer<Suspending>,
        )
            uses(manual_alloc)
        {
            trusted std.memory.raw_buffer_replace<Suspending>(destination, source);
        }
    "#;

    let (backend, codegen) = super::support::compilation::codegen_configuration();

    let package = bray_symbols::PackageIdentity::try_new("std")
        .expect("fixture package identity must be valid");

    let request = crate::CompilationRequest::with_options(
        package.clone(),
        [
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/std.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/memory/pointer.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/memory/storage.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/memory/allocation.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/numeric/limits.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/memory/buffer.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../standard-library/std/src/memory/function.bray"
            )),
            crate::test_support::RUNTIME_MEMORY_SOURCE,
            crate::test_support::RUNTIME_TEXT_SOURCE,
            source,
        ]
        .iter()
        .enumerate()
        .map(|(index, source)| {
            crate::test_support::source_input(
                source,
                u32::try_from(index).expect("fixture value must fit the requested representation"),
            )
        })
        .collect(),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        ),
    )
    .with_standard_library_source_authority();

    let compilation = crate::Compilation::load_with_codegen(request, codegen)
        .expect("fixture compilation must load for code generation");

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let archive = bray_testing::TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = super::support::runtime::runtime_artifact(&compilation, archive.path());

    let product = bray_symbols::ProductIdentity::try_new(package, "buffer")
        .expect("fixture product identity must be valid");

    let plan = compilation
        .native_product_plan(
            product,
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            None,
        )
        .expect("fixture product must reach native planning");

    let mut observed = std::collections::BTreeSet::new();
    let mut retained_array_prefix = false;

    for mir in plan
        .units()
        .iter()
        .flat_map(bray_codegen::CodegenUnit::mir_units)
    {
        if let Some(frame) = mir.frame_descriptor() {
            retained_array_prefix |= frame
                .states()
                .iter()
                .any(|state| broadcast_retains_nested_prefix(mir, state.retention().broadcast()));
        }

        for operation in mir.operations() {
            let MirOperationKind::Memory(memory) = operation.kind() else {
                continue;
            };

            let bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { element } =
                memory.kind()
            else {
                continue;
            };

            assert_eq!(
                operation.kind().helper_references(),
                [bray_ir::MirHelperReference::StandardLibrary(
                    bray_ir::MirStandardLibraryHelper::MemoryDeallocate,
                ),]
            );

            assert!(
                mir.operations()
                    .iter()
                    .any(|operation| matches!(operation.kind(),
                MirOperationKind::Memory(memory) if matches!(memory.kind(),
                    bray_bound_tree::CheckedMemoryOperationKind::RawBufferSetInitializedCount)))
            );

            observed.insert(element);
        }
    }

    assert_eq!(observed.len(), 7);

    assert!(
        retained_array_prefix,
        "waiting for a nested finalizer keeps the unconsumed array prefix broadcastable"
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

fn broadcast_retains_nested_prefix(mir: &bray_ir::MirUnit, entry: bray_ir::MirBlockId) -> bool {
    let mut pending = vec![entry];
    let mut visited = std::collections::BTreeSet::new();

    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }

        let block = mir.block(id).expect("fixture MIR block must exist");

        for operation in block.operations().iter().map(|id| {
            mir.operation(*id)
                .expect("fixture MIR operation must exist")
        }) {
            if let MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::TaskCancellation,
                place,
                ..
            } = operation.kind()
                && place
                    .projections()
                    .iter()
                    .filter(|projection| {
                        matches!(projection.kind(), bray_ir::MirProjectionKind::Index(_))
                    })
                    .count()
                    == 2
            {
                return true;
            }
        }

        block
            .terminator()
            .kind()
            .for_each_successor(|next| pending.push(next));
    }

    false
}

#[test]
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
fn library_source_bodies_are_not_skipped_by_compiler_known_names() {
    for module in ["app", "std.memory"] {
        let source = format!("module {module}; func allocate() -> i32 {{ return 3; }}");

        let (backend, plan) = runtime_native_plan_for_product(&source, ProductKind::Library);

        assert!(!plan.units().is_empty());
        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }
}

#[test]
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
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
            _ = child(Owner { value = 3 });
        }
    "#,
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
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
#[ignore = "tier:native"]
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
                    _ = core.atomic.fetch_add<usize, 0>(&COUNT, value = 1);
                    return self.capability;
                }}
                {execution}exit(pos lease: Capability) -> Result<unit, Failure>
                {{
                    return lease.outcome;
                }}
            }}
            async func main()
            {{
                _ = catch
                {{
                    let capability = Capability
                    {{
                        outcome = Error(Failure {{ marker = 619, child = pending() }})
                    }};
                    let resource = Resource {{ capability = {entry_value} }};
                    _ = with lease = resource
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
                                    owner: bray_ir::MirOutgoingOwner::Invocation { selection, result }, ..
                                } if selection.reference() == *reference && *result == call.result()
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
#[ignore = "tier:native"]
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

#[test]
#[ignore = "tier:native"]
fn constrained_async_admission_realizes_the_selected_implementation() {
    let source = r#"
        module app;

        trait Worker
        {
            async func run() -> u32;
        }

        struct Value { number: u32; }

        impl ValueWorker = Value(Worker)
        {
            async func run() -> u32 { return self.number; }
        }

        async func run_generic<T>(pos value: &T) -> u32
            with(T: Worker)
        {
            return await value.run();
        }

        async func main()
        {
            let value: Value = { number = 7 };
            let observed: u32 = await run_generic<Value>(&value);
            assert(observed == 7);
        }
    "#;

    let (_, plan) = runtime_native_plan(source);

    assert!(!plan.units().is_empty());
}

#[test]
#[ignore = "tier:native"]
fn admitted_synchronous_generic_cleanup_emits_native_units() {
    const SOURCE: &str = r#"
        module app;
        struct Owner { async finalize() executes(pure, total) {} }
        func discard<T>(pos value: T) {}
        struct Container<T, const Ready: bool> {
            value: T;
            async finalize() when(Ready) { executes(pure, total) } {}
        }
        func forward<T, const Ready: bool>(pos value: T) {
            let wrapped: Container<T, Ready && true> = { value = value };
            discard<Container<T, Ready && true>>(wrapped);
        }
        func recur<T, const N: i32>(pos value: T, pos stop: bool) {
            if stop { return; }
            recur<T, N + 0>(value, stop);
        }
        public func root() {
            discard<i32>(1);
            discard<Owner>(Owner {});
            forward<Owner, true>(Owner {});
            recur<i32, 0>(1, true);
        }
    "#;

    assert_source_emits_valid_native_units(SOURCE, crate::BuildConfiguration::Development);

    let dependency = super::support::dependencies::generic_dependency_from_fixture(
        true,
        false,
        super::support::dependencies::GenericDependencyFixture {
            source: SOURCE,
            runtime_frames: &[1, 1],
            executable_templates: 6,
            platform_service: None,
        },
    );

    let (backend, codegen) = super::support::compilation::codegen_configuration();

    let request = crate::CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(
            r#"
            module consumer;
            using example.dependency.app.root;
            public func forwarded()
            {
                example.dependency.app.root();
            }
            "#,
            0,
        )],
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        ),
    )
    .with_dependency_interfaces([dependency]);

    let consumer = crate::Compilation::load_with_codegen(request, codegen)
        .expect("fixture compilation must load for code generation");

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:?}",
        consumer.check_diagnostics()
    );

    let plan = consumer
        .native_product_plan(
            super::support::compilation::test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .expect("fixture product must reach native planning");

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
#[ignore = "tier:native"]
fn inactive_generic_scalar_future_emits_native_units() {
    assert_source_emits_valid_native_units(
        r#"
        module app;
        async func hold<T>(pos value: T) {}
        func discard<T>(pos value: T) { let pending = hold<T>(value); }
        func root() { discard<i32>(1); }
    "#,
        crate::BuildConfiguration::Development,
    );
}

#[test]
#[ignore = "tier:native"]
fn inactive_generic_captures_retain_synchronous_disposal_in_source_and_imports() {
    const SOURCE: &str = r#"
        module app;
        struct Synchronous {
            finalize() {}
            destruct() {}
        }
        struct Completed<const Ready: bool> {
            value: Synchronous;
            async finalize() when(Ready) { executes(pure, total) } {}
            destruct() {}
        }
        async func hold<T>(pos value: T) {}
        func discard<T>(pos value: T) {
            let pending = hold<T>(value);
        }
        func root() {
            discard<i32>(1);
            discard<Synchronous>(Synchronous {});
            let completed: Completed<true> = { value = Synchronous {} };
            discard<Completed<true>>(completed);
        }
    "#;

    let source =
        super::support::compilation::codegen_compilation_for_product(SOURCE, ProductKind::Library);

    let dependency = super::support::dependencies::generic_dependency_from_fixture(
        true,
        false,
        super::support::dependencies::GenericDependencyFixture {
            source: SOURCE,
            runtime_frames: &[1, 1],
            executable_templates: 7,
            platform_service: None,
        },
    );

    let (backend, codegen) = super::support::compilation::codegen_configuration();

    let request = crate::CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(
            r#"
            module consumer;
            using example.dependency.app.root;
            func forwarded() {
                example.dependency.app.root();
            }
            "#,
            0,
        )],
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        ),
    )
    .with_dependency_interfaces([dependency]);

    let consumer = crate::Compilation::load_with_codegen(request, codegen)
        .expect("fixture compilation must load for code generation");

    for (backend, compilation) in [source, (backend, consumer)] {
        for configuration in [
            crate::BuildConfiguration::Development,
            crate::BuildConfiguration::Release,
        ] {
            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );

            let plan = compilation
                .native_product_plan(
                    super::support::compilation::test_product_identity(),
                    configuration,
                    None,
                    [],
                    None,
                )
                .expect("fixture product must reach native planning");

            assert!(
                plan.units()
                    .iter()
                    .flat_map(|unit| unit.instances())
                    .flat_map(|instance| instance.mir().operations())
                    .any(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::ResolveInactiveFrame { .. }
                        )
                    ))
            );

            for instance in plan.units().iter().flat_map(|unit| unit.instances()) {
                let mir = instance.mir();

                for block in mir.blocks() {
                    if block.operations().iter().any(|id| {
                        matches!(
                            mir.operation(*id)
                                .expect("fixture `iter` call must succeed")
                                .kind(),
                            MirOperationKind::Async(
                                bray_ir::MirAsyncOperation::ResolveInactiveFrame { .. }
                            )
                        )
                    }) {
                        assert!(matches!(
                            block.terminator().kind(),
                            MirTerminatorKind::CheckCallOutcome {
                                completed_report: Some(_),
                                ..
                            }
                        ));
                    }
                }
            }

            assert!(plan.mappings().iter().flat_map(|mapping| mapping.symbols())
                .all(|symbol| !matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Runtime(runtime)
                    if runtime.role().service_class() == Some(bray_runtime_interface::RuntimeServiceClass::Execution))));

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }
}

#[test]
#[ignore = "tier:native"]
fn selected_associated_runtime_default_emits_source_and_imported_native_units() {
    const SOURCE: &str = r#"
        module app;
        func discard<T>(pos value: T) {}
        func demand<T>() -> i32 { let handler: func(pos value: T) = discard<T>; return 0; }
        trait Source {
            type Item;
            func select(value: i32 = demand<Item>()) {}
            func outer() { self.select(); }
        }
        struct Producer { marker: i32; }
        impl ProducerSource = Producer(Source) { type Item = i32; }
        using ProducerSource;
        impl Producer {
            type Scalar = i32;
            func alias(value: i32 = demand<Scalar>()) {}
        }
        public func root() {
            let producer = Producer { marker = 0 };
            producer.outer();
            producer.alias();
        }
    "#;

    const OVERRIDE: &str = r#"
        module app;
        func discard<T>(pos value: T) {}
        func demand<T>() -> i32 { let handler: func(pos value: T) = discard<T>; return 0; }
        trait Source {
            type Item;
            func select(value: i32 = demand<Item>()) {}
            func outer() { self.select(); }
        }
        struct Producer { marker: i32; }
        impl ProducerSource = Producer(Source) {
            type Item = i32;
            func select(value: i32 = demand<Item>()) {}
        }
        using ProducerSource;
        public func root() { let producer = Producer { marker = 0 }; producer.outer(); }
    "#;

    for (source, executable_templates) in [(SOURCE, 8), (OVERRIDE, 8)] {
        assert_source_emits_valid_native_units(source, crate::BuildConfiguration::Development);

        let dependency = super::support::dependencies::generic_dependency_from_fixture(
            true,
            false,
            super::support::dependencies::GenericDependencyFixture {
                source,
                runtime_frames: &[],
                executable_templates,
                platform_service: None,
            },
        );

        let (backend, codegen) = super::support::compilation::codegen_configuration();

        let request = crate::CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                r#"
            module consumer;
            using example.dependency.app.root;
            public func forwarded()
            {
                example.dependency.app.root();
            }
            "#,
                0,
            )],
            crate::CompilationOptions::new(
                crate::WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::baseline(),
            ),
        )
        .with_dependency_interfaces([dependency]);

        let consumer = crate::Compilation::load_with_codegen(request, codegen)
            .expect("fixture compilation must load for code generation");

        assert!(
            consumer.check_diagnostics().is_empty(),
            "{:?}",
            consumer.check_diagnostics()
        );

        let plan = consumer
            .native_product_plan(
                super::support::compilation::test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .expect("fixture product must reach native planning");

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }
}
