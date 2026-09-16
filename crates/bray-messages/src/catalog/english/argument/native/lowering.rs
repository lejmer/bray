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
                "could not find the verified lifecycle plan for the highlighted exit",
            );
        }
        Failure::MissingLiteralValue(_) => {
            "generating executable code for the highlighted literal because its value is unavailable"
        }
        Failure::MissingSemanticSelection(_) => {
            "generating executable code for the highlighted expression because its selected callable or built-in behavior is unavailable"
        }
        Failure::UnsupportedExpression(_) => {
            "generating executable code for the highlighted expression"
        }
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
        Failure::MissingOperationResult(_) => {
            "generating the value required by the highlighted expression"
        }
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
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(failure);
        }
        Failure::InvalidFrameDescriptor(problem) => {
            return super::format_internal_compiler_error(format!(
                "could not retain resumable state for the highlighted callable because {}",
                format_frame_descriptor_failure(problem),
            ));
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
        Failure::Mir(failure) => return format_english_mir_unit_failure(failure),
    };

    super::format_internal_compiler_error(format!("could not complete {prevented_operation}"))
}

const fn format_frame_descriptor_failure(
    failure: bray_diagnostics::DiagnosticFrameDescriptorFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticFrameDescriptorFailure as Failure;

    match failure {
        Failure::MissingState => "no resumable state was available",
        Failure::NonContiguousState => "resumable-state positions were not contiguous",
        Failure::DuplicateStateOrEntry => "a resumable state or entry point was duplicated",
        Failure::IdentityCapacityExceeded => "the resumable-state identity limit was exceeded",
    }
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

fn format_english_mir_unit_failure(
    failure: bray_diagnostics::DiagnosticMirUnitBuildFailure,
) -> String {
    use bray_diagnostics::DiagnosticMirUnitBuildFailureKind as Failure;

    let prevented_operation = match failure.kind() {
        Failure::SourceOriginMismatch => {
            "generating executable code associated with the highlighted source declaration"
        }
        Failure::IdentityCapacityExceeded => {
            "generating executable code for the highlighted declaration because an internal capacity was exceeded"
        }
        Failure::ForeignBlock
        | Failure::ForeignOperation
        | Failure::ForeignStorage
        | Failure::ForeignValue => {
            "generating executable code for the highlighted declaration because required analysis belongs to another declaration"
        }
        Failure::MissingBlock | Failure::MissingOperation => {
            "generating all instructions required by the highlighted declaration"
        }
        Failure::MissingOperationResult => {
            "generating a value required by the highlighted declaration"
        }
        Failure::UnexpectedOperationResult => {
            "generating valid executable code for the highlighted declaration because it unexpectedly produces a value"
        }
        Failure::OperationResultTypeMismatch => {
            "generating type-correct executable code for the highlighted declaration"
        }
        Failure::InvalidAggregateOperation => {
            "generating a valid aggregate value for the highlighted declaration"
        }
        Failure::InvalidMemoryOperation => {
            "generating a type-correct read, write, move, or borrow for the highlighted declaration"
        }
        Failure::InvalidAnonymousCallable => {
            "generating executable code for the highlighted anonymous callable"
        }
        Failure::InvalidConstructionInput => {
            "generating a valid construction of the highlighted value"
        }
        Failure::InvalidCall => {
            "generating a type-correct executable call for the highlighted call expression"
        }
        Failure::InvalidHostOperation => {
            "generating program startup or shutdown code for the selected product"
        }
        Failure::InvalidHostSequence => "generating correctly ordered program shutdown code",
        Failure::MissingStorage => "reserving memory required by the highlighted declaration",
        Failure::MissingValue => "generating a value required by the highlighted declaration",
        Failure::DuplicateTerminator => {
            "generating a single outcome for each path through the highlighted declaration"
        }
        Failure::MissingTerminator => {
            "generating an outcome for every path through the highlighted declaration"
        }
        Failure::InvalidInlineAssemblyTerminator => {
            "generating type-correct branches for the highlighted inline assembly"
        }
        Failure::InvalidSuspensionPayload => {
            "preserving the required value while suspending the highlighted asynchronous callable"
        }
        Failure::InvalidCallPanicCheck => {
            "generating valid panic handling for the highlighted call"
        }
        Failure::EdgeArgumentCountMismatch => {
            "passing the required number of values between paths through the highlighted declaration"
        }
        Failure::EdgeArgumentTypeMismatch => {
            "passing type-correct values between paths through the highlighted declaration"
        }
        Failure::DuplicateSwitchCase => "generating distinct cases for the highlighted branch",
        Failure::CleanupTargetMismatch => "generating cleanup code for the highlighted scope",
        Failure::CleanupPhaseOrderViolation => {
            "generating correctly ordered cleanup code for the highlighted scope"
        }
        Failure::RuntimeRoleMismatch => {
            "selecting the required runtime service for the highlighted declaration"
        }
        Failure::RuntimeAbiVersionMismatch => {
            "selecting runtime services from one compatible runtime interface version"
        }
        Failure::InvalidOperationBlock => "placing each instruction on a path where it can run",
        Failure::StorageKindMismatch => {
            "generating ownership-safe code for the highlighted declaration"
        }
        Failure::StorageTypeMismatch => {
            "generating a type-correct value access for the highlighted declaration"
        }
        Failure::ValueDoesNotDominateUse => {
            "generating code that produces each value before the highlighted declaration uses it"
        }
        Failure::ProtectedFrameMismatch => {
            "generating resumable code for the highlighted asynchronous callable"
        }
        Failure::MissingFrameDescriptor => {
            "preserving the state needed to resume the highlighted asynchronous callable"
        }
        Failure::DuplicateFrameDescriptor => {
            "generating one consistent state layout for the highlighted asynchronous callable"
        }
        Failure::UnexpectedFrameDescriptor => {
            "generating valid executable code for the highlighted non-suspending callable"
        }
        Failure::InvalidFrameStateEntry => {
            "generating valid resume points for the highlighted asynchronous callable"
        }
        Failure::MissingFrameState => {
            "generating every required resume point for the highlighted asynchronous callable"
        }
    };

    super::format_internal_compiler_error(format!("could not complete {prevented_operation}"))
}
