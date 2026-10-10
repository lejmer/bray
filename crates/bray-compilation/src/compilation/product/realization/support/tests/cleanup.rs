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
fn nullable_cancellation_branches_within_the_cleanup_phase() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let payload = values
        .intern_type(TypeData::tuple([]))
        .expect("payload type must intern");

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
            place,
        } if matches!(
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

    let checked = generated
        .blocks()
        .iter()
        .filter(|block| {
            block
                .operations()
                .last()
                .and_then(|id| generated.operation(*id))
                .is_some_and(|operation| {
                    matches!(
                        operation.kind(),
                        MirOperationKind::Finalize(_) | MirOperationKind::Destroy(_)
                    )
                })
        })
        .collect::<Vec<_>>();

    assert_eq!(checked.len(), 2);

    assert!(checked.iter().all(|block| matches!(
        block.terminator().kind(),
        bray_ir::MirTerminatorKind::CheckCallOutcome { .. }
    )));

    assert_eq!(
        generated
            .operations()
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
    use crate::test_support::{source_function_body_key, source_input};
    use crate::{Compilation, CompilationRequest};

    let compilation = Compilation::load(
        CompilationRequest::new(
            bray_symbols::PackageIdentity::try_new("std").expect("standard package is valid"),
            vec![
                source_input(include_str!("../../../../../../../../standard-library/std/src/std.bray"), 0),
                source_input(include_str!("../../../../../../../../standard-library/std/src/memory.bray"), 1),
                source_input("module std.test; func dispose(pos buffer: std.memory.RawBuffer<u8>) {}", 2),
            ],
        ).with_standard_library_source_authority(),
    ).expect("actual standard memory source must load");

    let lowered = compilation.lowered_unit(source_function_body_key(&compilation, "dispose"))
        .expect("buffer consumer must lower");
    assert!(lowered.diagnostics().is_empty(), "{:?}", lowered.diagnostics());
    let ty = lowered.value().as_ref().expect("consumer has lowered data").mir().expect("consumer has MIR").storages().iter()
        .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
        .expect("consumer has its owned buffer parameter").ty();
    let cancellation = crate::CancellationToken::new();
    assert_eq!(compilation.owner_outgoing_capacity(ty, &cancellation)
        .expect("buffer allowance must resolve"), 0);

    let cleanup = generated_lifecycle(&compilation, &codegen_target(&compilation),
        MirHelperReference::Destroy(ty), 83);
    assert!(cleanup.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Memory(memory)
            if matches!(memory.kind(), bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { .. }))));
    assert!(!cleanup.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call) if call.is_cleanup())));
}
