use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirGeneratedLifecycleRole, MirHelperReference, MirOperationKind, MirOutgoingOwner,
    MirStorageKind,
};
use bray_symbols::TypeData;

use super::lifecycle_fixtures::{generated_lifecycle, standard_memory_request};
use super::targets::codegen_target;
use crate::test_support::source_function_body_key;
use crate::{CancellationToken, Compilation};

#[test]
fn standalone_shared_generator_next_demands_its_buffer_layout() {
    let compilation = crate::test_support::compilation(
        r#"module app;
        func main() {
            let values = { each item in 0..3 { yield item > 0; } };
            for value in values {}
        }"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = source_function_body_key(&compilation, "main");

    let bound = compilation
        .bound_unit(key.clone())
        .expect("generator consumer binds");

    let expression = bound
        .value()
        .tree()
        .expressions()
        .find_map(|(id, expression)| {
            matches!(expression, bray_bound_tree::BoundExpression::For(_)).then_some(id)
        })
        .expect("consumer contains its selected iteration");

    let selected = compilation
        .iteration_source(key, expression)
        .expect("iteration selection publishes");

    let selected = selected
        .value()
        .as_ref()
        .expect("generator protocol is selected");

    let target = codegen_target(&compilation);
    let cancellation = CancellationToken::new();

    let instance = compilation
        .concrete_codegen_callable(
            selected.next(),
            [selected.iterator_witness()],
            &target,
            &cancellation,
        )
        .expect("standalone next has a concrete instance");

    let mir = compilation
        .codegen_compiler_provided_mir(&instance, bray_ir::MirUnitId::new(0), &cancellation)
        .expect("standalone next retains its shared generator operation");

    let demanded = compilation
        .operation_referent_types(&mir, &instance, &cancellation)
        .expect("memory referent demands resolve");

    assert!(
        demanded.contains(&selected.source_type()),
        "shared next accesses its borrowed buffer header: {demanded:?}"
    );

    let mappings = compilation
        .codegen_types(demanded, None, &target, &cancellation)
        .expect("standalone buffer layout realizes");

    assert!(mappings.iter().any(|mapping| mapping.ty() == selected.source_type()
        && matches!(mapping.kind(), bray_codegen::CodegenTypeKind::Aggregate(fields) if fields.len() == 3)),
        "shared next receives the complete three-word generator layout");
}

#[test]
fn generator_cleanup_admits_and_expands_suspending_prefix_traversal() {
    let compilation = crate::test_support::compilation(
        r#"
        module app;
        using core.atomic;
        static COUNT: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);
        async func record() {
            let previous = core.atomic.load<usize, 1>(&COUNT);
            core.atomic.store<usize, 4>(&COUNT, previous + 1);
        }
        struct Owner { async finalize() { await record(); } }
        async func dispose() {
            let owners = { each item in 0..2 { yield Owner {}; } };
        }
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_function_body_key(&compilation, "dispose"))
        .expect("generator disposal must lower");

    let mir = lowered
        .value()
        .as_ref()
        .expect("checked disposal has a body")
        .mir()
        .expect("disposal retains source MIR");

    let values = compilation
        .semantic_value_store()
        .expect("generator type retains its semantic values");

    let generator = mir
        .storages()
        .iter()
        .map(|storage| storage.ty())
        .find(|ty| matches!(values.type_data(*ty).as_ref(), TypeData::Generator(_)))
        .expect("eager construction retains its generator storage");

    let cancellation = CancellationToken::new();

    assert!(
        mir.operations()
            .iter()
            .all(|operation| !matches!(operation.kind(),
        MirOperationKind::ReleaseBuffer { place, .. } if place.ty() == generator)),
        "growth retires the relocated prefix before releasing empty backing, without invoking element cleanup"
    );

    for (_, block) in mir.blocks_with_ids() {
        if block.operations().iter().any(|id| matches!(mir.operation(*id).expect("block operation exists").kind(),
            MirOperationKind::Memory(memory) if *memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawBufferAllocate)) {
            let bray_ir::MirTerminatorKind::CheckCallOutcome { panicked, .. } = block.terminator().kind() else {
                panic!("generator allocation retains its checked outcome");
            };

            assert!(!matches!(mir.block(panicked.target()).expect("allocation failure target exists").terminator().kind(),
                bray_ir::MirTerminatorKind::Unreachable), "allocation failure reaches checked owner cleanup: {block:?}, failure={:?}", mir.block(panicked.target()));
        }
    }

    for operation in mir.operations() {
        if let MirOperationKind::Cleanup {
            phase: bray_ir::MirCleanupPhase::LifecycleResolution,
            place,
            collection,
        } = operation.kind()
            && place.ty() == generator
        {
            assert!(
                collection
                    .as_ref()
                    .and_then(|collection| collection.retention())
                    .is_some(),
                "every partial generator failure retains its enclosing cleanup frame"
            );
        }
    }

    let calls = compilation
        .owner_outgoing_calls(generator, &cancellation)
        .expect("generator cleanup admission must resolve");

    assert_eq!(calls.len(), 1, "admit the one local prefix traversal frame");

    let reference = calls[0]
        .0
        .generated_lifecycle_reference()
        .expect("prefix traversal selects a represented lifecycle frame");

    assert_eq!(
        reference,
        &MirHelperReference::LifecycleFrame {
            role: MirGeneratedLifecycleRole::ReleaseBuffer,
            ty: generator,
        }
    );

    let release = generated_lifecycle(
        &compilation,
        &codegen_target(&compilation),
        reference.clone(),
        132,
    );

    assert!(release.is_valid());
    assert!(release.frame_descriptor().is_some());

    assert!(release.operations().iter().all(|operation| !matches!(operation.kind(),
        MirOperationKind::DischargeOutgoing { owner: MirOutgoingOwner::Value(ty), .. } if *ty == generator)),
        "borrowed prefix release retains the existing generator allowance");

    let context = crate::compilation::product::realization::synthetic::CompilationSyntheticLoweringContext::new(
        &compilation, None, &cancellation).expect("generator lifecycle context must resolve");

    let expanded = bray_lowering::expand_lifecycle_continuations(&context, mir)
        .expect("generator cleanup must expand into its enclosing frame")
        .expect("generator disposal contains selected cleanup");

    assert!(expanded.is_valid());

    assert!(
        expanded
            .operations()
            .iter()
            .any(|operation| matches!(operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
            initializer: bray_ir::MirFrameInitializer::Callable(call), ..
        }) if matches!(call.target(), bray_ir::MirCallTarget::Lifecycle {
            role: MirGeneratedLifecycleRole::ReleaseBuffer, ty
        } if *ty == generator))),
        "consuming disposal composes the admitted prefix frame"
    );

    assert!(expanded.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::DischargeOutgoing { owner: MirOutgoingOwner::Value(ty), .. } if *ty == generator)),
        "consuming disposal retires the generator allowance in the caller");
}

