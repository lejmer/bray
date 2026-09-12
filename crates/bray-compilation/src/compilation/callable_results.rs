use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

use crate::test_support::compilation;

#[test]
fn callable_results_reject_reachable_fallthrough() {
    for source in [
        r#"
            module app;

            func missing() -> bool {}
        "#,
        r#"
            trusted module app;

            trusted func missing() -> bool {}
        "#,
        r#"
            module app;

            func missing(pos condition: bool) -> bool
            {
                if condition
                {
                    return true;
                }
            }
        "#,
        r#"
            module app;

            func outer()
            {
                let missing = lambda() -> bool {};
            }
        "#,
        r#"
            module app;

            func missing() -> bool
            {
                let inner = lambda() -> bool
                {
                    return true;
                };
            }
        "#,
        r#"
            module app;

            func missing() -> never {}
        "#,
        r#"
            module app;

            struct Value
            {
                construct() -> Self {}
            }
        "#,
        r#"
            module app;

            func missing() -> bool
            {
                let value: bool =
                {
                    yield true;
                };
            }
        "#,
        r#"
            module app;

            struct Value
            {
                func missing() -> bool {}
            }
        "#,
        r#"
            module app;

            async func missing() -> bool {}
        "#,
    ] {
        let compilation = compilation(source);

        assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingCallableResultRequired,
        );
    }
}

#[test]
fn callable_results_accept_complete_bodies() {
    for source in [
        r#"
            module app;

            func implicit() {}

            func explicit() -> unit {}

            func bare()
            {
                return;
            }
        "#,
        r#"
            module app;

            func complete(pos condition: bool) -> bool
            {
                if condition
                {
                    return true;
                }
                else
                {
                    return false;
                }
            }
        "#,
        r#"
            module app;

            func complete() -> bool
            {
                loop {}
            }
        "#,
        r#"
            module app;

            func stop() -> never
            {
                loop {}
            }

            func complete() -> bool
            {
                stop();
            }
        "#,
        r#"
            module app;

            func complete(pos condition: bool) -> bool
            {
                match condition
                {
                    case true { return false; }
                    case false { return true; }
                };
            }
        "#,
        r#"
            module app;

            func outer()
            {
                let complete = lambda() -> bool
                {
                    return true;
                };
            }
        "#,
        r#"
            module app;

            struct Value
            {
                construct() -> Self
                {
                    return Value
                    {
                    };
                }

                func complete() -> bool
                {
                    return true;
                }
            }
        "#,
        r#"
            module app;

            func complete() -> bool
            {
                panic("stop");
            }
        "#,
        r#"
            module app;

            func complete(pos condition: bool) -> bool
            {
                loop
                {
                    if condition
                    {
                        break;
                    }

                    return true;
                }

                return false;
            }
        "#,
        r#"
            module app;

            func complete() -> bool
            {
                assert(false);
            }

            func forever() -> bool
            {
                while true {}
            }

            func literal_branch() -> bool
            {
                if true
                {
                    return true;
                }
            }
        "#,
    ] {
        let compilation = compilation(source);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{source}\n{:?}",
            compilation.check_diagnostics()
        );
    }
}

#[test]
fn callable_results_render_source_context() {
    let compilation = compilation(
        r#"
            module app;

            func missing() -> bool {}
        "#,
    );

    let diagnostic = compilation
        .check_diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingCallableResultRequired)
        .unwrap();

    let rendered = bray_messages::DiagnosticRenderer::english().render(diagnostic);

    assert_eq!(
        rendered.message(),
        "'missing' can reach the end of its body without returning bool"
    );

    assert_eq!(
        rendered.labels()[0].message(),
        "a reachable path ends without returning a result"
    );
}

#[test]
fn callable_results_invalidate_execution_certificates() {
    let compilation = compilation(
        r#"
            module app;

            func missing() -> bool
                executes(pure, total) {}
        "#,
    );

    let key = crate::test_support::source_function_body_key(&compilation, "missing");
    let proof = compilation.execution_properties(key).unwrap();
    assert!(proof.value().properties.is_empty());

    assert_goal_state_diagnostic_kind(
        proof.diagnostics(),
        DiagnosticKind::CheckingCallableResultRequired,
    );
}

#[test]
fn callable_results_do_not_replace_explicit_return_type_errors() {
    let compilation = compilation(
        r#"
            module app;

            func wrong() -> bool
            {
                return;
            }
        "#,
    );

    assert!(compilation.check_diagnostics().has_errors());

    assert!(
        !compilation
            .check_diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingCallableResultRequired)
    );
}

