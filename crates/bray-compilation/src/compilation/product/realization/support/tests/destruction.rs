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
        matches!(discharge.kind(), MirOperationKind::DischargeOutgoing { ty, .. } if *ty == generator)
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
    for (guarantees, calls) in [
        ("executes(pure, total)", 0),
        ("executes(pure)", 1),
        ("executes(total)", 1),
        ("", 1),
    ] {
        let compilation = compilation(&format!(
            r#"
                module app;

                struct Value
                {{
                    finalize()
                        {guarantees}
                    {{
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

        let destroyed =
            generated_lifecycle(&compilation, &target, MirHelperReference::Destroy(ty), 191);

        assert_eq!(destroyed.operations().iter().filter(|operation|
            matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), MirCallTarget::Direct(_)))).count(), 1);
    }
}
