use crate::test_support::{compilation, source_function_body_key};
use crate::Compilation;
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
fn iteration_cursors_use_expression_cleanup_scopes() {
    use bray_bound_tree::{BoundExecutionSite, BoundExpression, StorageScope, StorageScopeOwners};

    for body in [
        "for value in items {}",
        "for value in items { break; }",
        "for value in items { continue; }",
        "for value in items { return; }",
        r#"for value in items { panic("iteration"); }"#,
        "let result: bool = for value in items { break value; } else { break false; };",
        "let result: bool = all(items);",
        "let result: bool = any(items);",
        "let result = { each value in items { yield value; } };",
    ] {
        let source = format!(
            r#"
            module app;
            using core.atomic;
            static COUNT: core.atomic.Atomic<usize> = core.atomic.initialize<usize>(0);
            struct Items {{}}
            struct IterationCursor {{
                mut pending: bool;

                async finalize() {{
                    let previous = core.atomic.load<usize, 1>(&COUNT);
                    core.atomic.store<usize, 4>(&COUNT, previous + 1);
                }}
            }}
            impl &Items(Iterable) {{
                type Element = bool;
                type Cursor = IterationCursor;
                consume func iterate() -> IterationCursor {{ return IterationCursor {{ pending = true }}; }}
            }}
            impl IterationCursor(Iterator) {{
                type Element = bool;
                mut func next() -> bool? {{
                    if !self.pending {{ return none; }}

                    self.pending = false;

                    return true;
                }}
            }}
            async func probe() {{ let items = Items {{}}; {body} }}
            "#
        );

        let compilation = compilation(&source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{body}: {:?}",
            compilation.check_diagnostics()
        );

        let key = source_function_body_key(&compilation, "probe");

        let unit = compilation
            .bound_unit(key.clone())
            .expect("iteration body binds");

        let owners = StorageScopeOwners::collect(unit.value()).expect("iteration scopes nest");

        let iterations = unit
            .value()
            .tree()
            .expressions()
            .filter_map(|(id, expression)| {
                StorageScope::for_iteration(id, expression).map(|scope| (id, expression, scope))
            })
            .collect::<Vec<_>>();

        assert_eq!(iterations.len(), 1, "{body}");

        let (iteration, expression, scope) = iterations[0];

        assert_eq!(
            owners.scope(Some(StorageIdentity::InvocationResult(
                BoundExecutionSite::Iterate(iteration)
            ))),
            Some(scope),
            "{body}"
        );

        let body_scope = match expression {
            BoundExpression::For(expression) => Some(expression.body()),
            BoundExpression::Generator(expression) => Some(expression.body()),
            _ => None,
        };

        if let Some(body_scope) = body_scope {
            assert_eq!(
                owners.scope(Some(StorageIdentity::InvocationResult(
                    BoundExecutionSite::Next(iteration)
                ))),
                Some(body_scope.into()),
                "{body}"
            );
        }

        let storage = compilation
            .storage_plan(key.clone())
            .expect("cursor storage publishes");

        let cleanup = compilation
            .storage_flow(key.clone())
            .expect("cursor cleanup publishes");

        assert!(
            cleanup.value().exits().iter().any(|exit| {
                exit.scope() == scope
                    && exit.lifecycle_resolution().iter().any(|access| {
                        storage
                            .value()
                            .root_identity(*access)
                            .and_then(|identity| storage.value().identity(identity))
                            == Some(StorageIdentity::InvocationResult(
                                BoundExecutionSite::Iterate(iteration),
                            ))
                    })
            }),
            "{body}: {:?}",
            cleanup.value()
        );

        let lowered = compilation
            .lowered_unit(key)
            .expect("iteration cleanup lowers");

        assert!(
            lowered
                .value()
                .as_ref()
                .and_then(|unit| unit.mir())
                .is_some_and(|mir| mir.is_valid()),
            "{body}: {lowered:?}"
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

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

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
        .storage_flow(key)
        .unwrap_or_else(|error| panic!("storage flow must publish: {error:?}"));

    assert!(
        analysis
            .value()
            .exits()
            .iter()
            .any(|exit| { !exit.lifecycle_resolution().is_empty() }),
        "plans={:#?}",
        analysis.value().exits()
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
        .storage_flow(key)
        .unwrap_or_else(|error| panic!("storage flow must publish: {error:?}"));

    let lifecycle_identities = analysis
        .value()
        .exits()
        .iter()
        .flat_map(bray_bound_tree::StorageExitDecision::lifecycle_resolution)
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

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");

        let mir = lowered
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR");

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

        let analysis = compilation
            .async_analysis(key.clone())
            .expect("fixture callable must complete async analysis");

        let partitions = analysis
            .value()
            .storage_requirements()
            .iter()
            .filter_map(bray_bound_tree::AsyncStorageRequirement::parts)
            .collect::<Vec<_>>();

        assert_eq!(partitions.len(), 1, "{name}: {analysis:?}");
        assert_eq!(partitions[0].len(), 2, "{name}: {analysis:?}");

        let lowered = compilation
            .lowered_unit(key)
            .expect("fixture callable must lower successfully");

        assert!(lowered.value().is_some(), "{name}: {lowered:?}");

        let mir = lowered
            .value()
            .as_ref()
            .expect("fixture optional value must be present")
            .mir()
            .expect("fixture instance must retain its MIR");

        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                bray_ir::MirOperationKind::Cleanup { place, .. } if !place.projections().is_empty()
            )));
    }
}

