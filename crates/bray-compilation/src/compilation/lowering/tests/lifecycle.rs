use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_ir::{MirOperationKind, MirTerminatorKind};

use super::support::lowered_mir;
use crate::test_support::{
    compilation, source_function_body_key, source_type_callable_member_body_key,
};

#[test]
fn destructor_receiver_move_admits_before_transfer() {
    let compilation = compilation(
        r#"
            module app;
            struct Value {
                mut again: bool;
                destruct() {
                    if self.again {
                        self.again = false;
                        take(self);
                        self = Value { again = false };
                        take(self);
                    }
                }
            }
            func take(pos value: Value) {}
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    let key = compilation
        .declared_unit_keys_for_test()
        .unwrap()
        .into_iter()
        .find(|key| {
            key.kind() == BoundUnitKind::CallableBody
                && key.declared_owner().kind() == bray_symbols::SymbolKind::Destructor
        })
        .unwrap();

    let result = compilation.lowered_unit(key).unwrap();
    let mir = lowered_mir(&result);

    let admission = mir
        .blocks()
        .iter()
        .find(|block| {
            block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::AdmitOutgoing { .. }
                )
            })
        })
        .expect("whole receiver transfer must be admitted");

    assert!(matches!(
        admission.terminator().kind(),
        MirTerminatorKind::CheckCallOutcome { .. }
    ));

    assert_eq!(
        mir.operations()
            .iter()
            .filter(|operation| matches!(operation.kind(), MirOperationKind::AdmitOutgoing { .. }))
            .count(),
        3
    );

    assert_eq!(
        mir.operations()
            .iter()
            .filter(|operation| matches!(
                operation.kind(),
                MirOperationKind::DischargeOutgoing { .. }
            ))
            .count(),
        1
    );
}

#[test]
fn default_failure_keeps_initialized_inputs_in_cleanup() {
    for consumer in [
        r#"
                struct Value { guard: Guard; number: i32 = rejected_default(); }
                func main() { let value = Value { guard = Guard {} }; }
            "#,
        r#"
                func accept_guard(pos guard: Guard, number: i32 = rejected_default()) {}
                func main() { accept_guard(Guard {}); }
            "#,
        r#"
                async func accept_guard(pos guard: Guard, number: i32 = rejected_default()) {}
                func main() {
                    let result = catch { let pending = accept_guard(Guard {}); yield unit; };
                }
            "#,
    ] {
        let source = format!(
            r#"
                module app;
                struct Guard {{ destruct() {{}} }}
                func rejected_default() -> i32 {{ panic("default failed"); }}
                {consumer}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "main");

        let result = compilation
            .lowered_unit(key)
            .expect("construction failure must have an executable cleanup path");

        let mir = lowered_mir(&result);

        let (block, edge) = mir.blocks().iter().find_map(|block| {
        let operation = block.operations().last().and_then(|id| mir.operation(*id))?;

        if !matches!(operation.kind(), MirOperationKind::Call(call) if matches!(call.target(), bray_ir::MirCallTarget::DefaultValue { .. })) { return None; }

        let MirTerminatorKind::CheckCallOutcome { panicked, .. } = block.terminator().kind() else { return None; };

        Some((block, panicked.clone()))
    }).expect("default must retain a failure edge before construction");

        assert!(!block.operations().iter().any(|id| matches!(mir.operation(*id).unwrap().kind(), MirOperationKind::Construct(construction) if construction.inputs().len() == 2)));

        let mut pending = vec![edge.target()];
        let mut seen = BTreeSet::new();
        let mut cleans_input = false;

        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }

            let block = mir.block(id).unwrap();

            cleans_input |= block.operations().iter().any(|id| {
                matches!(
                    mir.operation(*id).unwrap().kind(),
                    MirOperationKind::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
                )
            });

            block
                .terminator()
                .kind()
                .for_each_successor(|target| pending.push(target));
        }

        assert!(
            cleans_input,
            "default failure must reach lifecycle cleanup for the initialized guard"
        );
    }
}

#[test]
fn generic_union_returns_preserve_the_callable_result_through_cleanup() {
    let compilation = compilation(
        r#"module app;

union Choice<T>
{
    Value(pos value: T);
    Empty;
}

struct Guard
{
    destruct() {}
}

struct Receiver<T>
{
    async func receive(pos value: T) -> Choice<T>
    {
        let guard: Guard = Guard {};

        return Choice.Empty;
    }
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let lowered = compilation
        .lowered_unit(source_type_callable_member_body_key(
            &compilation,
            "receive",
        ))
        .unwrap_or_else(|error| panic!("generic union return must lower: {error:?}"));

    assert!(lowered.value().is_some(), "{lowered:#?}");
}

#[test]
fn cleanup_materializes_every_verified_lifecycle_storage() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Resource\n",
        "{\n",
        "    destruct()\n",
        "    {\n",
        "    }\n",
        "}\n",
        "union OpenError\n",
        "{\n",
        "    Failed;\n",
        "}\n",
        "func open() -> Result<Resource, OpenError>\n",
        "{\n",
        "    return Ok(Resource {});\n",
        "}\n",
        "func read(pos text: &string) -> bool\n",
        "{\n",
        "    return true;\n",
        "}\n",
        "func main() -> Result<unit, OpenError>\n",
        "{\n",
        "    let resource: Resource = try open();\n",
        "    let observed: bool = read(&\"borrowed\");\n",
        "    return Ok(unit);\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "main");
    let storage = compilation.storage_plan(key.clone()).unwrap();
    let analysis = compilation.async_analysis(key.clone()).unwrap();

    let required = analysis
        .value()
        .scope_exits()
        .iter()
        .flat_map(|exit| {
            exit.cancellation_broadcast()
                .iter()
                .chain(exit.lifecycle_resolution())
        })
        .filter_map(|access| storage.value().root_identity(*access))
        .collect::<BTreeSet<_>>();

    let lowered = compilation
        .lowered_unit(key)
        .unwrap_or_else(|error| panic!("resource cleanup must lower: {error:?}"));

    assert!(
        lowered.diagnostics().is_empty(),
        "{:#?}",
        lowered.diagnostics()
    );

    let cleanup_places = lowered_mir(&lowered)
        .operations()
        .iter()
        .filter_map(|operation| match operation.kind() {
            MirOperationKind::Cleanup { place, .. } => Some((place.storage(), place.ty())),
            _ => None,
        })
        .collect::<Vec<_>>();

    let cleanup_storages = cleanup_places
        .iter()
        .filter(|(storage, _)| {
            lowered_mir(&lowered).storage(*storage).unwrap().kind()
                != &bray_ir::MirStorageKind::Return
        })
        .map(|(storage, _)| *storage)
        .collect::<BTreeSet<_>>();

    assert_eq!(cleanup_storages.len(), required.len(), "{cleanup_places:?}");
}
