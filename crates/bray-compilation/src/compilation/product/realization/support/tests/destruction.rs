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
fn generated_destruction_composes_parts_in_reverse_order() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let leaf = values
        .intern_type(TypeData::tuple([]))
        .expect("leaf type must intern");

    let aggregate = values
        .intern_type(TypeData::tuple([leaf, leaf]))
        .expect("aggregate type must intern");

    let instance = compilation
        .concrete_codegen_lifecycle(MirHelperReference::Destroy(aggregate), &target)
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
                MirOperationKind::Finalize(_) | MirOperationKind::Destroy(_)
            )
        })
        .collect::<Vec<_>>();

    let expected = [
        ("finalize", 1),
        ("destroy", 1),
        ("finalize", 0),
        ("destroy", 0),
    ];

    assert_eq!(operations.len(), expected.len());

    for (operation, (kind, field)) in operations.into_iter().zip(expected) {
        let place = match operation {
            MirOperationKind::Finalize(place) if kind == "finalize" => place,
            MirOperationKind::Destroy(place) if kind == "destroy" => place,
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
        MirHelperReference::Destroy(report),
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

    let payload = values
        .intern_type(TypeData::tuple([]))
        .expect("payload type must intern");

    let nullable = values
        .intern_type(TypeData::Nullable(payload))
        .expect("nullable type must intern");

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(nullable),
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

    for (operation, expected) in operations.into_iter().zip(["finalize", "destroy"]) {
        let place = match (expected, operation) {
            ("finalize", MirOperationKind::Finalize(place))
            | ("destroy", MirOperationKind::Destroy(place)) => place,
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
        "    Value(value: i32);\n",
        "    Empty;\n",
        "}\n",
        "func main() {}\n",
    ));

    let target = codegen_target(&compilation);
    let union = source_union_type(&compilation);

    let generated = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Destroy(union),
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

    for (operation, expected) in payload_blocks[0].iter().zip(["finalize", "destroy"]) {
        let place = match (expected, operation) {
            ("finalize", MirOperationKind::Finalize(place))
            | ("destroy", MirOperationKind::Destroy(place)) => place,
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
        MirHelperReference::Destroy(generator),
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
        runtime,
        ..
    }) = operation.kind()
    else {
        panic!("generator destruction must use the generator destruction ABI");
    };

    assert_eq!(*operation_element, element);
    assert_eq!(runtime.role(), RuntimeAbiRole::GeneratorDestruction);

    assert_eq!(
        runtime.role().contract().effects(),
        &[RuntimeRoleContractEffect::DestroyGenerator]
    );

    assert_eq!(
        operation.kind().helper_references(),
        [
            MirHelperReference::Finalize(element),
            MirHelperReference::Destroy(element),
        ]
    );

    let owner = compilation
        .concrete_codegen_lifecycle(MirHelperReference::Destroy(generator), &target)
        .expect("outer generator lifecycle instance must realize");

    let dependencies = compilation
        .concrete_codegen_dependencies_for_mir(
            &owner,
            &generated,
            &target,
            &CancellationToken::new(),
        )
        .expect("element lifecycle dependencies must realize");

    let [dependency] = dependencies.as_slice() else {
        panic!("only nontrivial element lifecycle dependencies must remain");
    };

    assert_eq!(
        dependency.instance().generated_lifecycle_reference(),
        Some(&MirHelperReference::Destroy(element))
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
        MirHelperReference::Destroy(owned),
        81,
    );

    assert_eq!(
        generated
            .operations()
            .iter()
            .filter(|operation| matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_))))
            .count(),
        3
    );

    let reference = MirHelperReference::Destroy(owned);

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

            Some((reference.instance(), call.result().ty()))
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
        3,
        "projection, target destruction and release each retain their requirement"
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
            MirOperationKind::Borrow { place, .. } | MirOperationKind::Finalize(place) => place
                .projections()
                .iter()
                .any(|projection| projection.kind() == &MirProjectionKind::OwnedStorage),
            _ => false,
        }
    }));

    let broadcast = generated_lifecycle(
        &compilation,
        &target,
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            ty: owned,
        },
        83,
    );

    assert!(
        broadcast.blocks().iter().any(|block| {
            let MirTerminatorKind::CheckCallOutcome { panicked, .. } = block.terminator().kind()
            else {
                return false;
            };

            matches!(
                broadcast
                    .block(panicked.target())
                    .map(|target| target.terminator().kind()),
                Some(MirTerminatorKind::ContinueCleanup(_))
            )
        }),
        "storage borrow panic must retain its report across cleanup broadcast"
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

        let finalized =
            generated_lifecycle(&compilation, &target, MirHelperReference::Finalize(ty), 190);

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

        let destroyed =
            generated_lifecycle(&compilation, &target, MirHelperReference::Destroy(ty), 191);

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

    let reference = MirHelperReference::Destroy(storage);

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

    let generated =
        generated_lifecycle(&compilation, &target, MirHelperReference::Finalize(ty), 192);

    assert!(generated.is_valid());

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

    let transfers = error
        .operations()
        .iter()
        .filter_map(|id| match generated.operation(*id).unwrap().kind() {
            MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
                incident,
                ..
            }) => Some(incident),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(transfers.len(), 1);

    let bray_ir::MirOperand::Value(payload) = transfers[0] else {
        panic!("transfer consumes the projected payload");
    };

    let projection = error
        .operations()
        .iter()
        .find_map(|id| {
            let operation = generated.operation(*id).unwrap();

            (operation.result() == Some(*payload)).then_some(operation.kind())
        })
        .unwrap();

    assert!(matches!(
        projection,
        MirOperationKind::PatternProjection {
            operation: bray_bound_tree::PatternOperation::Consume,
            ..
        }
    ));

    assert!(success.operations().iter().all(|id| !matches!(
        generated.operation(*id).unwrap().kind(),
        MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident { .. })
    )));

    assert_ne!(panicked.target(), unmatched.target());
    assert_ne!(cancelled.target(), unmatched.target());

    assert!(
        matches!(error.terminator().kind(), MirTerminatorKind::Goto(_)),
        "incident transfer retains ownership without replacing the pending cleanup outcome"
    );

    assert!(
        generated.operations().iter().all(|operation| !matches!(
            operation.kind(),
            MirOperationKind::Destroy(_) | MirOperationKind::Finalize(_)
        )),
        "the transferred payload stays owned by the incident chain"
    );
}
