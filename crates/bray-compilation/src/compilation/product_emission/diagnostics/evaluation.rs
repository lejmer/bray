use bray_diagnostics::{
    DiagnosticBag, DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::common::emission_failure_diagnostics;
use crate::fact::FactQueryError;

pub(super) fn query_failure_diagnostics(
    error: &FactQueryError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    if matches!(error, FactQueryError::Cancelled) {
        return DiagnosticBag::new();
    }

    emission_failure_diagnostics(
        DiagnosticEmissionFailure::Evaluation(diagnostic_evaluation_failure(error)),
        product,
        target,
    )
}

pub(crate) fn diagnostic_evaluation_failure(
    error: &FactQueryError,
) -> DiagnosticEmissionEvaluationFailure {
    match error {
        FactQueryError::Cancelled => DiagnosticEmissionEvaluationFailure::Cancelled,
        FactQueryError::Cycle(cycle) => DiagnosticEmissionEvaluationFailure::Cycle(
            crate::fact::diagnostic_cycle_failure(cycle),
        ),
        FactQueryError::InfrastructureFailure => DiagnosticEmissionEvaluationFailure::Infrastructure,
        FactQueryError::Runtime(error) => DiagnosticEmissionEvaluationFailure::Runtime(
            crate::fact::diagnostic_fact_runtime_failure(error),
        ),
        FactQueryError::SemanticValueStoreCreate(_) => {
            DiagnosticEmissionEvaluationFailure::SemanticValueStoreCreate
        }
        FactQueryError::SemanticValueStore(error) => {
            DiagnosticEmissionEvaluationFailure::SemanticValue(
                crate::fact::diagnostic_semantic_value_failure(*error),
            )
        }
        FactQueryError::BindingDependencyUnavailable => {
            DiagnosticEmissionEvaluationFailure::Binding(
                bray_diagnostics::DiagnosticBindingFailure::DependencyUnavailable,
            )
        }
        FactQueryError::Binding(error) => DiagnosticEmissionEvaluationFailure::Binding(
            crate::fact::diagnostic_binding_failure(error),
        ),
        FactQueryError::LoweringInput(error) => DiagnosticEmissionEvaluationFailure::LoweringInput(
            super::super::super::lowering_diagnostic::lowering_input_failure(error),
        ),
        FactQueryError::Lowering(error) => DiagnosticEmissionEvaluationFailure::Lowering(
            super::super::super::lowering_diagnostic::lowering_failure(error),
        ),
        FactQueryError::ConstantCallableBodyUnavailable => {
            DiagnosticEmissionEvaluationFailure::ConstantCallableBodyUnavailable
        }
        FactQueryError::ConstantCallableRootUnavailable => {
            DiagnosticEmissionEvaluationFailure::ConstantCallableRootUnavailable
        }
        FactQueryError::AtomicInitializerArgumentUnavailable => {
            DiagnosticEmissionEvaluationFailure::AtomicInitializerArgumentUnavailable
        }
        FactQueryError::AtomicInitializerResultUnavailable => {
            DiagnosticEmissionEvaluationFailure::AtomicInitializerResultUnavailable
        }
        FactQueryError::UninitInitializerResultUnavailable => {
            DiagnosticEmissionEvaluationFailure::UninitInitializerResultUnavailable
        }
        FactQueryError::ImportedExecutableTemplateMismatch => {
            DiagnosticEmissionEvaluationFailure::ImportedExecutableTemplateMismatch
        }
        FactQueryError::SemanticUnitContext(error) => {
            DiagnosticEmissionEvaluationFailure::SemanticContext(
                crate::fact::diagnostic_semantic_context_failure(error),
            )
        }
        FactQueryError::SemanticQuery(error) => DiagnosticEmissionEvaluationFailure::SemanticQuery(
            crate::fact::diagnostic_semantic_query_failure(error),
        ),
        FactQueryError::Product(error) => DiagnosticEmissionEvaluationFailure::Product(
            super::product_query::diagnostic_product_query_failure(error),
        ),
        FactQueryError::Foreign(error) => DiagnosticEmissionEvaluationFailure::Foreign(
            super::foreign_query::diagnostic_foreign_query_failure(error),
        ),
        FactQueryError::CheckerInfrastructure(error) => match error {
            bray_checker::CheckerInfrastructureError::AtomicRepresentationTypeUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicRepresentationTypeUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicRepresentationArgumentsUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicRepresentationArgumentsUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicInitializerArgumentUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicInitializerArgumentUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicInitializerResultUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicInitializerResultUnavailable
            }
            bray_checker::CheckerInfrastructureError::UninitInitializerResultUnavailable => {
                DiagnosticEmissionEvaluationFailure::UninitInitializerResultUnavailable
            }
            bray_checker::CheckerInfrastructureError::ImportedExecutableTemplateMismatch => {
                DiagnosticEmissionEvaluationFailure::ImportedExecutableTemplateMismatch
            }
            error => DiagnosticEmissionEvaluationFailure::Checker(
                crate::fact::diagnostic_checker_failure(*error),
            ),
        },
    }
}