#[test]
fn callable_results_preserve_caught_cleanup_exits() {
    let source = r#"
        module app;

        struct Guard
        {
            destruct()
            {
                panic("cleanup");
            }
        }

        func missing(pos value: Guard) -> bool
        {
            let result = catch
            {
                let local = value;

                return true;
            };
        }
    "#;

    let rejected = compilation(source);

    assert_goal_state_diagnostic_kind(
        rejected.check_diagnostics(),
        DiagnosticKind::CheckingCallableResultRequired,
    );

    let completed = compilation(&source.replace(
        "            };",
        "            };\n            return false;",
    ));

    assert!(
        completed.check_diagnostics().is_empty(),
        "{:?}",
        completed.check_diagnostics()
    );
}

#[test]
fn callable_results_exclude_transferred_cleanup() {
    let compilation = compilation(
        r#"
            module app;

            struct Guard
            {
                destruct()
                {
                    panic("cleanup");
                }
            }

            func complete(pos value: Guard) -> Guard
            {
                let result = catch
                {
                    let guard = value;

                    return guard;
                };
            }

            func conditional(pos value: Guard, pos condition: bool) -> Guard
            {
                let result = catch
                {
                    if condition
                    {
                        return value;
                    }

                    let guard = value;

                    return guard;
                };
            }

            func nested(pos value: Guard) -> Guard
            {
                let result = catch
                {
                    let guard =
                    {
                        let inner = value;

                        yield inner;
                    };

                    return guard;
                };
            }

            struct Packet
            {
                guard: Guard;
                flag: bool;
            }

            func partial(pos value: Packet) -> Guard
            {
                let result = catch
                {
                    let packet = value;

                    return packet.guard;
                };
            }

            func joined(pos value: Packet, pos condition: bool) -> Guard
            {
                let result = catch
                {
                    let packet = value;
                    let guard = if condition
                    {
                        yield packet.guard;
                    }
                    else
                    {
                        yield packet.guard;
                    };

                    return guard;
                };
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn callable_results_preserve_surviving_partial_cleanup() {
    let compilation = compilation(
        r#"
            module app;

            struct Guard
            {
                destruct()
                {
                    panic("cleanup");
                }
            }

            struct Packet
            {
                first: Guard;
                second: Guard;
            }

            func missing(pos value: Packet) -> Guard
            {
                let result = catch
                {
                    let packet = value;

                    return packet.first;
                };
            }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingCallableResultRequired,
    );
}

#[test]
fn callable_results_refine_cleanup_after_resolved_branches() {
    let compilation = compilation(
        r#"
            module app;

            struct Guard
            {
                destruct()
                {
                    panic("cleanup");
                }
            }

            struct Packet
            {
                guard: Guard;
                flag: bool;
            }

            func literal(pos value: Packet) -> Guard
            {
                let result = catch
                {
                    let packet = value;
                    let guard = if true
                    {
                        yield packet.guard;
                    }
                    else
                    {
                        yield Guard {};
                    };

                    return guard;
                };
            }

            func exhaustive(pos value: Packet, pos condition: bool) -> Guard
            {
                let result = catch
                {
                    let packet = value;
                    let guard = match condition
                    {
                        case true { yield packet.guard; }
                        case false { yield packet.guard; }
                    };

                    return guard;
                };
            }

            func nested(pos value: Packet) -> Result<(Guard, Guard), PanicReport>
            {
                let outer = catch
                {
                    let packet = value;
                    let inner = catch
                    {
                        let first =
                        {
                            let source = Packet
                            {
                                guard = Guard {},
                                flag = false,
                            };
                            let taken = if true
                            {
                                yield source.guard;
                            }
                            else
                            {
                                yield Guard {};
                            };

                            yield taken;
                        };

                        yield (first, packet.guard);
                    };

                    return inner;
                };
            }
        "#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:?}",
        compilation.check_diagnostics()
    );

    for name in ["literal", "exhaustive", "nested"] {
        let key = crate::test_support::source_function_body_key(&compilation, name);
        let lowered = compilation.lowered_unit(key).unwrap();
        let mir = lowered.value().as_ref().unwrap().mir().unwrap();

        assert!(mir.blocks().iter().all(|block| {
            !matches!(block.terminator().kind(), bray_ir::MirTerminatorKind::Return(None))
        }), "{name}");
    }
}

#[test]
fn caught_owned_results_consume_the_success_operand() {
    let compilation = compilation(
        r#"
            module app;

            struct Guard
            {
                destruct() {}
            }

            func invalid(pos value: Guard)
            {
                let result = catch value;
                let reused = value;
            }
        "#,
    );

    assert_goal_state_diagnostic_kind(
        compilation.check_diagnostics(),
        DiagnosticKind::CheckingUseOfMovedStorage,
    );
}
