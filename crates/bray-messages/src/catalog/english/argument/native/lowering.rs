use super::artifact::format_english_semantic_value_failure;

#[cfg(test)]
mod tests {
    #[test]
    fn initialized_input_cleanup_failure_identifies_the_expression() {
        use bray_diagnostics::{
            DiagnosticLoweringFailure, DiagnosticLoweringFailureKind, DiagnosticLoweringIdentity,
        };

        use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(3), TextSize::new(8)),
        );

        let failure = DiagnosticLoweringFailure::new(
            DiagnosticLoweringFailureKind::MissingInputCleanup(DiagnosticLoweringIdentity::new(
                13, 57,
            )),
            source,
        );

        let rendered = super::format_english_lowering_failure(failure);

        assert!(rendered.contains("internal compiler error"));
        assert!(rendered.contains("highlighted expression"));
        assert!(rendered.contains("cleanup for an initialized input"));
        assert!(crate::catalog::english::forbidden_internal_term(&rendered).is_none());
        assert!(!rendered.contains("13"));
        assert!(!rendered.contains("57"));
        assert_eq!(failure.source(), source);
    }
}

pub(super) fn format_english_lowering_failure(
    failure: bray_diagnostics::DiagnosticLoweringFailure,
) -> String {
    use bray_diagnostics::DiagnosticLoweringFailureKind as Failure;

    let prevented_operation = match failure.kind() {
        Failure::UnsupportedRoot(_) => {
            "generating executable code for the highlighted declaration because it has no executable body"
        }
        Failure::MissingSourceNode { kind, .. } => {
            return super::format_internal_compiler_error(format!(
                "could not generate executable code for the highlighted {}",
                format_source_construct(kind),
            ));
        }
        Failure::RecoveredSourceNode { kind, .. } => {
            return super::format_internal_compiler_error(format!(
                "could not generate executable code for the highlighted {} after an earlier error",
                format_source_construct(kind),
            ));
        }
        Failure::MissingExpressionType(_) => {
            "generating executable code for the highlighted expression because the type established for it is unavailable"
        }
        Failure::AwaitOutsideProtectedFrame(_) => {
            "generating resumable code for the highlighted `await` expression"
        }
        Failure::MissingSuspensionPoint(_) => {
            "generating a valid resume path for the highlighted `await` expression"
        }
        Failure::InvalidTaskOperation(_) => {
            "generating a type-correct call for the highlighted task operation"
        }
        Failure::MissingInputCleanup(_) => {
            "generating executable code for the highlighted expression because cleanup for an initialized input was not determined"
        }
        Failure::MissingCallableResultType => {
            "generating executable code for the highlighted callable because its result type is unavailable"
        }
        Failure::InvalidCleanupScopeDepth {
            scope_depth,
            active_scope_count,
            ..
        } => {
            return super::format_internal_compiler_error(format!(
                "could not clean lexical scope position {scope_depth} because only {active_scope_count} scopes were active",
            ));
        }
        Failure::MissingScopeExitPlan { .. } => {
            return super::format_internal_compiler_error(
                "could not find the checked async cleanup plan for the highlighted exit",
            );
        }
        Failure::MissingLiteralValue(_) => {
            "generating executable code for the highlighted literal because its value is unavailable"
        }
        Failure::MissingSemanticSelection(_) => {
            "generating executable code for the highlighted expression because its selected callable or built-in behavior is unavailable"
        }
        Failure::UnsupportedExpression(_) => "generating executable code for the highlighted expression",
        Failure::UnsupportedPattern(_) => "generating executable code for the highlighted pattern",
        Failure::UnsupportedOperator { .. } => {
            "generating executable code for the highlighted operator"
        }
        Failure::MissingStorageAccess(_) => {
            "generating ownership-safe code for the highlighted expression because its value-access behavior is unavailable"
        }
        Failure::MissingStorageAccessRecord(_) => {
            "generating ownership-safe code for the highlighted expression because its read, borrow, move, or write behavior is unavailable"
        }
        Failure::MissingStorageIdentity(_) => {
            "generating ownership-safe code for the highlighted expression because the value it accesses is unavailable"
        }
        Failure::MissingStorageIdentityRecord(_) => {
            "generating ownership-safe code for the highlighted declaration because one of its associated values is unavailable"
        }
        Failure::MissingIterationStorage(_) => {
            "generating executable code for the highlighted iteration because its cursor or current value is unavailable"
        }
        Failure::UnsupportedStorageAccess(_) => {
            "generating ownership-safe code for the highlighted value access"
        }
        Failure::MissingOperationResult(_) => "generating the value required by the highlighted expression",
        Failure::MissingRepresentation(_) => {
            "generating target-correct code for the highlighted declaration because its target representation is unavailable"
        }
        Failure::SemanticValueUnavailable => {
            "generating executable code for the highlighted declaration because a required type or constant value is unavailable"
        }
        Failure::GenericSubstitution(failure) => {
            return super::format_internal_compiler_error(
                super::checker::format_generic_substitution_failure(failure),
            );
        }
        Failure::SemanticValue(failure) => return format_english_semantic_value_failure(failure),
        Failure::MirCapacity => {
            return super::format_internal_compiler_error(
                "could not generate executable code because a MIR identity capacity was exceeded",
            );
        }
        Failure::MemoryArgumentOrdinalUnrepresentable { ordinal, .. } => {
            return super::format_internal_compiler_error(format!(
                "could not represent memory-operation argument position {ordinal}",
            ));
        }
        Failure::MatchArmOrdinalUnrepresentable { ordinal, .. } => {
            return super::format_internal_compiler_error(format!(
                "could not represent match-arm position {ordinal}",
            ));
        }
    };

    super::format_internal_compiler_error(format!("could not complete {prevented_operation}"))
}

const fn format_source_construct(
    kind: bray_diagnostics::DiagnosticSourceConstructKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticSourceConstructKind as Kind;

    match kind {
        Kind::Expression => "expression",
        Kind::Pattern => "pattern",
        Kind::Block => "block",
        Kind::CallableBody => "callable body",
    }
}
