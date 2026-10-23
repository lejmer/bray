use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockKind, MirCleanupPhase, MirGeneratorOperation, MirHelperReference, MirOperationKind,
    MirProjectionKind, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeData;

use super::lifecycle_fixtures::{generated_lifecycle, reachable_cleanup_operations};
use super::targets::codegen_target;

use crate::test_support::compilation;

#[test]
fn string_release_collects_deallocation_failure_before_returning() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let string = compilation
        .compiler_known_type(RepresentationRole::String)
        .expect("string representation must resolve");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(string, bray_ir::MirDestructionDisposition::Dynamic),
        79,
    );

    let release = generated
        .blocks()
        .iter()
        .find(|block| {
            block.operations().last().is_some_and(|id| {
                matches!(
                    generated.operation(*id).map(|operation| operation.kind()),
                    Some(MirOperationKind::Text(operation))
                        if operation.kind() == bray_ir::MirTextOperationKind::Release
                )
            })
        })
        .expect("string destruction must release its backing");

    assert!(matches!(
        release.terminator().kind(),
        MirTerminatorKind::CheckCallOutcome { .. }
    ));

    assert!(generated.blocks().iter().all(|block| !matches!(
        block.terminator().kind(),
        MirTerminatorKind::PropagatePanic { .. } | MirTerminatorKind::PropagateCancellation { .. }
    )));
}

#[test]
fn nullable_cancellation_branches_within_the_cleanup_phase() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let unit = compilation
        .compiler_known_type(RepresentationRole::Unit)
        .unwrap();

    let payload = compilation
        .available_compiler_known_symbols()
        .unary_representation_type(values, RepresentationRole::Future, unit)
        .unwrap()
        .unwrap();

    let nullable = values
        .intern_type(TypeData::Nullable(payload))
        .expect("nullable type must intern");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            ty: nullable,
        },
        82,
    );

    let branch = generated
        .blocks()
        .iter()
        .find_map(|block| match block.terminator().kind() {
            MirTerminatorKind::PatternBranch {
                predicate: bray_ir::MirPatternPredicate::NullablePresent,
                matched,
                unmatched,
                ..
            } => Some((matched.target(), unmatched.target())),
            _ => None,
        })
        .expect("nullable cleanup must branch on presence");

    let present = generated
        .block(branch.0)
        .expect("present cleanup block must exist");

    let absent = generated
        .block(branch.1)
        .expect("absent cleanup block must exist");

    assert_eq!(present.kind(), MirBlockKind::CleanupBroadcast);

    let operations = reachable_cleanup_operations(&generated, branch.0);

    assert_eq!(operations.len(), 1);
    assert!(absent.operations().is_empty());

    assert!(matches!(
        operations[0],
        MirOperationKind::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            place, .. } if matches!(
            place.projections().last().map(|projection| projection.kind()),
            Some(MirProjectionKind::NullableValue)
        )
    ));
}

#[test]
fn generator_cleanup_broadcast_reaches_exact_element_cleanup() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let leaf = values
        .intern_type(TypeData::tuple([]))
        .expect("generator leaf type must intern");

    let element = values
        .intern_type(TypeData::Generator(leaf))
        .expect("nontrivial generator element type must intern");

    let generator = values
        .intern_type(TypeData::Generator(element))
        .expect("outer generator type must intern");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            ty: generator,
        },
        83,
    );

    let operation = generated
        .operations()
        .iter()
        .find(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Generator(MirGeneratorOperation::CleanupBroadcast { .. })
            )
        })
        .expect("generator cleanup must contain one broadcast operation");

    let MirOperationKind::Generator(MirGeneratorOperation::CleanupBroadcast {
        element: operation_element,
        runtime,
        ..
    }) = operation.kind()
    else {
        unreachable!("the operation was selected by its exact variant");
    };

    assert_eq!(*operation_element, element);
    assert_eq!(runtime.role(), RuntimeAbiRole::GeneratorCleanupBroadcast);

    assert_eq!(
        operation.kind().helper_references(),
        [MirHelperReference::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            ty: element,
        }]
    );
}