#[test]
fn differing_complete_moves_exclude_cleanup_even_at_a_mixed_exit() {
    let source = r#"module app;
struct Guard { value: i32; destruct() {} }
struct Pair { whole: bool; mut left: Guard; mut right: Guard; }
struct Remaining { destruct() {} }
func take_pair(pos pair: Pair) {}
func take_guard(pos guard: Guard) {}
func finish(pos pair: Pair%remaining%) -> i32
{
    let choice: i32 = if pair.whole
    {
        take_pair(pair);
        yield 11;
    }
    else
    {
        take_guard(pair.left);
        take_guard(pair.right);
        yield 22;
    };

    return choice;
}
"#;

    for (remaining, expected) in [("", 0), (", pos remaining: Remaining", 1)] {
        let compilation = compilation(&source.replace("%remaining%", remaining));

        assert_moved_storage_cleanup(&compilation, expected);
    }
}

fn assert_moved_storage_cleanup(compilation: &Compilation, expected: usize) {
    let diagnostics = compilation.check_diagnostics();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let key = source_function_body_key(compilation, "finish");
    let unit = compilation.bound_unit(key.clone()).expect("finish has a bound body");
    let storage = compilation.storage_plan(key.clone()).expect("finish has planned storage");

    let parameters = storage.value().identity_entries()
        .filter_map(|(id, identity)| matches!(identity, StorageIdentity::Parameter(_)).then_some(id))
        .collect::<Vec<_>>();

    let pair = parameters[0];
    let remaining = &parameters[1..];

    assert_eq!(remaining.len(), expected);

    let asynchronous = compilation.async_analysis(key.clone()).expect("finish has checked cleanup");

    let owner = asynchronous.value().storage_requirement(pair).and_then(|requirement| requirement.owner())
        .expect("the parameter belongs to the callable scope");

    let flow = compilation.storage_flow(key.clone()).expect("finish has checked storage flow");

    let exit = flow.value().exits().iter().find(|exit| {
        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = exit.exit() else { return false; };

        exit.scope() == owner && matches!(unit.value().view().expression(expression),
            Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer))
                if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Return)
    }).expect("both value paths reach the same callable return");

    assert!(exit.moved().iter().any(|path| path.root == pair && !path.projections.is_empty()));
    assert!(!exit.fully_moved().contains(&pair));

    let scheduled = exit.lifecycle_resolution().iter()
        .map(|access| storage.value().root_identity(*access).expect("scheduled cleanup owns storage"))
        .collect::<Vec<_>>();

    assert_eq!(scheduled, remaining);
    assert_eq!(exit.has_cleanup(), !remaining.is_empty());

    let lowered = compilation.lowered_unit(key).expect("the checked return lowers");

    let mir = lowered.value().as_ref().expect("finish has a lowered body")
        .mir().expect("finish retains its MIR");

    let mut disposed_parameters = std::collections::BTreeSet::new();

    for (id, block) in mir.blocks_with_ids() {
        let parameters = block.operations().iter().filter_map(|operation| {
            let bray_ir::MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution, place, ..
            } = mir.operation(*operation).expect("cleanup block retains its operation").kind() else {
                return None;
            };

            let bray_ir::MirStorageKind::Parameter(ordinal) =
                *mir.storage(place.storage()).expect("cleanup retains its owner").kind()
            else {
                return None;
            };

            Some(ordinal)
        }).collect::<Vec<_>>();

        if !parameters.is_empty()
            && bray_ir::reachable_blocks(mir, [id], &Default::default()).into_iter().any(|id| {
                matches!(mir.block(id).expect("MIR successor exists").terminator().kind(), bray_ir::MirTerminatorKind::Return { .. })
            })
        {
            disposed_parameters.extend(parameters);
        }
    }

    assert_eq!(disposed_parameters.into_iter().collect::<Vec<_>>(), if remaining.is_empty() { vec![] } else { vec![1] });
}

