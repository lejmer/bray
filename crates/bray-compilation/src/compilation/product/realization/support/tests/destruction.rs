use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{
    MirCallTarget, MirCleanupPhase, MirGeneratorOperation, MirHelperReference, MirOperationKind,
    MirProjectionKind, MirTerminatorKind, MirUnitId,
};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeRoleContractEffect};
use bray_symbols::{NamedTypeSymbolId, SymbolOrigin, TypeData};

use super::lifecycle_fixtures::{generated_lifecycle, reachable_cleanup_operations};
use super::targets::codegen_target;
use super::type_fixtures::source_union_type;

use crate::CancellationToken;
use crate::compilation::substitution::named_type;
use crate::test_support::compilation;

#[test]
fn known_abandoned_destruction_excludes_async_receiver_cleanup() {
    let compilation = compilation(
        r#"
        module app;

        struct Owner
        {
            pending: Future<unit>;

            destruct() {}
        }

        func main() {}
    "#,
    );

    assert!(
        !compilation.check_diagnostics().has_errors(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let symbols = compilation.symbol_graph().unwrap();

    let definition = symbols
        .structures()
        .iter()
        .find(|value| value.origin() == SymbolOrigin::Source)
        .unwrap();

    let ty = named_type(
        compilation.semantic_value_store().unwrap(),
        NamedTypeSymbolId::Struct(definition.id()),
    )
    .unwrap();

    let target = codegen_target(&compilation);

    let destroyed = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(ty, bray_ir::MirDestructionDisposition::Abandoned),
        192,
    );

    assert!(destroyed.is_valid());

    assert!(destroyed.operations().iter().all(|operation| !matches!(
        operation.kind(),
        MirOperationKind::Cleanup {
            phase: MirCleanupPhase::LifecycleResolution,
            ..
        } | MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
    )));

    assert!(
        destroyed
            .blocks()
            .iter()
            .all(|block| !matches!(block.terminator().kind(), MirTerminatorKind::Suspend { .. }))
    );

    assert_eq!(
        destroyed
            .operations()
            .iter()
            .filter(|operation| matches!(operation.kind(),
        MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_))))
            .count(),
        1,
        "the source destructor body remains one shared invocation"
    );

    assert!(destroyed.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Destroy { place, abandoned: Some(bray_ir::MirOperand::Immediate {
            value: bray_ir::MirImmediateValue::Boolean(true), .. }), .. }
            if compilation.available_compiler_known_symbols().unary_representation_argument(
                compilation.semantic_value_store().unwrap(), RepresentationRole::Future, place.ty()).is_some())));
}

#[test]
fn generated_destruction_composes_parts_in_reverse_order() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let leaf = compilation
        .compiler_known_type(RepresentationRole::PanicReport)
        .expect("the represented members have real destruction obligations");

    let aggregate = values
        .intern_type(TypeData::tuple([leaf, leaf]))
        .expect("aggregate type must intern");

    let instance = compilation
        .concrete_codegen_lifecycle(
            MirHelperReference::Destroy(aggregate, bray_ir::MirDestructionDisposition::Dynamic),
            &target,
        )
        .expect("destruction instance must realize");

    let reference = instance
        .generated_lifecycle_reference()
        .expect("generated lifecycle payload must be retained");

    let generated = compilation
        .codegen_generated_lifecycle_mir(
            instance.key(),
            reference,
            MirUnitId::new(77),
            &CancellationToken::new(),
        )
        .expect("represented-part destruction must generate");

    let operations = generated
        .operations()
        .iter()
        .map(|operation| operation.kind())
        .filter(|operation| {
            matches!(
                operation,
                MirOperationKind::Cleanup { .. } | MirOperationKind::Destroy { .. }
            )
        })
        .collect::<Vec<_>>();

    let expected = [
        ("destroy", 1),
        ("cleanup", 1),
        ("destroy", 0),
        ("cleanup", 0),
    ];

    assert_eq!(operations.len(), expected.len());

    for (operation, (kind, field)) in operations.into_iter().zip(expected) {
        let place = match operation {
            MirOperationKind::Cleanup { place, .. } if kind == "cleanup" => place,
            MirOperationKind::Destroy { place, .. } if kind == "destroy" => place,
            other => panic!("unexpected lifecycle operation: {other:?}"),
        };

        assert!(matches!(
            place.projections().last().map(|projection| projection.kind()),
            Some(MirProjectionKind::TupleField(actual)) if *actual == field
        ));
    }
}