#[test]
fn task_cleanup_checks_completed_payload_before_releasing_the_task() {
    let compilation = compilation("module app;");
    let target = codegen_target(&compilation);
    let values = compilation.semantic_value_store().unwrap();

    let unit = compilation
        .compiler_known_type(RepresentationRole::Unit)
        .unwrap();

    let task = compilation
        .available_compiler_known_symbols()
        .unary_representation_type(values, RepresentationRole::Task, unit)
        .unwrap()
        .unwrap();

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Cleanup {
            phase: bray_ir::MirCleanupPhase::LifecycleResolution,
            ty: task,
        },
        97,
    );

    assert!(generated.is_valid());

    let operations = generated.operations();

    let payload = operations
        .iter()
        .position(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Cleanup {
                    phase: MirCleanupPhase::LifecycleResolution,
                    collection: Some(_),
                    ..
                }
            )
        })
        .expect("completed task payload borrows the enclosing cleanup collection");

    let release = operations
        .iter()
        .position(|operation| {
            matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::DestroyTerminalTask { .. })
            )
        })
        .expect("the resolved task retains one terminal release");

    assert!(
        payload < release,
        "payload cleanup finishes before its task storage is released"
    );

    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::DestroyTerminalTask { .. })
            ))
            .count(),
        1
    );
}

#[test]
fn source_standard_buffer_cleanup_uses_the_imported_structural_allowance() {
    use crate::test_support::source_function_body_key;
    use crate::{Compilation, CompilationRequest};

    let compilation = Compilation::load(
        CompilationRequest::new(
            bray_symbols::PackageIdentity::try_new("std").expect("standard package is valid"),
            crate::test_support::source_inputs(&[
                include_str!("../../../../../../../../standard-library/std/src/std.bray"),
                include_str!(
                    "../../../../../../../../standard-library/std/src/memory/pointer.bray"
                ),
                include_str!(
                    "../../../../../../../../standard-library/std/src/memory/storage.bray"
                ),
                include_str!(
                    "../../../../../../../../standard-library/std/src/memory/allocation.bray"
                ),
                include_str!(
                    "../../../../../../../../standard-library/std/src/numeric/limits.bray"
                ),
                include_str!("../../../../../../../../standard-library/std/src/memory/buffer.bray"),
                include_str!(
                    "../../../../../../../../standard-library/std/src/memory/function.bray"
                ),
                r#"module std.test;

using std.memory;

 func dispose(pos buffer: std.memory.RawBuffer<u8>)
 {
 }"#,
            ]),
        )
        .with_standard_library_source_authority(),
    )
    .expect("actual standard memory source must load");

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "dispose"))
        .expect("buffer consumer must lower");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );

    let ty = lowered
        .value()
        .as_ref()
        .expect("consumer has lowered data")
        .mir()
        .expect("consumer has MIR")
        .storages()
        .iter()
        .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
        .expect("consumer has its owned buffer parameter")
        .ty();

    let cancellation = crate::CancellationToken::new();

    assert_eq!(
        compilation
            .owner_outgoing_capacity(ty, &cancellation)
            .expect("buffer allowance must resolve"),
        0
    );

    let cleanup = generated_lifecycle(
        &compilation,
        &codegen_target(&compilation),
        MirHelperReference::Destroy(ty, bray_ir::MirDestructionDisposition::Dynamic),
        83,
    );

    assert!(cleanup.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Memory(memory)
            if matches!(memory.kind(), bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { .. })
                && memory.abandoned_cleanup().is_some())));

    assert!(
        !cleanup
            .operations()
            .iter()
            .any(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call) if call.is_cleanup()))
    );
}

