use crate::test_support::{compilation, source_function_body_key};
use bray_bound_tree::StorageIdentity;

#[test]
fn earlier_operands_survive_later_short_circuit_and_conditional_blocks() {
    for body in [
        "return (value == first) == (first || second);",
        "return compare(value == first, first && second);",
        "let values: [bool; 2] = [value == first, first || second]; return values[0];",
        "return (value == first) == (if first { yield second; } else { yield value; });",
    ] {
        let source = format!(
            "module app; func compare(pos left: bool, pos right: bool) -> bool {{ return left == right; }} func probe(pos value: bool, pos first: bool, pos second: bool) -> bool {{ {body} }}"
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "probe");

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("{source}: {error:?}"));

        assert!(
            lowered
                .value()
                .as_ref()
                .and_then(|unit| unit.mir())
                .is_some(),
            "{source}: {lowered:?}"
        );
    }
}

#[test]
fn partial_cleanup_ignores_repaired_moves_at_retained_inner_exits() {
    for repair in ["outer.inner.first = replacement;", ""] {
        let source = format!(
            "module app; struct Guard {{ destruct() {{}} }} struct Inner {{ mut first: Guard; second: Guard; }} struct Outer {{ mut inner: Inner; other: Guard; }} func take(pos value: Guard) {{}} func probe(pos mut outer: Outer, pos replacement: Guard, pos flag: bool) {{ {{ let taken: Guard = outer.inner.first; }} {repair} if flag {{ take(outer.other); }} }}",
        );

        let compilation = compilation(&source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "probe");
        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(
            lowered
                .value()
                .as_ref()
                .and_then(|unit| unit.mir())
                .is_some(),
            "{source}: {lowered:?}"
        );
    }
}

#[test]
fn scope_exits_destroy_plain_storage_with_declared_destructors() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Lock\n",
        "{\n",
        "    acquired: bool;\n",
        "    destruct()\n",
        "    {\n",
        "    }\n",
        "}\n",
        "func use_lock()\n",
        "{\n",
        "    let lock: Lock = Lock\n",
        "    {\n",
        "        acquired = true,\n",
        "    };\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "use_lock");

    let analysis = compilation
        .async_analysis(key)
        .unwrap_or_else(|error| panic!("lifecycle analysis must publish: {error:?}"));

    assert!(
        analysis
            .value()
            .scope_exits()
            .iter()
            .any(|exit| { !exit.lifecycle_resolution().is_empty() }),
        "plans={:#?}",
        analysis.value().scope_exits()
    );
}

#[test]
fn trust_boundaries_preserve_value_transfer_ownership() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Guard\n",
        "{\n",
        "    value: bool;\n",
        "    destruct() {}\n",
        "}\n",
        "trusted internal func make() -> Guard\n",
        "{\n",
        "    return Guard { value = true };\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let guard: Guard = trusted internal make();\n",
        "    guard;\n",
        "}\n",
    ));

    let key = source_function_body_key(&compilation, "main");

    let storage = compilation
        .storage_plan(key.clone())
        .unwrap_or_else(|error| panic!("storage plan must publish: {error:?}"));

    let analysis = compilation
        .async_analysis(key)
        .unwrap_or_else(|error| panic!("lifecycle analysis must publish: {error:?}"));

    let lifecycle_identities = analysis
        .value()
        .scope_exits()
        .iter()
        .flat_map(bray_bound_tree::AsyncScopeExitPlan::lifecycle_resolution)
        .filter_map(|access| storage.value().root_identity(*access))
        .filter_map(|identity| storage.value().identity(identity))
        .collect::<Vec<_>>();

    assert_eq!(
        lifecycle_identities
            .iter()
            .filter(|identity| matches!(identity, StorageIdentity::LocalOwned(_)))
            .count(),
        1
    );

    assert!(
        lifecycle_identities
            .iter()
            .all(|identity| !matches!(identity, StorageIdentity::Temporary(_)))
    );
}

#[test]
fn conditional_whole_moves_lower_guarded_cleanup_and_preserve_return_values() {
    let compilation = compilation(
        r#"module app;
struct Guard
{
    value: bool;
    destruct() {}
}
func take(pos guard: Guard) {}
func conditional(pos flag: bool, pos guard: Guard) -> i32
{
    if flag { take(guard); }
    return 7;
}
func reinitialized(pos flag: bool, pos mut guard: Guard) -> i32
{
    if flag
    {
        take(guard);
        guard = Guard { value = true };
    }
    return 9;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in ["conditional", "reinitialized"] {
        let key = source_function_body_key(&compilation, name);
        let analysis = compilation.async_analysis(key.clone()).unwrap();
        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        if name == "conditional" {
            assert!(mir.blocks().iter().any(|block| {
                block.kind() == bray_ir::MirBlockKind::LifecycleResolution
                    && matches!(
                        block.terminator().kind(),
                        bray_ir::MirTerminatorKind::Branch {
                            condition: bray_ir::MirOperand::Copy(_),
                            ..
                        }
                    )
            }));

            assert!(
                analysis
                    .value()
                    .scope_exits()
                    .iter()
                    .flat_map(|exit| exit.storage())
                    .any(|decision| matches!(
                        decision.disposition(),
                        bray_bound_tree::AsyncStorageExitDisposition::Cleanup {
                            guard: bray_bound_tree::AsyncCleanupGuard::Initialized,
                            ..
                        }
                    ))
            );
        }
    }
}

#[test]
fn projected_moves_lower_guarded_remainder_cleanup() {
    let compilation = compilation(
        r#"module app;
struct Guard { value: i32; destruct() {} }
struct Pair { mut left: Guard; right: Guard; }
func take(pos guard: Guard) {}
func projected(pos flag: bool, pos pair: Pair) -> i32
{
    if flag { take(pair.left); }
    return 7;
}
func tupled(pos flag: bool, pos pair: (Guard, Guard)) -> i32
{
    if flag { take(pair.0); }
    return 9;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in ["projected", "tupled"] {
        let key = source_function_body_key(&compilation, name);
        let analysis = compilation.async_analysis(key.clone()).unwrap();

        let partitions = analysis
            .value()
            .storage_requirements()
            .iter()
            .filter_map(bray_bound_tree::AsyncStorageRequirement::parts)
            .collect::<Vec<_>>();

        assert_eq!(partitions.len(), 1, "{name}: {analysis:?}");
        assert_eq!(partitions[0].len(), 2, "{name}: {analysis:?}");

        let lowered = compilation.lowered_unit(key).unwrap();

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");

        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                bray_ir::MirOperationKind::Cleanup { place, .. } if !place.projections().is_empty()
            )));
    }
}