#[test]
fn generated_panic_report_destruction_uses_the_non_reporting_runtime_role() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let report = compilation
        .compiler_known_type(RepresentationRole::PanicReport)
        .expect("panic report representation must resolve");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(report, bray_ir::MirDestructionDisposition::Dynamic),
        78,
    );

    let runtime_roles = generated.operations().iter().filter_map(|operation| {
        let MirOperationKind::Call(call) = operation.kind() else {
            return None;
        };

        let MirCallTarget::Runtime(runtime) = call.target() else {
            return None;
        };

        Some(runtime.role())
    });

    assert_eq!(
        runtime_roles.collect::<Vec<_>>(),
        [RuntimeAbiRole::PanicReportDestruction]
    );
}

#[test]
fn nullable_lifecycle_resolves_only_the_present_payload() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let payload = compilation
        .compiler_known_type(RepresentationRole::PanicReport)
        .expect("the nullable payload has a real destruction obligation");

    let nullable = values
        .intern_type(TypeData::Nullable(payload))
        .expect("nullable type must intern");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(nullable, bray_ir::MirDestructionDisposition::Dynamic),
        78,
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
        .expect("nullable destruction must branch on presence");

    let absent = generated.block(branch.1).expect("absent block must exist");

    let operations = reachable_cleanup_operations(&generated, branch.0);

    assert_eq!(operations.len(), 2);
    assert!(absent.operations().is_empty());

    for (operation, expected) in operations.into_iter().zip(["destroy", "cleanup"]) {
        let place = match (expected, operation) {
            ("cleanup", MirOperationKind::Cleanup { place, .. })
            | ("destroy", MirOperationKind::Destroy { place, .. }) => place,
            other => panic!("unexpected nullable lifecycle operation: {other:?}"),
        };

        assert!(matches!(
            place
                .projections()
                .last()
                .map(|projection| projection.kind()),
            Some(MirProjectionKind::NullableValue)
        ));
    }
}

#[test]
fn union_lifecycle_resolves_only_the_active_variant_payload() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    Value(value: Child);\n",
        "    Empty;\n",
        "}\n",
        "struct Child { destruct() {} }\n",
        "func main() {}\n",
    ));

    let target = codegen_target(&compilation);
    let union = source_union_type(&compilation);

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(union, bray_ir::MirDestructionDisposition::Dynamic),
        79,
    );

    let mut branches = generated
        .blocks()
        .iter()
        .filter_map(|block| match block.terminator().kind() {
            MirTerminatorKind::PatternBranch {
                predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(variant),
                matched,
                ..
            } => Some((*variant, matched.target())),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(branches.len(), 2);

    branches.sort_by_key(|(variant, _)| *variant);

    let payload_blocks = branches
        .into_iter()
        .map(|(_, block)| reachable_cleanup_operations(&generated, block))
        .collect::<Vec<_>>();

    assert_eq!(
        payload_blocks.iter().map(Vec::len).collect::<Vec<_>>(),
        [2, 0]
    );

    for (operation, expected) in payload_blocks[0].iter().zip(["destroy", "cleanup"]) {
        let place = match (expected, operation) {
            ("cleanup", MirOperationKind::Cleanup { place, .. })
            | ("destroy", MirOperationKind::Destroy { place, .. }) => place,
            other => panic!("unexpected union lifecycle operation: {other:?}"),
        };

        assert!(matches!(
            place
                .projections()
                .last()
                .map(|projection| projection.kind()),
            Some(MirProjectionKind::ActiveUnionPayloadElement { .. })
        ));
    }

    assert!(
        generated
            .blocks()
            .iter()
            .any(|block| { matches!(block.terminator().kind(), MirTerminatorKind::Unreachable) })
    );
}

#[test]
fn generator_destruction_reaches_required_element_lifecycle_and_releases_storage() {
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
        MirHelperReference::Destroy(generator, bray_ir::MirDestructionDisposition::Dynamic),
        80,
    );

    let [operation, discharge] = generated.operations() else {
        panic!("generator destruction must destroy its representation and discharge its owner");
    };

    assert!(
        matches!(discharge.kind(), MirOperationKind::DischargeOutgoing { owner: bray_ir::MirOutgoingOwner::Value(ty), .. } if *ty == generator)
    );

    let MirOperationKind::Generator(MirGeneratorOperation::Destroy {
        element: operation_element,
        abandoned,
        runtime,
        ..
    }) = operation.kind()
    else {
        panic!("generator destruction must use the generator destruction ABI");
    };

    assert_eq!(*operation_element, element);

    assert!(matches!(abandoned, bray_ir::MirOperand::Copy(place)
        if matches!(generated.storage(place.storage()).unwrap().kind(), bray_ir::MirStorageKind::Parameter(1))));

    assert_eq!(runtime.role(), RuntimeAbiRole::GeneratorDestruction);

    assert_eq!(
        runtime.role().contract().effects(),
        &[RuntimeRoleContractEffect::DestroyGenerator]
    );

    assert_eq!(
        operation.kind().helper_references(),
        [
            MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ty: element,
            },
            MirHelperReference::Destroy(element, bray_ir::MirDestructionDisposition::Dynamic),
        ]
    );

    let owner = compilation
        .concrete_codegen_lifecycle(
            MirHelperReference::Destroy(generator, bray_ir::MirDestructionDisposition::Dynamic),
            &target,
        )
        .expect("outer generator lifecycle instance must realize");

    let dependencies = compilation
        .concrete_codegen_dependencies_for_mir(
            &owner,
            &generated,
            &target,
            &CancellationToken::new(),
        )
        .expect("element lifecycle dependencies must realize");

    let references = dependencies
        .iter()
        .map(|dependency| dependency.instance().generated_lifecycle_reference())
        .collect::<Vec<_>>();

    assert_eq!(
        references,
        [
            Some(&MirHelperReference::Destroy(
                element,
                bray_ir::MirDestructionDisposition::Dynamic
            )),
            Some(&MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ty: element,
            }),
        ]
    );
}