#[test]
fn imported_standard_buffer_cleanup_uses_structural_allowance() {
    use crate::test_support::source_function_body_key;
    use crate::{Compilation, CompilationRequest};

    let dependency = super::lifecycle_fixtures::standard_memory_dependency();

    let compilation = Compilation::load(
        CompilationRequest::new(
            crate::test_support::package_identity(),
            crate::test_support::source_inputs(&[r#"
            module app;

            using internal std.memory;

            func dispose(pos buffer: std.memory.RawBuffer<u8>)
            {
            }
        "#]),
        )
        .with_dependency_interfaces([dependency]),
    )
    .expect("imported storage consumer must load");

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "dispose"))
        .expect("imported storage consumer must load");

    assert!(
        lowered.diagnostics().is_empty(),
        "{:?}",
        lowered.diagnostics()
    );

    let ty = lowered
        .value()
        .as_ref()
        .expect("consumer has lowered data")
        .mir()
        .expect("consumer has MIR")
        .storages()
        .iter()
        .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
        .expect("consumer retains its owned buffer parameter")
        .ty();

    assert_eq!(
        compilation
            .owner_outgoing_capacity(ty, &crate::CancellationToken::new())
            .expect("buffer allowance must resolve"),
        0
    );

    let cleanup = generated_lifecycle(
        &compilation,
        &codegen_target(&compilation),
        MirHelperReference::Destroy(ty, bray_ir::MirDestructionDisposition::Dynamic),
        84,
    );

    assert!(cleanup.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Memory(memory)
            if matches!(memory.kind(), bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { .. })
                && memory.abandoned_cleanup().is_some())));
}

