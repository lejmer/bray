use bray_diagnostics::{DiagnosticArgValue, DiagnosticCondition, DiagnosticContractObligationKind, DiagnosticKind, DiagnosticRelatedLocationKind};

use crate::test_support::compilation;

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