#[test]
fn failing_calls_transfer_owned_inputs_without_initializing_results() {
    let compilation = compilation(
        r#"
        module app;
        struct Guard { destruct() {} }
        func take(pos value: Guard, pos fail: bool) -> Guard {
            if fail { panic("failed"); }
            return value;
        }
        func caller(pos value: Guard, pos fail: bool) {
            let returned = take(value, fail);
        }
    "#,
    );

    let key = source_function_body_key(&compilation, "caller");

    let bound = compilation
        .bound_unit(key.clone())
        .expect("fixture callable must bind successfully");

    let storage = compilation
        .storage_plan(key.clone())
        .expect("fixture callable must have a storage plan");

    let flow = compilation
        .storage_flow(key)
        .expect("fixture callable must complete storage-flow analysis");

    let call = bound
        .value()
        .tree()
        .expressions()
        .find_map(|(id, expression)| {
            matches!(expression, bray_bound_tree::BoundExpression::Call(_)).then_some(id)
        })
        .expect("fixture selection must find its expected value");

    let argument = storage
        .value()
        .access_plans()
        .iter()
        .find(|plan| {
            storage.value().effective_purpose(**plan) == bray_bound_tree::StorageAccessPurpose::Move
        })
        .map(|plan| {
            storage
                .value()
                .root_identity(plan.access())
                .expect("fixture `root_identity` call must succeed")
        });

    assert!(argument.is_some(), "{storage:?}");

    let argument = argument.expect("fixture `argument` call must succeed");

    let failures = flow
        .value()
        .exits()
        .iter()
        .filter(|exit| exit.exit() == call.into())
        .collect::<Vec<_>>();

    assert!(!failures.is_empty(), "{flow:?}");

    let result = storage
        .value()
        .identity_entries()
        .find_map(|(id, identity)| {
            (identity == bray_bound_tree::StorageIdentity::Temporary(call)).then_some(id)
        })
        .expect("fixture selection must find its expected value");

    for failure in failures {
        assert!(failure.fully_moved().contains(&argument), "{failure:?}");
        assert!(!failure.initialized().contains(&argument), "{failure:?}");
        assert!(!failure.live().contains(&result), "{failure:?}");
    }

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}
