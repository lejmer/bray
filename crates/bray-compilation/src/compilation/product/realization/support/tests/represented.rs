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
        let compilation = Compilation::load(standard_memory_request(source, imported)).unwrap();
        let diagnostics = compilation.check_diagnostics();

        assert!(
            !diagnostics.has_errors(),
            "imported={imported}: {diagnostics:?}"
        );

        let lowered = compilation
            .lowered_unit(source_function_body_key(&compilation, "inspect"))
            .unwrap();

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        let buffer = mir
            .storages()
            .iter()
            .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
            .unwrap()
            .ty();

        let values = compilation.semantic_value_store().unwrap();
        let cancellation = CancellationToken::new();

        let aggregate = values
            .intern_type(TypeData::tuple([buffer, buffer]))
            .unwrap();

        assert_eq!(
            compilation
                .owner_outgoing_capacity(buffer, &cancellation)
                .unwrap(),
            1,
            "one local traversal frame is secured before buffer ownership exists"
        );

        assert_eq!(
            compilation
                .owner_outgoing_capacity(aggregate, &cancellation)
                .unwrap(),
            0,
            "wrapping buffers transfers their existing allowances without readmission"
        );

        let boxed = compilation.lowered_unit(source_function_body_key(&compilation, "dispose_boxed")).unwrap();

        let boxed = boxed.value().as_ref().unwrap().mir().unwrap().storages().iter()
            .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0))).unwrap().ty();

        let frames = compilation.owner_outgoing_calls(boxed, &cancellation).unwrap()
            .iter().filter_map(|(instance, _, _)| match instance.generated_lifecycle_reference() {
                Some(MirHelperReference::LifecycleFrame { role, .. }) => Some(*role),
                _ => None,
            }).collect::<Vec<_>>();

        assert_eq!(frames, [MirGeneratedLifecycleRole::Destroy(bray_ir::MirDestructionDisposition::Dynamic)],
            "retain storage-policy call allowances and only the selected consuming traversal frame");

        for (function, suspends) in [("plain", false), ("recursive", true), ("shared", true)] {
            if suspends {
                let lowered = compilation
                    .lowered_unit(source_function_body_key(&compilation, "node"))
                    .unwrap();

                let node = lowered
                    .value()
                    .as_ref()
                    .unwrap()
                    .mir()
                    .unwrap()
                    .storages()
                    .iter()
                    .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
                    .unwrap()
                    .ty();

                assert_eq!(
                    compilation
                        .owner_outgoing_capacity(node, &cancellation)
                        .unwrap(),
                    0,
                    "compute the cycle from its aggregate root before requesting the recursive buffer"
                );
            }

            let lowered = compilation
                .lowered_unit(source_function_body_key(&compilation, function))
                .unwrap();

            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            let ty = mir
                .storages()
                .iter()
                .find(|storage| matches!(storage.kind(), MirStorageKind::Parameter(0)))
                .unwrap()
                .ty();

            let capacity = compilation
                .owner_outgoing_capacity(ty, &cancellation)
                .unwrap();

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
            .unwrap();

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        let context = crate::compilation::product::realization::synthetic::CompilationSyntheticLoweringContext::new(
            &compilation, None, &cancellation).unwrap();

        let destruction = bray_lowering::expand_lifecycle_continuations(&context, mir)
            .unwrap()
            .unwrap();

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
            .unwrap();

        assert_eq!(release.frame_descriptor().unwrap().result_type(), unit);

        let allocated = compilation
            .lowered_unit(source_function_body_key(&compilation, "allocate_owner"))
            .unwrap();

        let allocated = allocated.value().as_ref().unwrap().mir().unwrap();

        let admission = allocated.operations().iter().position(|operation| matches!(operation.kind(),
            MirOperationKind::AdmitOutgoing { owner: MirOutgoingOwner::Value(ty), .. } if *ty == buffer)).unwrap();

        let allocation = allocated.operations().iter().position(|operation| matches!(operation.kind(),
            MirOperationKind::Memory(memory) if memory.kind() == bray_bound_tree::CheckedMemoryOperationKind::RawBufferAllocate)).unwrap();

        assert!(
            admission < allocation,
            "secure local traversal backing before allocation publishes ownership"
        );
    }
}