#[test]
fn owned_indirection_uses_storage_policy_teardown() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let payload = values
        .intern_type(TypeData::tuple([]))
        .expect("owned payload type must intern");

    let heap_key = CompilerKnownDeclarationKey::try_new("Heap")
        .expect("compiler-known Heap key must validate");

    let heap = compilation
        .available_compiler_known_symbols()
        .declaration_symbol::<bray_symbols::StructSymbolId>(&heap_key)
        .expect("compiler-known Heap must be available");

    let storage =
        named_type(values, NamedTypeSymbolId::Struct(heap)).expect("Heap storage type must intern");

    let owned = values
        .intern_type(TypeData::OwnedIndirection {
            storage,
            target: payload,
        })
        .expect("owned indirection type must intern");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(owned, bray_ir::MirDestructionDisposition::Dynamic),
        81,
    );

    assert_eq!(
        generated
            .operations()
            .iter()
            .filter(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_))))
            .count(),
        2
    );

    let reference = MirHelperReference::Destroy(owned, bray_ir::MirDestructionDisposition::Dynamic);

    let instance = compilation
        .concrete_codegen_lifecycle(reference.clone(), &target)
        .unwrap();

    let cancellation = crate::CancellationToken::new();

    let expanded = compilation
        .codegen_generated_lifecycle(
            instance.key(),
            &reference,
            bray_ir::MirUnitId::new(0),
            &cancellation,
        )
        .unwrap();

    let calls = expanded
        .operations()
        .iter()
        .filter_map(|operation| {
            let MirOperationKind::Call(call) = operation.kind() else {
                return None;
            };

            if !call.is_cleanup() {
                return None;
            }

            let MirCallTarget::Direct(reference) = call.target() else {
                panic!("local cleanup has a selected target");
            };

            Some((reference.instance(), call.result()))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        calls,
        compilation
            .codegen_lifecycle_action(&reference, &cancellation)
            .unwrap()
            .local_calls()
            .map(|call| (call.callable, call.result))
            .collect::<Vec<_>>()
    );

    assert_eq!(
        calls.len(),
        2,
        "projection and storage release retain their selected policy calls"
    );

    compilation
        .owner_outgoing_capacity(owned, &cancellation)
        .unwrap();

    let reused = compilation
        .codegen_generated_lifecycle(
            instance.key(),
            &reference,
            bray_ir::MirUnitId::new(0),
            &cancellation,
        )
        .unwrap();

    assert!(
        std::sync::Arc::ptr_eq(&expanded, &reused),
        "emission must reuse the same generated body"
    );

    assert!(generated.operations().iter().any(|operation| {
        match operation.kind() {
            MirOperationKind::Borrow { place, .. } => place
                .projections()
                .iter()
                .any(|projection| projection.kind() == &MirProjectionKind::OwnedStorage),
            _ => false,
        }
    }));

    assert!(
        compilation
            .codegen_lifecycle_is_trivial(
                &MirHelperReference::Cleanup {
                    phase: MirCleanupPhase::TaskCancellation,
                    ty: owned,
                },
                &cancellation
            )
            .unwrap(),
        "task-free stored targets do not need a projection for broadcast"
    );

    let future = compilation
        .available_compiler_known_symbols()
        .unary_representation_type(
            values,
            bray_compiler_known::RepresentationRole::Future,
            compilation
                .compiler_known_type(bray_compiler_known::RepresentationRole::Unit)
                .unwrap(),
        )
        .unwrap()
        .unwrap();

    let cancellable = values
        .intern_type(TypeData::OwnedIndirection {
            storage,
            target: future,
        })
        .unwrap();

    let broadcast = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            ty: cancellable,
        },
        83,
    );

    let failed = broadcast
        .blocks()
        .iter()
        .find_map(|block| {
            let MirTerminatorKind::CheckCallOutcome { panicked, .. } = block.terminator().kind()
            else {
                return None;
            };

            Some(panicked.target())
        })
        .expect("storage projection checks its failure outcome");

    let reachable = bray_base::transitive_dependencies([failed], |id| {
        let mut successors = Vec::new();

        broadcast
            .block(*id)
            .unwrap()
            .terminator()
            .kind()
            .for_each_successor(|next| successors.push(next));

        successors
    });

    assert!(reachable.iter().any(|id| broadcast.block(*id).unwrap().operations().iter().any(|id|
        matches!(broadcast.operation(*id).unwrap().kind(),
            MirOperationKind::Store { destination, .. }
                if matches!(broadcast.storage(destination.storage()).unwrap().kind(), bray_ir::MirStorageKind::Parameter(2)))
    )), "storage projection failures write the enclosing report");

    assert!(
        reachable.iter().any(|id| matches!(
            broadcast.block(*id).unwrap().terminator().kind(),
            MirTerminatorKind::Return {
                value: None,
                report: None
            }
        )),
        "a failed projection returns to the enclosing traversal"
    );

    assert!(
        broadcast.blocks().iter().all(|block| !matches!(
            block.terminator().kind(),
            MirTerminatorKind::PropagatePanic { .. }
                | MirTerminatorKind::PropagateCancellation { .. }
        )),
        "a borrowed collection is dispatched only by its owner"
    );
}