#[test]
fn buffer_traversal_admits_local_work_and_retains_it_after_borrowed_release() {
    let source = r#"
        module app;
        using std.memory;
        using core.atomic;

        static COUNT: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);

        async func record()
        {
            let previous = core.atomic.load<usize, 1>(&COUNT);
            core.atomic.store<usize, 4>(&COUNT, previous + 1);
        }

        struct Owner
        {
            async finalize() { await record(); }
        }

        struct PlainNode
        {
            children: std.memory.RawBuffer<PlainNode>;
        }

        struct AsyncNode
        {
            children: std.memory.RawBuffer<AsyncNode>;
            shared: Branch;
            after: Owner;
        }

        struct Branch
        {
            children: std.memory.RawBuffer<AsyncNode>;
        }

        func node(pos value: AsyncNode) -> AsyncNode { return value; }

        func plain(pos buffer: std.memory.RawBuffer<PlainNode>) -> std.memory.RawBuffer<PlainNode>
        {
            return buffer;
        }

        func recursive(pos buffer: std.memory.RawBuffer<AsyncNode>) -> std.memory.RawBuffer<AsyncNode>
        {
            return buffer;
        }

        func shared(pos buffer: std.memory.RawBuffer<Branch>) -> std.memory.RawBuffer<Branch>
        {
            return buffer;
        }

        func inspect(pos buffer: std.memory.RawBuffer<Owner>) -> std.memory.RawBuffer<Owner>
        {
            return buffer;
        }

        async func dispose(pos buffer: std.memory.RawBuffer<Owner>) {}
        async func dispose_boxed(pos owner: box Owner) {}

        trusted async func allocate_owner() uses(manual_alloc)
        {
            let buffer: std.memory.RawBuffer<Owner> = trusted internal std.memory.allocate_buffer<Owner>(
                capacity = 0, bytes = 0, align = std.memory.align_of<Owner>());
        }
    "#;

    for imported in [false, true] {
        let compilation = Compilation::load(standard_memory_request(source, imported))
            .expect("fixture input must load");

        let diagnostics = compilation.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "imported={imported}: {diagnostics:?}"
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "inspect"))
            .expect("fixture callable must lower successfully");

        let mir = lowered
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR");

        let buffer = mir
            .storages()
            .iter()
            .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
            .expect("fixture lookup must find its expected value")
            .ty();

        let values = compilation
            .semantic_value_store()
            .expect("fixture compilation must provide its semantic value store");

        let cancellation = CancellationToken::new();

        let aggregate = values
            .intern_type(TypeData::tuple([buffer, buffer]))
            .expect("fixture type must be interned");

        assert_eq!(
            compilation
                .owner_outgoing_capacity(buffer, &cancellation)
                .expect("fixture `owner_outgoing_capacity` call must succeed"),
            1,
            "one local traversal frame is secured before buffer ownership exists"
        );

        assert_eq!(
            compilation
                .owner_outgoing_capacity(aggregate, &cancellation)
                .expect("fixture `owner_outgoing_capacity` call must succeed"),
            0,
            "wrapping buffers transfers their existing allowances without readmission"
        );

        let boxed = compilation
            .lowered_unit(source_function_body_key(&compilation, "dispose_boxed"))
            .expect("fixture callable must lower successfully");

        let boxed = boxed
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR")
            .storages()
            .iter()
            .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
            .expect("fixture lookup must find its expected value")
            .ty();

        let frames = compilation
            .owner_outgoing_calls(boxed, &cancellation)
            .expect("fixture `owner_outgoing_calls` call must succeed")
            .iter()
            .filter_map(
                |(instance, _, _)| match instance.generated_lifecycle_reference() {
                    Some(MirHelperReference::LifecycleFrame { role, .. }) => Some(*role),
                    _ => None,
                },
            )
            .collect::<Vec<_>>();

        assert_eq!(
            frames,
            [MirGeneratedLifecycleRole::Destroy(
                bray_ir::MirDestructionDisposition::Dynamic
            )],
            "retain storage-policy call allowances and only the selected consuming traversal frame"
        );

        for (function, suspends) in [("plain", false), ("recursive", true), ("shared", true)] {
            if suspends {
                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "node"))
                    .expect("fixture callable must lower successfully");

                let node = lowered
                    .value()
                    .as_ref()
                    .expect("fixture optional value must be present")
                    .mir()
                    .expect("fixture instance must retain its MIR")
                    .storages()
                    .iter()
                    .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
                    .expect("fixture lookup must find its expected value")
                    .ty();

                assert_eq!(
                    compilation
                        .owner_outgoing_capacity(node, &cancellation)
                        .expect("fixture `owner_outgoing_capacity` call must succeed"),
                    0,
                    "compute the cycle from its aggregate root before requesting the recursive buffer"
                );
            }

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, function))
                .expect("fixture callable must lower successfully");

            let mir = lowered
                .value()
                .as_ref()
                .expect("fixture optional value must be present")
                .mir()
                .expect("fixture instance must retain its MIR");

            let ty = mir
                .storages()
                .iter()
                .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
                .expect("fixture lookup must find its expected value")
                .ty();

            let capacity = compilation
                .owner_outgoing_capacity(ty, &cancellation)
                .expect("fixture `owner_outgoing_capacity` call must succeed");

            assert_eq!(
                capacity > 0,
                suspends,
                "recursive {function} only admits selected suspending work"
            );
        }

        let target = codegen_target(&compilation);

        let release = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::LifecycleFrame {
                role: MirGeneratedLifecycleRole::ReleaseBuffer,
                ty: buffer,
            },
            131,
        );

        assert!(release.is_valid(), "{release:?}");
        assert!(release.frame_descriptor().is_some());

        assert!(release.operations().iter().all(|operation| !matches!(operation.kind(),
            MirOperationKind::DischargeOutgoing { owner: MirOutgoingOwner::Value(ty), .. } if *ty == buffer)),
            "borrowed release leaves the same buffer allowance live for refill or later cleanup");

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "dispose"))
            .expect("fixture callable must lower successfully");

        let mir = lowered
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR");

        let context = crate::compilation::product::realization::synthetic::CompilationSyntheticLoweringContext::new(
            &compilation, None, &cancellation).expect("fixture compilation synthetic lowering context must be valid");

        let destruction = bray_lowering::expand_lifecycle_continuations(&context, mir)
            .expect("fixture `bray_lowering::expand_lifecycle_continuations` call must succeed")
            .expect("successful fixture query must contain its expected result");

        // Source guards select an ownership end. Expanded paths discharge in the
        // caller; synchronous abandoned callbacks retain their own discharge.
        let consumes = |operation: &MirOperationKind| {
            matches!(operation,
            MirOperationKind::Cleanup { phase: bray_ir::MirCleanupPhase::LifecycleResolution, place, .. }
            | MirOperationKind::Destroy { place, .. } if place.ty() == buffer)
        };

        let owner_ends = mir
            .operations()
            .iter()
            .filter(|operation| consumes(operation.kind()))
            .count();

        let callbacks = destruction
            .operations()
            .iter()
            .filter(|operation| consumes(operation.kind()))
            .count();

        let discharges = destruction.operations().iter().filter(|operation| matches!(operation.kind(),
            MirOperationKind::DischargeOutgoing { owner: MirOutgoingOwner::Value(ty), .. } if *ty == buffer)).count();

        assert!(discharges > 0);

        assert_eq!(
            discharges + callbacks,
            owner_ends,
            "one allowance end per consuming site"
        );

        let unit = compilation
            .compiler_known_type(RepresentationRole::Unit)
            .expect("fixture compiler-known type must resolve");

        assert_eq!(
            release
                .frame_descriptor()
                .expect("fixture `frame_descriptor` call must succeed")
                .result_type(),
            unit
        );

        let allocated = compilation
            .lowered_unit(source_function_body_key(&compilation, "allocate_owner"))
            .expect("fixture callable must lower successfully");

        let allocated = allocated
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR");

        let admission = allocated.operations().iter().position(|operation| matches!(operation.kind(),
            MirOperationKind::AdmitOutgoing { owner: MirOutgoingOwner::Value(ty), .. } if *ty == buffer)).expect("fixture sequence must contain the selected entry");

        let allocation = allocated.operations().iter().position(|operation| matches!(operation.kind(),
            MirOperationKind::Memory(memory) if memory.kind() == &bray_bound_tree::CheckedMemoryOperationKind::RawBufferAllocate)).expect("fixture sequence must contain the selected entry");

        assert!(
            admission < allocation,
            "secure local traversal backing before allocation publishes ownership"
        );
    }
}
