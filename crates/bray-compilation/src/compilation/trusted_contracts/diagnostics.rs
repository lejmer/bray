use bray_diagnostics::{DiagnosticArgValue, DiagnosticCondition, DiagnosticContractObligationKind, DiagnosticKind, DiagnosticRelatedLocationKind};

use crate::test_support::compilation;

#[test]
fn failed_conjunction_names_the_missing_condition_and_keeps_the_full_requirement() {
    let application = compilation(r#"
        trusted module app;

        struct Owner
        {
        }

        trusted predicate first(owner: &Owner);
        trusted predicate second(owner: &Owner);

        trusted func supply(pos owner: &Owner)
            ensures(trusted first(owner = owner))
            executes(pure, total)
        {
        }

        func inspect(pos owner: &Owner)
            requires(trusted first(owner = owner) && trusted second(owner = owner))
        {
        }

        func caller(pos owner: &Owner)
        {
            trusted supply(owner);
            inspect(owner);
        }
    "#);
    let diagnostics = application.check_diagnostics();
    bray_testing::assert_goal_state_diagnostic_kind(diagnostics,
        DiagnosticKind::CheckingTrustedObligationNotProven);
    let obligation = diagnostics.iter().flat_map(|diagnostic| diagnostic.args()).find_map(|argument| match argument.value() {
        DiagnosticArgValue::ContractObligation(obligation) => Some(obligation),
        _ => None,
    }).unwrap_or_else(|| panic!("{diagnostics:#?}"));

    assert!(matches!(&obligation.actual, DiagnosticCondition::Application { path, .. }
        if path.last().is_some_and(|name| name == "second")), "{obligation:#?}");
    assert!(matches!(&obligation.required, DiagnosticCondition::Operation { .. }), "{obligation:#?}");
}

#[test]
fn inline_callback_conditions_show_a_snippet_without_the_function_body() {
    let application = compilation(r#"
        trusted module app;

        callable Callback = func() -> u32;

        trusted predicate acceptable(value: &Callback);

        func inspect(pos value: Callback)
            requires(trusted acceptable(value = &value))
        {
        }

        func caller()
        {
            inspect(lambda() -> u32
            {
                return 12345;
            });
        }
    "#);
    let diagnostics = application.check_diagnostics();
    bray_testing::assert_goal_state_diagnostic_kind(diagnostics,
        DiagnosticKind::CheckingTrustedObligationNotProven);
    let failure = diagnostics.iter().find(|diagnostic| diagnostic.kind()
        == DiagnosticKind::CheckingTrustedObligationNotProven).expect("callback requirement fails");
    let rendered = bray_messages::DiagnosticRenderer::english().render(failure);
    assert!(rendered.message().contains("lambda() -> u32…"), "{}", rendered.message());
    assert!(!rendered.message().contains('\n') && !rendered.message().contains("12345"));
}

#[test]
fn conflicting_borrows_retain_callable_signatures() {
    let application = compilation(r#"
        module app;

        callable Callback = trusted async func(pos value: &mut usize, enabled: bool) -> u32;

        func inspect(pos callback: &mut Callback)
        {
        }

        func caller(pos mut callback: Callback)
        {
            let child = &mut callback;
            inspect(&mut callback);
            _ = child;
        }
    "#);
    let diagnostics = application.check_diagnostics();

    let signature = diagnostics.iter()
        .filter(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingConflictingBorrow)
        .flat_map(|diagnostic| diagnostic.args())
        .find_map(|argument| {
            let DiagnosticArgValue::StorageAccess(access) = argument.value() else { return None; };
            let bray_diagnostics::DiagnosticType::Borrow { target, .. } = access.reached_type() else { return None; };
            matches!(target.as_ref(), bray_diagnostics::DiagnosticType::Callable { .. }).then_some(target.as_ref())
        }).unwrap_or_else(|| panic!("the conflict must retain its callback signature: {diagnostics:#?}"));
    let bray_diagnostics::DiagnosticType::Callable { parameters, result, execution, trust, abi, .. } = signature else { unreachable!() };

    assert_eq!(*execution, bray_diagnostics::DiagnosticCallableExecution::Asynchronous);
    assert_eq!(*trust, bray_diagnostics::DiagnosticCallableTrust::Trusted);
    assert_eq!(*abi, bray_diagnostics::DiagnosticCallableAbi::Bray);
    assert_eq!(result.as_ref(), &bray_diagnostics::DiagnosticType::U32);
    assert_eq!(parameters.len(), 2);
    assert_eq!(parameters[0].name, "value");
    assert_eq!(parameters[0].position, bray_diagnostics::DiagnosticCallablePosition::PositionalOrNamed);
    assert!(matches!(&parameters[0].ty, bray_diagnostics::DiagnosticType::Borrow {
        kind: bray_diagnostics::DiagnosticBorrowKind::Mutable, target,
    } if target.as_ref() == &bray_diagnostics::DiagnosticType::Usize));
    assert_eq!(parameters[1].name, "enabled");
    assert_eq!(parameters[1].position, bray_diagnostics::DiagnosticCallablePosition::NamedOnly);
    assert_eq!(parameters[1].ty, bray_diagnostics::DiagnosticType::Boolean);
}

#[test]
fn conflicting_borrows_retain_the_concrete_target_and_capability() {
    let application = compilation(r#"
        module app;

        struct Slot
        {
            mut value: usize;
        }

        func inspect(pos owner: &mut Slot)
        {
        }

        func caller(pos owner: &mut Slot)
        {
            let child = &mut owner.value;
            inspect(owner);
            _ = child;
        }
    "#);
    let diagnostics = application.check_diagnostics();

    assert!(diagnostics.iter().filter(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingConflictingBorrow)
        .flat_map(|diagnostic| diagnostic.args()).any(|argument| {
            let DiagnosticArgValue::StorageAccess(access) = argument.value() else { return false; };
            let bray_diagnostics::DiagnosticType::Borrow {
                kind: bray_diagnostics::DiagnosticBorrowKind::Mutable, target,
            } = access.reached_type() else { return false; };
            matches!(target.as_ref(), bray_diagnostics::DiagnosticType::Named(named)
                if named.path().last().map(String::as_str) == Some("Slot"))
        }), "the conflict must retain &mut Slot: {diagnostics:#?}");
}

#[test]
fn failed_conditions_name_the_complete_producer_call() {
    for (producer, input_count) in [("acquire()", 0), ("factory.acquire()", 1)] {
    let source = r#"
        trusted module app;

        struct Owner
        {
        }

        struct Factory
        {
            func acquire() -> Owner
            {
                return
                {
                };
            }
        }

        trusted predicate live(owner: &Owner);

        extern func acquire() -> Owner;

        func inspect(pos owner: Owner)
            requires(trusted live(owner = &owner))
        {
        }

        func caller(pos factory: &Factory)
        {
            inspect(PRODUCER);
        }
    "#.replace("PRODUCER", producer);
    let application = compilation(&source);
    let diagnostics = application.check_diagnostics();
    let obligation = diagnostics.iter().flat_map(|diagnostic| diagnostic.args()).find_map(|argument| match argument.value() {
        DiagnosticArgValue::ContractObligation(obligation) => Some(obligation),
        _ => None,
    }).unwrap_or_else(|| panic!("{diagnostics:#?}"));
    let DiagnosticCondition::Application { arguments, .. } = &obligation.actual else {
        panic!("{obligation:#?}");
    };

    let DiagnosticCondition::Application { path, arguments: inputs, .. } = &arguments[0] else {
        panic!("the producer must retain its checked callable and inputs: {obligation:#?}");
    };

    assert_eq!(path.last().map(String::as_str), Some("acquire"));
    assert_eq!(inputs.len(), input_count);
    if input_count != 0 {
        assert_eq!(inputs[0], DiagnosticCondition::Source("factory".to_owned()));
    }
    }
}

#[test]
fn failed_conditions_distinguish_staged_owned_arguments() {
    for abi in ["", "@abi(c)"] {
    let source = r#"
        trusted module app;

        using core.memory;

        trusted predicate compatible(left: &DataPointer<u8>, right: &DataPointer<u8>);

        ABI
        trusted func inspect(pos mut left: DataPointer<u8>, pos right: DataPointer<u8>)
            requires(trusted compatible(left = &left, right = &right))
        {
        }

        func caller(pos first: DataPointer<u8>, pos second: DataPointer<u8>)
        {
            trusted inspect(first, second);
        }
    "#.replace("ABI", abi);
    let application = compilation(&source);
    let diagnostics = application.check_diagnostics();
    let obligation = diagnostics.iter().flat_map(|diagnostic| diagnostic.args()).find_map(|argument| match argument.value() {
        DiagnosticArgValue::ContractObligation(obligation) => Some(obligation),
        _ => None,
    }).unwrap_or_else(|| panic!("{diagnostics:#?}"));
    let DiagnosticCondition::Application { arguments, .. } = &obligation.actual else {
        panic!("{obligation:#?}");
    };
    assert_eq!(arguments, &[
        DiagnosticCondition::Source("first".to_owned()),
        DiagnosticCondition::Source("second".to_owned()),
    ]);
    }
}

#[test]
fn trusted_obligation_diagnostics_retain_the_condition_and_its_role() {
    for (role, cleanup, body) in [
        (
            DiagnosticContractObligationKind::CallRequirement,
            "",
            r#"
                func caller(pos actual_owner: Owner)
                {
                    inspect_owner(&actual_owner, 42);
                }
            "#,
        ),
        (
            DiagnosticContractObligationKind::Postcondition,
            "",
            r#"
                func caller(pos owner: Owner) -> Owner
                    ensures(trusted live(owner = &result, count = 42))
                {
                    return owner;
                }
            "#,
        ),
        (
            DiagnosticContractObligationKind::CleanupRequirement,
            r#"
                trusted destruct()
                    requires(trusted live(owner = &self, count = 42))
                {
                }
            "#,
            r#"
                func caller()
                {
                    let owner: Owner =
                    {
                        value = 42
                    };
                }
            "#,
        ),
    ] {
        let source = format!(r#"
            trusted module app;

            struct Owner
            {{
                value: usize;
                {cleanup}
            }}

            trusted predicate live(owner: &Owner, count: usize);

            func inspect_owner(pos owner: &Owner, pos count: usize)
                requires(trusted live(owner = owner, count = count))
            {{
            }}

            {body}
        "#);
        let application = compilation(&source);
        let diagnostics = application.check_diagnostics();
        let failure = diagnostics.iter()
            .find(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingTrustedObligationNotProven)
            .unwrap_or_else(|| panic!("{role:?}: {diagnostics:#?}"));
        let obligation = failure.args().iter()
            .find(|argument| argument.name().as_str() == "contract_obligation")
            .unwrap_or_else(|| panic!("{role:?} must retain its failed condition: {failure:#?}"));
        let DiagnosticArgValue::ContractObligation(obligation) = obligation.value() else {
            panic!("the proof failure must use its typed condition model");
        };
        assert_eq!(obligation.kind, role);

        let DiagnosticCondition::Application { path, arguments, .. } = &obligation.actual else {
            panic!("the instantiated predicate must survive: {obligation:#?}");
        };
        assert_eq!(path.last().map(String::as_str), Some("live"));
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[1], DiagnosticCondition::Literal("42".to_owned()));

        match role {
            DiagnosticContractObligationKind::CallRequirement => {
                assert_eq!(arguments[0], DiagnosticCondition::Source("actual_owner".to_owned()));
            }
            DiagnosticContractObligationKind::Postcondition => {
                assert_eq!(arguments[0], DiagnosticCondition::Source("owner".to_owned()));
            }
            DiagnosticContractObligationKind::CleanupRequirement => {
                let DiagnosticCondition::Source(subject) = &arguments[0] else {
                    panic!("cleanup must identify the produced owner: {obligation:#?}");
                };
                assert!(subject.contains("value = 42"), "{subject}");
            }
            DiagnosticContractObligationKind::ExecutionRequirement => unreachable!("this fixture checks value contracts"),
        }

        if role != DiagnosticContractObligationKind::Postcondition {
            assert!(failure.related_locations().iter().any(|location|
                location.kind() == DiagnosticRelatedLocationKind::RequirementOrigin),
                "the owning callable declaration must survive: {failure:#?}");
        }
    }
}

#[test]
fn invalidated_trusted_conditions_identify_the_argument_change() {
    let source = r#"
        trusted module app;

        struct Owner
        {
            mut value: usize;
        }

        trusted predicate live(owner: &Owner);

        trusted func acquire() -> Owner
            ensures(trusted live(owner = &result))
        {
            return
            {
                value = 42
            };
        }

        extern func opaque(pos owner: &mut Owner);

        func inspect_owner(pos owner: &Owner)
            requires(trusted live(owner = owner))
        {
        }

        func caller()
        {
            let mut owner = trusted acquire();
            opaque(&mut owner);
            inspect_owner(&owner);
        }
    "#;
    let application = compilation(source);
    let diagnostics = application.check_diagnostics();
    let failure = diagnostics.iter().find(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingTrustedObligationNotProven)
        .unwrap_or_else(|| panic!("{diagnostics:#?}"));
    let change = failure.related_locations().iter().find(|location| location.kind().as_str() == "value_changed")
        .unwrap_or_else(|| panic!("the known argument change must survive: {failure:#?}"));
    let operation = source.find("opaque(&mut owner)").expect("fixture has its modifying call");
    let range = change.span();

    assert!(usize::try_from(range.start().bytes()).expect("source offset fits host") >= operation);
    assert!(usize::try_from(range.end().bytes()).expect("source offset fits host") <= operation + "opaque(&mut owner)".len());
}

#[test]
fn failed_call_diagnostics_preserve_arguments_before_later_writes() {
    let application = compilation(r#"
        trusted module app;

        struct Owner
        {
        }

        trusted predicate live(owner: &Owner, count: usize);

        func inspect_owner(pos owner: &Owner, pos count: usize)
            requires(trusted live(owner = owner, count = count))
        {
        }

        func caller(pos owner: Owner)
        {
            let mut count: usize = 42;
            inspect_owner(&owner, count);
            count = 99;
        }
    "#);
    let diagnostics = application.check_diagnostics();
    let obligation = diagnostics.iter().flat_map(|diagnostic| diagnostic.args()).find_map(|argument| match argument.value() {
        DiagnosticArgValue::ContractObligation(obligation) => Some(obligation),
        _ => None,
    }).unwrap_or_else(|| panic!("{diagnostics:#?}"));
    let DiagnosticCondition::Application { arguments, .. } = &obligation.actual else {
        panic!("{obligation:#?}");
    };
    assert_eq!(arguments[1], DiagnosticCondition::Literal("42".to_owned()));

    let application = compilation(r#"
        trusted module app;

        struct Owner
        {
        }

        trusted predicate live(owner: &Owner);

        async func identity(pos owner: Owner) -> Owner
        {
            return owner;
        }

        func inspect_owner(pos owner: &Owner)
            requires(trusted live(owner = owner))
        {
        }

        async func caller(pos source: Owner)
        {
            let returned = await identity(source);

            inspect_owner(&returned);
        }
    "#);
    let diagnostics = application.check_diagnostics();
    let obligation = diagnostics.iter().flat_map(|diagnostic| diagnostic.args()).find_map(|argument| match argument.value() {
        DiagnosticArgValue::ContractObligation(obligation) => Some(obligation),
        _ => None,
    }).unwrap_or_else(|| panic!("{diagnostics:#?}"));
    let DiagnosticCondition::Application { arguments, .. } = &obligation.actual else {
        panic!("{obligation:#?}");
    };
    assert_eq!(arguments, &[DiagnosticCondition::Source("source".to_owned())]);
}