#[test]
fn completed_unit_finalizer_omits_only_its_invocation() {
    for (execution, result, body, guarantees, calls) in [
        ("", "", "", "executes(pure, total)", 0),
        ("", "", "", "executes(pure)", 1),
        ("", "", "", "executes(total)", 1),
        ("", "", "", "", 1),
        ("async ", "", "", "executes(pure, total)", 0),
        (
            "",
            "-> Result<unit, bool>",
            "return Ok(unit);",
            "executes(pure, total)",
            0,
        ),
        (
            "async ",
            "-> Result<unit, bool>",
            "return Ok(unit);",
            "executes(pure, total)",
            0,
        ),
        (
            "",
            "-> Result<unit, bool>",
            "return Error(true);",
            "executes(pure, total)",
            1,
        ),
    ] {
        let compilation = compilation(&format!(
            r#"
                module app;

                struct Value
                {{
                    {execution}finalize() {result}
                        {guarantees}
                    {{
                        {body}
                    }}

                    destruct()
                    {{
                    }}
                }}
                "#
        ));

        assert!(!compilation.check_diagnostics().has_errors());

        let symbols = compilation.symbol_graph().unwrap();

        let definition = symbols
            .structures()
            .iter()
            .find(|value| value.origin() == SymbolOrigin::Source)
            .unwrap();

        let ty = named_type(
            compilation.semantic_value_store().unwrap(),
            NamedTypeSymbolId::Struct(definition.id()),
        )
        .unwrap();

        let target = codegen_target(&compilation);

        let finalized = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty,
            },
            190,
        );

        assert_eq!(finalized.operations().iter().filter(|operation|
            matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_)))).count(), calls);

        assert_eq!(
            finalized
                .operations()
                .iter()
                .filter(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::TransferCleanupIncident { .. }
                    )
                ))
                .count(),
            usize::from(calls != 0 && !result.is_empty()),
            "only retained fallible finalizer calls transfer an error payload",
        );

        if calls == 0 {
            assert!(finalized.operations().iter().all(|operation| !matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame { .. })
            )));
        }

        let destroyed = generated_lifecycle(
            &compilation,
            &target,
            MirHelperReference::Destroy(ty, bray_ir::MirDestructionDisposition::Dynamic),
            191,
        );

        assert_eq!(destroyed.operations().iter().filter(|operation|
            matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_)))).count(), 1);
    }
}

