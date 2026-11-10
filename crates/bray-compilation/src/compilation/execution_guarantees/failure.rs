use bray_checker::{ExecutionObligation, ExecutionVerificationFailure};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticExecutionProofFailure as FailureCause, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, SeverityKind};
use bray_source::SourceSpan;
use bray_symbols::{ExecutionProofFailure, check_execution_proof_dependencies};

use super::imported::{ExecutionProofGraph, ExecutionProofOwner};

pub(super) fn execution_proof_failure(
    graph: &ExecutionProofGraph, owner: ExecutionProofOwner, source: Option<SyntaxAnchor>,
    obligation: ExecutionObligation, mut failure: Option<ExecutionVerificationFailure<Option<SourceSpan>>>,
) -> Option<ExecutionVerificationFailure<Option<SourceSpan>>> {
    let root_key = (owner, obligation);

    if let Some(reason) = check_execution_proof_dependencies(graph).get(&root_key) {
        failure = Some(match reason {
            ExecutionProofFailure::CircularCompletion(owner) => ExecutionVerificationFailure { condition: None,
                source: proof_owner_span(*owner, source), cause: FailureCause::Circular,
            },
            ExecutionProofFailure::MissingCandidate(owner) => failure.unwrap_or(ExecutionVerificationFailure { condition: None,
                source: proof_owner_span(*owner, source), cause: FailureCause::MissingEvidence,
            }),
        });
    }

    if !graph.contains_key(&root_key) {
        failure.get_or_insert(ExecutionVerificationFailure { condition: None, source: proof_owner_span(root_key.0, source), cause: FailureCause::MissingEvidence });
    }

    failure
}

pub(super) fn guarantee_diagnostic(
    obligation: ExecutionObligation, source: SourceSpan, failure: ExecutionVerificationFailure<SourceSpan>,
) -> Diagnostic {
    let postcondition = matches!(obligation, ExecutionObligation::Postcondition(_));
    let kind = match (failure.cause, postcondition) {
        (FailureCause::DepthLimit { .. }, _) => DiagnosticKind::CheckingExecutionProofDepthExceeded,
        (FailureCause::Circular, true) => DiagnosticKind::CheckingCircularPostcondition,
        (FailureCause::Circular, false) => DiagnosticKind::CheckingCircularExecutionGuarantee,
        (_, true) => DiagnosticKind::CheckingPostconditionNotProven,
        (_, false) => DiagnosticKind::CheckingExecutionGuaranteeNotProven,
    };
    let cause = DiagnosticArg::execution_proof_failure(failure.cause);
    let mut diagnostic = Diagnostic::new(DiagnosticId::new(source.start().bytes()), kind, SeverityKind::Error)
        .with_primary_span(source)
        .with_arg(cause.clone())
        .with_label(DiagnosticLabel::primary(DiagnosticLabelKind::ContractNotVerified, source));

    if let Some(condition) = failure.condition {
        let condition = DiagnosticArg::contract_obligation((*condition).clone());
        diagnostic = diagnostic.with_arg(condition.clone()).with_label(
            DiagnosticLabel::secondary(DiagnosticLabelKind::UnprovenContractCondition, failure.source)
                .with_arg(condition).with_arg(cause));
    } else {
        diagnostic = diagnostic.with_label(DiagnosticLabel::secondary(
            DiagnosticLabelKind::ExecutionProofFailure, failure.source).with_arg(cause));
    }

    if kind == DiagnosticKind::CheckingExecutionGuaranteeNotProven {
        diagnostic = diagnostic.with_arg(DiagnosticArg::referenced_name(obligation.property()
            .expect("only a source execution property uses its property diagnostic").as_str()));
    }

    diagnostic
}

pub(super) fn unrepresentable_condition_diagnostic(source: SourceSpan) -> Diagnostic {
    Diagnostic::new(DiagnosticId::new(source.start().bytes()), DiagnosticKind::CheckingContractConditionNotPortable, SeverityKind::Error)
        .with_primary_span(source)
        .with_label(DiagnosticLabel::primary(DiagnosticLabelKind::ContractConditionNotPortable, source))
}