#[test]
fn imported_raw_storage_formation_requires_allocation_ownership() {
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    let dependency = super::lifecycle_fixtures::standard_memory_dependency();

    for source in [
        r#"trusted module app;

using core.memory;

 using internal std.memory; func forge() -> std.memory.RawAllocation { return { pointer = core.memory.null<u8>(), bytes = 8, align = 8 }; }"#,
        r#"trusted module app;

using core.memory;

 using internal std.memory; func forge() -> std.memory.RawBuffer<u8> { return { pointer = core.memory.null<u8>(), capacity = 1, initialized = 0 }; }"#,
    ] {
        let compilation = Compilation::load(
            CompilationRequest::new(
                crate::test_support::package_identity(),
                vec![source_input(source, 0)],
            )
            .with_dependency_interfaces([dependency.clone()]),
        )
        .expect("imported storage consumer must load");

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    let compilation = Compilation::load(CompilationRequest::new(
        crate::test_support::package_identity(), vec![source_input(r#"
            trusted module app;

            using core.memory;
            using internal std.memory;
            func pack(pointer: DataPointer<u8>, bytes: usize, align: usize) -> std.memory.RawAllocation
                requires(trusted core.memory.owned_allocation(pointer = pointer, bytes = bytes, align = align))
            {
                return { pointer = pointer, bytes = bytes, align = align };
            }
        "#, 0)],
    ).with_dependency_interfaces([dependency])).expect("imported storage consumer must load");

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn imported_standard_buffer_accepts_live_generic_storage() {
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    let dependency = super::lifecycle_fixtures::standard_memory_dependency();

    let compilation = Compilation::load(CompilationRequest::new(
        crate::test_support::package_identity(), vec![source_input(r"
            trusted module app;

            using std.numeric;

            using core.memory;
            using internal std.memory;
            func pack<T>(pointer: DataPointer<T>, capacity: usize) -> std.memory.RawBuffer<T>
                requires(
                    std.memory.stride_of<T>() * capacity <= std.numeric.USIZE_MAX,
                    trusted core.memory.owned_allocation(pointer = std.memory.byte_address<T>(pointer), bytes = std.memory.stride_of<T>() * capacity, align = std.memory.align_of<T>()),
                    trusted core.memory.valid_write<T>(pointer = pointer, count = capacity),
                    trusted core.memory.aligned_for<T>(pointer = pointer),
                    trusted core.memory.initialized_range_as<T>(pointer = pointer, count = 0),
                )
            {
                return { pointer = pointer, capacity = capacity, initialized = 0 };
            }
        ", 0)],
    ).with_dependency_interfaces([dependency])).expect("imported storage consumer must load");

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn imported_standard_buffer_requires_bounded_initialized_prefix() {
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    let dependency = super::lifecycle_fixtures::standard_memory_dependency();

    for (bound, valid) in [("initialized <= capacity,", true), ("", false)] {
        let source = format!(
            r"
            trusted module app;

            using std.numeric;

            using core.memory;
            using internal std.memory;
            func pack<T>(pointer: DataPointer<T>, capacity: usize, initialized: usize) -> std.memory.RawBuffer<T>
                requires(
                    {bound}
                    std.memory.stride_of<T>() * capacity <= std.numeric.USIZE_MAX,
                    trusted core.memory.owned_allocation(pointer = std.memory.byte_address<T>(pointer), bytes = std.memory.stride_of<T>() * capacity, align = std.memory.align_of<T>()),
                    trusted core.memory.valid_write<T>(pointer = pointer, count = capacity),
                    trusted core.memory.aligned_for<T>(pointer = pointer),
                    trusted core.memory.initialized_range_as<T>(pointer = pointer, count = initialized),
                )
            {{
                return {{ pointer = pointer, capacity = capacity, initialized = initialized }};
            }}
        "
        );

        let compilation = Compilation::load(
            CompilationRequest::new(
                crate::test_support::package_identity(),
                vec![source_input(&source, 0)],
            )
            .with_dependency_interfaces([dependency.clone()]),
        )
        .expect("imported storage consumer must load");

        if valid {
            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:?}",
                compilation.check_diagnostics()
            );
        } else {
            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn imported_raw_storage_cannot_adopt_an_allocation_twice() {
    use crate::test_support::source_input;
    use crate::{Compilation, CompilationRequest};

    let dependency = super::lifecycle_fixtures::standard_memory_dependency();

    for owner in [
        "let first: std.memory.RawBuffer<u8> = { pointer = pointer, capacity = capacity, initialized = 0 };",
        "let first: std.memory.RawAllocation = { pointer = pointer, bytes = std.memory.stride_of<u8>() * capacity, align = std.memory.align_of<u8>() };",
    ] {
        let source = format!(
            r"
            trusted module app;

            using core.memory;
            using internal std.memory;
            func duplicate(pointer: DataPointer<u8>, capacity: usize) -> std.memory.RawBuffer<u8>
                requires(
                    trusted core.memory.owned_allocation(pointer = pointer, bytes = std.memory.stride_of<u8>() * capacity, align = std.memory.align_of<u8>()),
                    trusted core.memory.valid_write<u8>(pointer = pointer, count = capacity),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                    trusted core.memory.initialized_range_as<u8>(pointer = pointer, count = 0),
                )
            {{
                {owner}
                return {{ pointer = pointer, capacity = capacity, initialized = 0 }};
            }}
        "
        );

        let compilation = Compilation::load(
            CompilationRequest::new(
                crate::test_support::package_identity(),
                vec![source_input(&source, 0)],
            )
            .with_dependency_interfaces([dependency.clone()]),
        )
        .expect("imported storage consumer must load");

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}

#[test]
fn cancellation_and_quiescence_retain_terminal_task_ownership() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);
    let values = compilation.semantic_value_store().unwrap();

    let unit = compilation
        .codegen_representation_type(RepresentationRole::Unit)
        .unwrap();

    let task = compilation
        .available_compiler_known_symbols()
        .unary_representation_type(values, RepresentationRole::Task, unit)
        .unwrap()
        .unwrap();

    for reference in [
        MirHelperReference::Quiesce(task),
        MirHelperReference::Cleanup {
            phase: bray_ir::MirCleanupPhase::TaskCancellation,
            ty: task,
        },
    ] {
        let generated = generated_lifecycle(&compilation, &target, reference, 92);

        let resolved = generated
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Async(
                    bray_ir::MirAsyncOperation::ResolveTask { task, .. }
                    | bray_ir::MirAsyncOperation::RequestTaskCancellation { task, .. },
                ) => Some(task),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].ty(), task);

        for operation in generated.operations() {
            operation.kind().for_each_operand(|operand| {
                assert!(
                    !matches!(operand, bray_ir::MirOperand::Move(place) if place == resolved[0])
                );
            });
        }

        assert!(!generated.operations().iter().any(|operation| matches!(
            operation.kind(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::DestroyTerminalTask { .. })
                | MirOperationKind::DischargeOutgoing { .. }
        )));

        assert!(generated.is_valid());
    }
}

#[test]
fn incident_quiescence_visits_children_without_parent_finalization_or_destruction() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);
    let values = compilation.semantic_value_store().unwrap();

    let unit = compilation
        .codegen_representation_type(RepresentationRole::Unit)
        .unwrap();

    let task = compilation
        .available_compiler_known_symbols()
        .unary_representation_type(values, RepresentationRole::Task, unit)
        .unwrap()
        .unwrap();

    let nullable = values.intern_type(TypeData::Nullable(task)).unwrap();

    let pair = values
        .intern_type(TypeData::tuple([task, nullable]))
        .unwrap();

    let generated =
        generated_lifecycle(&compilation, &target, MirHelperReference::Quiesce(pair), 93);

    let report = compilation
        .codegen_representation_type(RepresentationRole::PanicReport)
        .unwrap();

    assert!(
        generated
            .storages()
            .iter()
            .all(|storage| storage.ty() != report),
        "structural traversal borrows report storage without a local report temporary"
    );

    let children = generated
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Quiesce { place, .. } => Some(place),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(children.len(), 2);
    assert_eq!(children[0].ty(), nullable);
    assert_eq!(children[1].ty(), task);

    assert!(!generated.operations().iter().any(|operation| matches!(
        operation.kind(),
        MirOperationKind::Destroy { .. } | MirOperationKind::DischargeOutgoing { .. }
    )));

    assert!(
        !compilation
            .codegen_lifecycle_is_trivial(
                &MirHelperReference::Quiesce(pair),
                &crate::CancellationToken::new()
            )
            .unwrap()
    );

    assert!(generated.is_valid());
}

#[test]
fn incident_quiescence_does_not_select_declared_graceful_or_destructive_calls() {
    let compilation = compilation(
        r"
        module app;
        struct Error {
            finalize() {}
            destruct() {}
        }
    ",
    );

    assert!(!compilation.check_diagnostics().has_errors());

    let symbols = compilation.symbol_graph().unwrap();

    let definition = symbols
        .structures()
        .iter()
        .find(|value| value.origin() == bray_symbols::SymbolOrigin::Source)
        .unwrap();

    let ty = crate::compilation::substitution::named_type(
        compilation.semantic_value_store().unwrap(),
        bray_symbols::NamedTypeSymbolId::Struct(definition.id()),
    )
    .unwrap();

    let target = codegen_target(&compilation);
    let generated = generated_lifecycle(&compilation, &target, MirHelperReference::Quiesce(ty), 94);

    assert!(generated.operations().is_empty());
    assert!(generated.is_valid());
}

#[test]
fn incident_quiescence_keeps_erased_capture_work_nontrivial() {
    let compilation = compilation("module app; func main() {}");
    let values = compilation.semantic_value_store().unwrap();

    let unit = compilation
        .codegen_representation_type(RepresentationRole::Unit)
        .unwrap();

    let future = compilation
        .available_compiler_known_symbols()
        .unary_representation_type(values, RepresentationRole::Future, unit)
        .unwrap()
        .unwrap();

    let nullable = values.intern_type(TypeData::Nullable(future)).unwrap();

    let pair = values
        .intern_type(TypeData::tuple([future, nullable]))
        .unwrap();

    for ty in [future, nullable, pair] {
        assert!(
            !compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Quiesce(ty),
                    &crate::CancellationToken::new()
                )
                .unwrap()
        );
    }
}

#[test]
fn buffer_child_cleanup_uses_the_retained_initialized_prefix() {
    use crate::test_support::source_function_body_key;
    use crate::{Compilation, CompilationRequest};
    use bray_bound_tree::CheckedMemoryOperationKind;

    let source = r"
        module app;
        using internal std.memory;

        func inspect(pos buffer: std.memory.RawBuffer<Task<u8>>) -> std.memory.RawBuffer<Task<u8>> {
            return buffer;
        }

        async func dispose(pos buffer: std.memory.RawBuffer<Task<u8>>) {}
    ";

    let imported = CompilationRequest::new(
        crate::test_support::package_identity(),
        crate::test_support::source_inputs(&[source]),
    )
    .with_dependency_interfaces([super::lifecycle_fixtures::standard_memory_dependency()]);

    let source_authority = CompilationRequest::new(
        bray_symbols::PackageIdentity::try_new("std").unwrap(),
        crate::test_support::source_inputs(&[
            include_str!("../../../../../../../../standard-library/std/src/std.bray"),
            include_str!("../../../../../../../../standard-library/std/src/memory/pointer.bray"),
            include_str!("../../../../../../../../standard-library/std/src/memory/storage.bray"),
            include_str!("../../../../../../../../standard-library/std/src/memory/allocation.bray"),
            include_str!("../../../../../../../../standard-library/std/src/numeric/limits.bray"),
            include_str!("../../../../../../../../standard-library/std/src/memory/buffer.bray"),
            include_str!("../../../../../../../../standard-library/std/src/memory/function.bray"),
            source,
        ]),
    )
    .with_standard_library_source_authority();

    for request in [source_authority, imported] {
        let compilation = Compilation::load(request).unwrap();

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "inspect"))
            .unwrap();

        assert!(
            !lowered.diagnostics().has_errors(),
            "{:?}",
            lowered.diagnostics()
        );

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        let parameter = mir
            .storages()
            .iter()
            .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
            .unwrap();

        let buffer = parameter.ty();

        let disposed = compilation
            .lowered_unit(source_function_body_key(&compilation, "dispose"))
            .unwrap();

        assert!(
            !disposed.diagnostics().has_errors(),
            "{:?}",
            disposed.diagnostics()
        );

        assert!(disposed.value().as_ref().unwrap().mir().unwrap().operations().iter().any(|operation| matches!(operation.kind(),
            MirOperationKind::Cleanup { phase: MirCleanupPhase::TaskCancellation, place, .. } if place.ty() == buffer)),
            "scope broadcast must include task obligations stored in the buffer");

        let target = codegen_target(&compilation);
        let cancellation = crate::CancellationToken::new();

        for reference in [
            MirHelperReference::Quiesce(buffer),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty: buffer,
            },
        ] {
            assert!(
                !compilation
                    .codegen_lifecycle_is_trivial(&reference, &cancellation)
                    .unwrap(),
                "{reference:?}"
            );

            let generated = generated_lifecycle(&compilation, &target, reference.clone(), 97);

            assert!(generated.is_valid(), "{generated:?}");

            let memory = generated
                .operations()
                .iter()
                .filter_map(|operation| match operation.kind() {
                    MirOperationKind::Memory(memory) => Some(memory.kind()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(
                memory,
                [
                    CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
                    CheckedMemoryOperationKind::SequenceLength
                ]
            );

            let children = generated
                .operations()
                .iter()
                .filter_map(|operation| match operation.kind() {
                    MirOperationKind::Quiesce { place, .. } => {
                        assert!(matches!(reference, MirHelperReference::Quiesce(_)));

                        Some(place)
                    }
                    MirOperationKind::Cleanup { phase, place, .. } => {
                        assert_eq!(*phase, MirCleanupPhase::TaskCancellation);
                        assert!(matches!(reference, MirHelperReference::Cleanup { .. }));

                        Some(place)
                    }
                    MirOperationKind::Destroy { .. }
                    | MirOperationKind::DischargeOutgoing { .. } => {
                        panic!("prefix traversal must retain its owner and elements")
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert_eq!(
                children.len(),
                1,
                "one indexed body serves every initialized element"
            );

            assert!(matches!(
                children[0].projections().last().unwrap().kind(),
                MirProjectionKind::Index(_)
            ));

            assert!(
                generated.operations().iter().any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Binary {
                        operator: bray_ir::MirBinaryOperator::Subtract,
                        ..
                    }
                )),
                "prefix traversal follows reverse destruction order"
            );
        }
    }
}