#[test]
fn cancelling_lifecycle_expansion_owner_preserves_waiters_and_cached_result() {
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    use crate::fact::{FactCellTestEvent, FactCellTestObserver};

    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let heap = compilation
        .available_compiler_known_symbols()
        .declaration_symbol::<bray_symbols::StructSymbolId>(
            &CompilerKnownDeclarationKey::try_new("Heap").unwrap(),
        )
        .unwrap();

    let storage = named_type(
        compilation.semantic_value_store().unwrap(),
        NamedTypeSymbolId::Struct(heap),
    )
    .unwrap();

    let reference =
        MirHelperReference::Destroy(storage, bray_ir::MirDestructionDisposition::Dynamic);

    let instance = compilation
        .concrete_codegen_lifecycle(reference.clone(), &target)
        .unwrap();

    let unit = MirUnitId::new(0);

    let cell = compilation
        .state
        .generated_lifecycle
        .cell((instance.key().clone(), unit))
        .unwrap();

    let (started_tx, started_rx) = mpsc::channel();

    let (waiting_tx, waiting_rx) = mpsc::sync_channel(1);

    let (release_tx, release_rx) = mpsc::channel();

    let release_rx = Mutex::new(release_rx);

    cell.set_test_observer(FactCellTestObserver::new(move |event| match event {
        FactCellTestEvent::Computing => {
            started_tx.send(()).unwrap();

            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
        FactCellTestEvent::Waiting => {
            let _ = waiting_tx.try_send(());
        }
        FactCellTestEvent::Computed => {}
    }))
    .unwrap();

    let owner_token = CancellationToken::new();
    let waiter_token = CancellationToken::new();

    let query = |token: &CancellationToken| {
        compilation.codegen_generated_lifecycle(instance.key(), &reference, unit, token)
    };

    let (owner, waiter) = std::thread::scope(|scope| {
        let owner = scope.spawn(|| query(&owner_token));

        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

        let waiter = scope.spawn(|| query(&waiter_token));

        waiting_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        owner_token.cancel();
        release_tx.send(()).unwrap();

        (owner.join().unwrap(), waiter.join().unwrap())
    });

    assert!(matches!(
        owner,
        Err(crate::compilation::CodegenPreparationError::Query(
            crate::FactQueryError::Cancelled
        ))
    ));

    let waiter = waiter.expect("active waiter must receive a successful expansion");
    let later = query(&CancellationToken::new()).expect("later requests must reuse success");

    assert!(Arc::ptr_eq(&waiter, &later));
}

#[test]
fn generated_finalizer_transfers_only_a_completed_error_payload() {
    let compilation = compilation(
        r#"
        module app;

        struct Failure
        {
            destruct() {}
        }

        struct Value
        {
            finalize() -> Result<unit, Failure>
            {
                return Error(Failure {});
            }
        }
    "#,
    );

    assert!(!compilation.check_diagnostics().has_errors());

    let symbols = compilation.symbol_graph().unwrap();

    let definition = symbols
        .structures()
        .iter()
        .find(|value| value.origin() == SymbolOrigin::Source && !value.finalizers().is_empty())
        .unwrap();

    let ty = named_type(
        compilation.semantic_value_store().unwrap(),
        NamedTypeSymbolId::Struct(definition.id()),
    )
    .unwrap();

    let target = codegen_target(&compilation);

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::LifecycleResolution,
            ty,
        },
        192,
    );

    assert!(generated.is_valid());

    assert_eq!(
        generated
            .operations()
            .iter()
            .filter(|operation| matches!(
                operation.kind(),
                MirOperationKind::Call(call)
                    if matches!(call.target(), MirCallTarget::Runtime(runtime)
                        if runtime.role() == RuntimeAbiRole::CleanupShieldEnter)
            ))
            .count(),
        0,
        "structural helpers borrow the caller's shield and collection",
    );

    let (call_block, call) = generated.blocks_with_ids().find_map(|(id, block)| {
        block.operations().iter().find_map(|operation| {
            let operation = generated.operation(*operation).unwrap();

            matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_)))
                .then(|| (id, operation.result().unwrap()))
        })
    }).unwrap();

    let MirTerminatorKind::CheckCallOutcome {
        completed,
        panicked,
        cancelled,
        completed_report: None,
    } = generated.block(call_block).unwrap().terminator().kind()
    else {
        panic!("the finalizer outcome must be checked before inspecting its Result");
    };

    assert_eq!(completed.arguments(), &[bray_ir::MirOperand::Value(call)]);

    let completed = generated.block(completed.target()).unwrap();

    let MirTerminatorKind::PatternBranch {
        subject,
        matched,
        unmatched,
        ..
    } = completed.terminator().kind()
    else {
        panic!("only successful completion inspects the finalizer Result");
    };

    assert_eq!(
        subject,
        &bray_ir::MirOperand::Value(completed.parameters()[0])
    );

    let success = generated.block(matched.target()).unwrap();
    let error = generated.block(unmatched.target()).unwrap();

    let (transfer_block, incident, producer) = generated
        .blocks_with_ids()
        .find_map(|(id, block)| {
            block.operations().iter().find_map(|operation| {
                match generated.operation(*operation).unwrap().kind() {
                    MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::TransferCleanupIncident {
                            incident,
                            producer,
                            ..
                        },
                    ) => Some((id, incident, producer)),
                    _ => None,
                }
            })
        })
        .expect("completed finalizer error retains one transfer");

    let payload = incident;

    assert!(error.operations().iter().any(|id| matches!(
        generated.operation(*id).unwrap().kind(),
        MirOperationKind::Store { kind: bray_ir::MirStoreKind::Initialize, destination, .. } if destination == payload
    )));

    let MirTerminatorKind::BeginCleanup(broadcast) = error.terminator().kind() else {
        panic!("the newly returned error broadcasts cancellation before waiting");
    };

    let mut broadcast_id = broadcast.edge().target();
    let broadcast = generated.block(broadcast_id).unwrap();

    assert!(broadcast.operations().iter().any(|id| matches!(
        generated.operation(*id).unwrap().kind(),
        MirOperationKind::Cleanup { phase: MirCleanupPhase::TaskCancellation, place, .. } if place == payload
    )));

    let mut seen = std::collections::BTreeSet::new();

    while let MirTerminatorKind::Goto(next) =
        generated.block(broadcast_id).unwrap().terminator().kind()
    {
        assert!(
            seen.insert(broadcast_id),
            "the completed broadcast cannot cycle before child resolution"
        );

        broadcast_id = next.target();

        assert_eq!(
            generated.block(broadcast_id).unwrap().kind(),
            bray_ir::MirBlockKind::CleanupBroadcast
        );
    }

    let MirTerminatorKind::ContinueCleanup(broadcast_complete) =
        generated.block(broadcast_id).unwrap().terminator().kind()
    else {
        panic!("shared broadcast collection proceeds directly to child resolution");
    };

    let (resolution_id, resolution) = generated
        .blocks_with_ids()
        .find(|(_, block)| {
            block.operations().iter().any(|id| {
                matches!(
                    generated.operation(*id).unwrap().kind(),
                    MirOperationKind::Quiesce { place, .. } if place == payload
                )
            })
        })
        .expect("retained error has a quiescence operation");

    assert!(resolution.operations().iter().any(|id| matches!(
        generated.operation(*id).unwrap().kind(),
        MirOperationKind::Quiesce { place, .. } if place == payload
    )));

    assert!(generated.operations().iter().any(|operation| matches!(
        operation.kind(), MirOperationKind::Call(call) if call.target() == &MirCallTarget::Direct(*producer)
    )));

    let owner = compilation
        .concrete_codegen_lifecycle(
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty,
            },
            &target,
        )
        .unwrap();

    let cancellation = CancellationToken::new();
    let mut requirements = Vec::new();

    for (operation_id, operation) in generated.operations_with_ids() {
        if matches!(operation.kind(), MirOperationKind::Call(call) if call.target() == &MirCallTarget::Direct(*producer))
            || matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident { .. })
            )
        {
            requirements.push(
                compilation
                    .operation_outgoing_requirements(
                        &owner,
                        operation_id,
                        operation.kind(),
                        &target,
                        &cancellation,
                    )
                    .unwrap()
                    .unwrap(),
            );
        }
    }

    assert_eq!(requirements.len(), 2);

    assert_eq!(
        requirements[0], requirements[1],
        "retention reuses the producing invocation's exact admitted descriptor"
    );

    assert_eq!(requirements[0].len(), 1);

    let reachable = |start| {
        bray_base::transitive_dependencies([start], |id| {
            let mut successors = Vec::new();

            generated
                .block(*id)
                .unwrap()
                .terminator()
                .kind()
                .for_each_successor(|next| successors.push(next));

            successors
        })
    };

    assert_eq!(broadcast_complete.edge().target(), resolution_id);

    assert!(
        reachable(resolution_id).contains(&transfer_block),
        "shared quiescence retains the payload before transfer on every outcome"
    );

    let collections = generated
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Cleanup {
                place, collection, ..
            }
            | MirOperationKind::Quiesce { place, collection }
                if place == payload =>
            {
                Some(
                    collection
                        .as_deref()
                        .expect("structural cleanup borrows its caller collection"),
                )
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(collections.len(), 2);
    assert_eq!(collections[0], collections[1]);

    assert!(generated.operations().iter().any(|operation| matches!(operation.kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident { report, .. })
            if report == collections[0].report()
    )), "quiescence and transfer append to the same enclosing report");

    for target in [panicked.target(), cancelled.target(), matched.target()] {
        assert!(
            !reachable(target).contains(&transfer_block),
            "only a returned Error owns an incident payload"
        );
    }

    let (destroy_id, destroyed, abandoned) = generated
        .blocks_with_ids()
        .find_map(|(id, block)| {
            block.operations().iter().find_map(|operation| {
                match generated.operation(*operation).unwrap().kind() {
                    MirOperationKind::Destroy {
                        place,
                        abandoned: Some(abandoned),
                        ..
                    } if place.ty() == ty => Some((id, place, abandoned)),
                    _ => None,
                }
            })
        })
        .expect("owner is destroyed after finalization");

    let (pending, ordinary) = generated
        .blocks()
        .iter()
        .find_map(|block| match block.terminator().kind() {
            MirTerminatorKind::Branch {
                then_edge,
                else_edge,
                ..
            } if else_edge.target() == destroy_id => Some((then_edge.target(), else_edge.target())),
            _ => None,
        })
        .expect("finalizer disposition selects abandonment");

    assert_eq!(
        ordinary, destroy_id,
        "successful finalization proceeds directly to destruction"
    );

    assert_eq!(
        abandoned,
        &bray_ir::MirOperand::Value(generated.block(destroy_id).unwrap().parameters()[0]),
        "each outcome passes its disposition into destruction"
    );

    let quiescence = generated
        .blocks_with_ids()
        .find_map(|(id, block)| {
            block
                .operations()
                .iter()
                .any(|operation| {
                    matches!(generated.operation(*operation).unwrap().kind(),
            MirOperationKind::Quiesce { place, .. } if place == destroyed)
                })
                .then_some(id)
        })
        .expect("abandonment resolves the owner's current child graph");

    assert!(reachable(pending).contains(&quiescence));
    assert!(reachable(quiescence).contains(&destroy_id));
    assert!(!reachable(ordinary).contains(&quiescence));

    assert!(success.operations().iter().all(|id| !matches!(
        generated.operation(*id).unwrap().kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident { .. })
    )));

    assert!(
        generated.operations().iter().all(|operation| !matches!(
            operation.kind(),
            MirOperationKind::Destroy { place, .. } if place == payload
        )),
        "the transferred payload stays owned by the incident chain"
    );
}