pub(super) fn source_span(anchor: SyntaxAnchor) -> SourceSpan {
    SourceSpan::new(anchor.source_id(), anchor.full_range())
}

pub(super) fn proof_owner_span(owner: ExecutionProofOwner, source: Option<SyntaxAnchor>) -> Option<SourceSpan> {
    match owner {
        ExecutionProofOwner::Source(anchor, _) => Some(source_span(anchor)),
        ExecutionProofOwner::Imported(_) | ExecutionProofOwner::Premise(..) | ExecutionProofOwner::Indirect(_) => source.map(source_span),
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::{ExecutionVerificationFailure, FailureCause, guarantee_diagnostic};
    use bray_checker::ExecutionObligation;
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

    fn span(start: u32) -> SourceSpan {
        SourceSpan::new(SourceId::new(0), TextRange::new(TextSize::new(start), TextSize::new(start + 1)))
    }

    #[test]
    fn postcondition_certification_is_not_an_execution_property_failure() {
        let declaration = span(1);
        let dependency = span(3);
        let diagnostic = guarantee_diagnostic(ExecutionObligation::Postcondition(declaration.into()), declaration, ExecutionVerificationFailure { condition: None, source: dependency, cause: FailureCause::Unproven });

        assert_eq!(diagnostic.kind().as_str(), "checking_postcondition_not_proven");
        assert_eq!(diagnostic.primary_span(), Some(declaration));
        assert!(diagnostic.labels().iter().any(|label| label.span() == dependency));
    }

    #[test]
    fn failed_postconditions_retain_the_required_and_actual_condition() {
        let checked = crate::test_support::compilation(r#"
            module app;

            func select(pos value: u32) -> u32
                ensures(result > 1)
            {
                return value;
            }

            func forward(pos value: u32) -> u32
                ensures(result > 1)
            {
                return select(value);
            }
        "#);
        let diagnostics = checked.check_diagnostics();
        let failures = diagnostics.iter().filter(|diagnostic|
            diagnostic.kind() == bray_diagnostics::DiagnosticKind::CheckingPostconditionNotProven)
            .collect::<Vec<_>>();

        assert!(!failures.is_empty(), "{diagnostics:#?}");
        for failure in failures {
            assert!(failure.args().iter().any(|argument| matches!(argument.value(),
                bray_diagnostics::DiagnosticArgValue::ContractObligation(obligation)
                    if obligation.kind == bray_diagnostics::DiagnosticContractObligationKind::Postcondition
                        && matches!(obligation.required, bray_diagnostics::DiagnosticCondition::Operation { .. })
                        && matches!(obligation.actual, bray_diagnostics::DiagnosticCondition::Operation { .. })
                        && obligation.required != obligation.actual)), "{failure:#?}");
        }
    }

    #[test]
    fn circular_postcondition_certification_retains_its_contract_category() {
        let declaration = span(1);
        let diagnostic = guarantee_diagnostic(ExecutionObligation::Postcondition(declaration.into()), declaration, ExecutionVerificationFailure { condition: None, source: span(3), cause: FailureCause::Circular });

        bray_testing::assert_goal_state_diagnostic_kind(
            &bray_diagnostics::DiagnosticBag::single(diagnostic),
            bray_diagnostics::DiagnosticKind::CheckingCircularPostcondition,
        );
    }
    #[test]
    fn proof_depth_exhaustion_retains_the_operational_cause_and_counts() {
        let declaration = span(1);
        let cause = FailureCause::DepthLimit { depth: 129, limit: 128 };
        let diagnostic = guarantee_diagnostic(
            ExecutionObligation::Property(bray_checker::ExecutionProperty::Total, None), declaration,
            ExecutionVerificationFailure { condition: None, source: span(3), cause },
        );

        assert_eq!(diagnostic.kind().as_str(), "checking_execution_proof_depth_exceeded");
        assert!(diagnostic.args().iter().any(|argument| matches!(argument.value(),
            bray_diagnostics::DiagnosticArgValue::ExecutionProofFailure(actual) if *actual == cause)));

        bray_testing::assert_goal_state_diagnostic_kind(
            &bray_diagnostics::DiagnosticBag::single(diagnostic),
            bray_diagnostics::DiagnosticKind::CheckingExecutionProofDepthExceeded,
        );
    }

    #[test]
    fn invalid_dependencies_keep_their_specific_cause() {
        let diagnostic = guarantee_diagnostic(
            ExecutionObligation::Property(bray_checker::ExecutionProperty::Total, None), span(1),
            ExecutionVerificationFailure { condition: None, source: span(3), cause: FailureCause::DependencyErrors },
        );

        assert!(diagnostic.args().iter().any(|argument| matches!(argument.value(),
            bray_diagnostics::DiagnosticArgValue::ExecutionProofFailure(FailureCause::DependencyErrors))));
    }

    #[test]
    fn unrepresentable_conditions_are_not_reported_as_failed_promises() {
        let diagnostic = super::unrepresentable_condition_diagnostic(span(1));
        bray_testing::assert_goal_state_diagnostic_kind(
            &bray_diagnostics::DiagnosticBag::single(diagnostic),
            bray_diagnostics::DiagnosticKind::CheckingContractConditionNotPortable,
        );
    }

    #[test]
    fn failing_call_preconditions_retain_the_condition_in_total_execution_diagnostics() {
        let checked = crate::test_support::compilation(r#"
            module app;

            func checked(pos value: u32)
                requires(value > 3)
                executes(total)
            {
            }

            func forward(pos value: u32)
                executes(total)
            {
                checked(value);
            }
        "#);
        let diagnostics = checked.check_diagnostics();
        let failure = diagnostics.iter().find(|diagnostic|
            diagnostic.kind() == bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven)
            .unwrap_or_else(|| panic!("{diagnostics:#?}"));

        assert!(failure.args().iter().any(|argument| matches!(argument.value(),
            bray_diagnostics::DiagnosticArgValue::ContractObligation(obligation)
                if obligation.kind == bray_diagnostics::DiagnosticContractObligationKind::CallRequirement
                    && matches!(obligation.required, bray_diagnostics::DiagnosticCondition::Operation { .. })
                    && matches!(obligation.actual, bray_diagnostics::DiagnosticCondition::Operation { .. }))), "{failure:#?}");
    }

    #[test]
    fn execution_failures_identify_the_operation_and_preserve_its_cause() {
        for (body, expected) in [
            (r#"while forever
                    {
                    }"#, FailureCause::UnboundedLoop),
            (r#"while forever
                    {
                        continue;
                    }"#, FailureCause::UnboundedLoop),
            (r#"loop
                    {
                    }"#, FailureCause::UnboundedLoop),
            (r#"panic("failure");"#, FailureCause::Panic),
        ] {
            let source = format!(r#"
                module app;

                func probe(pos forever: bool)
                    executes(total)
                {{
                    {body}
                }}
            "#);
            let application = crate::test_support::compilation(&source);
            let diagnostics = application.check_diagnostics();
            let failure = diagnostics.iter().find(|diagnostic|
                diagnostic.kind() == bray_diagnostics::DiagnosticKind::CheckingExecutionGuaranteeNotProven)
                .unwrap_or_else(|| panic!("{diagnostics:#?}"));
            let cause = failure.args().iter().find_map(|argument| match argument.value() {
                bray_diagnostics::DiagnosticArgValue::ExecutionProofFailure(cause) => Some(*cause),
                _ => None,
            }).expect("execution failure retains its typed cause");
            assert_eq!(cause, expected, "{body}: the operation retains its specific cause");
            let location = failure.labels().iter().find(|label|
                label.kind() == bray_diagnostics::DiagnosticLabelKind::ExecutionProofFailure)
                .expect("failure labels its owning operation").span();
            let input = application.source(location.source_id()).expect("loop source");
            let start = usize::try_from(location.start().bytes()).expect("source offset fits");
            let end = usize::try_from(location.end().bytes()).expect("source offset fits");
            assert_eq!(input.text()[start..end].trim(), body.trim_end_matches(';'));
        }
    }

}
