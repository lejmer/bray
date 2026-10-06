use bray_codegen::{
    CodegenInstance, CodegenParameterMapping, CodegenResultMapping, CodegenSymbolKey,
    CodegenTypeKind, TargetAddressSpaceKind,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirFrameReference, MirHelperReference, MirRuntimeReference};
use bray_runtime_interface::{ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole};
use bray_testing::test_mir_unit;

use super::targets::codegen_target;
use super::type_fixtures::realized_types;

use crate::compilation::product::realization::support::direct_helper_symbol;
use crate::test_support::compilation;

#[test]
fn generated_helpers_map_to_exact_runtime_and_frame_roles() {
    let owner = CodegenInstance::non_generic(test_mir_unit(1));
    let frame = ProtectedAsyncFrameId::new([7; 32]);

    let runtime = |role| {
        CodegenSymbolKey::Runtime(MirRuntimeReference::new(
            role,
            owner.key().target().runtime_abi(),
        ))
    };

    let cases = [
        (
            MirHelperReference::BeginGenerator,
            runtime(RuntimeAbiRole::GeneratorBegin),
        ),
        (
            MirHelperReference::PushGenerator,
            runtime(RuntimeAbiRole::GeneratorPush),
        ),
        (
            MirHelperReference::FinishGenerator,
            runtime(RuntimeAbiRole::GeneratorFinish),
        ),
        (
            MirHelperReference::PanicReport,
            runtime(RuntimeAbiRole::PanicReportConstruction),
        ),
        (
            MirHelperReference::MoveInactiveFrame(MirFrameReference::Known(frame)),
            CodegenSymbolKey::ProtectedFrame {
                frame,
                operation: ProtectedFrameOperation::MoveBeforeStart,
            },
        ),
        (
            MirHelperReference::MoveInactiveFrame(MirFrameReference::Erased),
            runtime(RuntimeAbiRole::InactiveFrameMove),
        ),
        (
            MirHelperReference::ComposeAwaitedFrame(MirFrameReference::Erased),
            runtime(RuntimeAbiRole::AwaitedFrameComposition),
        ),
        (
            MirHelperReference::CommitAwaitedCompletion(MirFrameReference::Known(frame)),
            CodegenSymbolKey::ProtectedFrame {
                frame,
                operation: ProtectedFrameOperation::CompletionMove,
            },
        ),
        (
            MirHelperReference::CommitAwaitedCompletion(MirFrameReference::Erased),
            runtime(RuntimeAbiRole::FrameCompletionMove),
        ),
        (
            MirHelperReference::DestroyTerminalTask,
            runtime(RuntimeAbiRole::TaskDestruction),
        ),
    ];

    for (reference, expected) in cases {
        assert_eq!(direct_helper_symbol(&owner, &reference), Some(expected));
    }
}

#[test]
fn generator_runtime_helpers_use_stable_erased_abis() {
    let compilation = compilation("module app; func main() {}");

    let begin = compilation
        .codegen_runtime_signature(RuntimeAbiRole::GeneratorBegin)
        .expect("generator begin signature must realize");

    let destruction = compilation
        .codegen_runtime_signature(RuntimeAbiRole::GeneratorDestruction)
        .expect("generator destruction signature must realize");

    assert_eq!(begin.parameters().len(), 6);
    assert_eq!(destruction.parameters().len(), 3);
    assert_eq!(begin.result(), &CodegenResultMapping::Void);
    assert_eq!(destruction.result(), &CodegenResultMapping::Void);
}

#[test]
fn cancellation_observation_runtime_helper_returns_boolean() {
    let compilation = compilation("module app; func main() {}");

    let signature = compilation
        .codegen_runtime_signature(RuntimeAbiRole::CurrentRunCancellationObservation)
        .expect("cancellation observation signature must realize");

    let boolean = compilation
        .codegen_representation_type(RepresentationRole::ScalarBool)
        .expect("boolean representation must realize");

    assert!(signature.parameters().is_empty());

    assert_eq!(
        signature.result(),
        &CodegenResultMapping::direct(boolean, None, [])
    );
}

#[test]
fn panic_report_transfer_runtime_helpers_use_the_owned_report_and_status_types() {
    let compilation = compilation("module app; func main() {}");

    let report = compilation
        .codegen_representation_type(RepresentationRole::PanicReport)
        .expect("panic report representation must realize");

    let status = compilation
        .codegen_representation_type(RepresentationRole::ScalarU32)
        .expect("status representation must realize");

    for role in [
        RuntimeAbiRole::PanicReporting,
        RuntimeAbiRole::PanicReportDestruction,
    ] {
        let signature = compilation
            .codegen_runtime_signature(role)
            .unwrap_or_else(|error| panic!("{role:?} signature must realize: {error:?}"));

        assert_eq!(
            signature.parameters(),
            [CodegenParameterMapping::direct(report, None, [])]
        );

        assert_eq!(
            signature.result(),
            &CodegenResultMapping::direct(status, None, [])
        );
    }
}

#[test]
fn task_event_runtime_helpers_use_event_and_status_scalars() {
    let compilation = compilation("module app; func main() {}");

    let event = compilation
        .codegen_representation_type(RepresentationRole::ScalarUsize)
        .expect("task event representation must realize");

    let status = compilation
        .codegen_representation_type(RepresentationRole::ScalarU32)
        .expect("status representation must realize");

    let creation = compilation
        .codegen_runtime_signature(RuntimeAbiRole::TaskEventCreation)
        .expect("task event creation signature must realize");

    let signal = compilation
        .codegen_runtime_signature(RuntimeAbiRole::TaskEventSignal)
        .expect("task event signal signature must realize");

    let destruction = compilation
        .codegen_runtime_signature(RuntimeAbiRole::TaskEventDestruction)
        .expect("task event destruction signature must realize");

    assert!(creation.parameters().is_empty());

    assert_eq!(
        creation.result(),
        &CodegenResultMapping::direct(event, None, [])
    );

    for signature in [signal, destruction] {
        assert_eq!(
            signature.parameters(),
            [CodegenParameterMapping::direct(event, None, [])]
        );

        assert_eq!(
            signature.result(),
            &CodegenResultMapping::direct(status, None, [])
        );
    }
}

#[test]
fn generator_runtime_signatures_demand_concrete_opaque_pointers() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let signature = compilation
        .codegen_runtime_signature(RuntimeAbiRole::GeneratorBegin)
        .expect("generator begin signature must realize");

    let Some(CodegenParameterMapping::Direct { ty: pointer, .. }) = signature.parameters().first()
    else {
        panic!("generator begin must receive one direct state pointer");
    };

    let element = compilation
        .compiler_known_type(RepresentationRole::ScalarU8)
        .expect("byte representation must resolve");

    let mappings = realized_types(&compilation, &target, [*pointer]);

    assert!(matches!(
        mappings[pointer].kind(),
        CodegenTypeKind::Pointer {
            target,
            address_space: TargetAddressSpaceKind::Default,
        } if *target == element
    ));
}
