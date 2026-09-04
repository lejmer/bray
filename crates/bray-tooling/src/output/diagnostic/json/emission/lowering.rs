use super::context::semantic_value_failure_context;
use super::failure::{DiagnosticEmissionFieldJson, count_u64_field, text_field};

pub(in crate::output::diagnostic::json) fn lowering_input_failure_context(
    failure: bray_diagnostics::DiagnosticLoweringInputFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticLoweringInputFailureKind as Failure;

    match failure.kind() {
        Failure::SemanticValue(failure) => semantic_value_failure_context(failure),
        Failure::ForeignInput {
            input,
            expected_unit,
            actual_unit,
        } => vec![
            text_field("cause", failure.as_str()),
            text_field("input", input),
            count_u64_field("expected_unit", u64::from(expected_unit)),
            count_u64_field("actual_unit", u64::from(actual_unit)),
        ],
        Failure::InputKindMismatch {
            input,
            expected_kind,
            actual_kind,
        } => vec![
            text_field("cause", failure.as_str()),
            text_field("input", input),
            text_field("expected_unit_kind", expected_kind),
            text_field("actual_unit_kind", actual_kind),
        ],
        Failure::MissingSemanticSelection(identity)
        | Failure::MissingExpressionType(identity)
        | Failure::InvalidStorageOperation(identity) => {
            lowering_identity_context(failure.as_str(), "expression", identity)
        }
        Failure::InvalidStorageExit(identity) => {
            lowering_identity_context(failure.as_str(), "block", identity)
        }
        Failure::InvalidPlan {
            plan,
            cause,
            expression,
            scope,
            exit,
            storage,
            access,
        } => {
            let mut context = vec![
                text_field("cause", failure.as_str()),
                text_field("plan", plan),
                text_field("plan_failure", cause),
            ];

            let identities = [
                ("expression", expression),
                ("scope", scope),
                ("exit", exit),
                ("storage", storage),
                ("storage_access", access),
            ];

            if let Some(identity) = identities.iter().find_map(|(_, identity)| *identity) {
                context.push(count_u64_field("bound_unit", u64::from(identity.unit())));
            }

            for (name, identity) in identities {
                if let Some(identity) = identity {
                    context.push(count_u64_field(name, u64::from(identity.ordinal())));
                }
            }

            context
        }
        Failure::InvalidInputContents(input) => vec![
            text_field("cause", failure.as_str()),
            text_field("input", input),
        ],
        Failure::StorageOperationCountMismatch { expected, actual } => vec![
            text_field("cause", failure.as_str()),
            count_u64_field("expected", expected),
            count_u64_field("actual", actual),
        ],
        Failure::LiteralTargetWidthMismatch { expected, actual } => vec![
            text_field("cause", failure.as_str()),
            count_u64_field("expected", u64::from(expected)),
            count_u64_field("actual", u64::from(actual)),
        ],
        Failure::InvalidPatternInput
        | Failure::ExecutableHostRequiresSyntheticInput
        | Failure::CompileTimeUnitRequiresClassification => {
            vec![text_field("cause", failure.as_str())]
        }
    }
}

pub(in crate::output::diagnostic::json) fn lowering_failure_context(
    failure: bray_diagnostics::DiagnosticLoweringFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticLoweringFailureKind as Failure;

    match failure.kind() {
        Failure::SemanticValue(failure) => semantic_value_failure_context(failure),
        Failure::GenericSubstitution(failure) => {
            let mut context = vec![text_field(
                "cause",
                "code_production_generic_substitution_invalid",
            )];

            super::checker::push_generic_substitution_failure(&mut context, failure);

            context
        }
        Failure::Mir(failure) => mir_unit_failure_context(failure),
        Failure::UnsupportedRoot(root) => lowering_root_context(failure.as_str(), root),
        Failure::MissingSourceNode { kind, identity }
        | Failure::RecoveredSourceNode { kind, identity } => {
            let mut context = lowering_identity_context(failure.as_str(), "node", identity);
            context.push(text_field("node_kind", source_construct_kind(kind)));

            context
        }
        Failure::MissingExpressionType(identity)
        | Failure::AwaitOutsideProtectedFrame(identity)
        | Failure::MissingSuspensionPoint(identity)
        | Failure::InvalidTaskOperation(identity)
        | Failure::MissingLiteralValue(identity)
        | Failure::MissingSemanticSelection(identity)
        | Failure::UnsupportedExpression(identity)
        | Failure::MissingStorageAccess(identity)
        | Failure::MissingIterationStorage(identity)
        | Failure::MissingOperationResult(identity) => {
            lowering_identity_context(failure.as_str(), "expression", identity)
        }
        Failure::UnsupportedPattern(identity) => {
            lowering_identity_context(failure.as_str(), "pattern", identity)
        }
        Failure::UnsupportedOperator {
            expression,
            operator,
        } => {
            let mut context = lowering_identity_context(failure.as_str(), "expression", expression);
            context.push(text_field("operator", operator));

            context
        }
        Failure::MissingStorageAccessRecord(identity)
        | Failure::MissingStorageIdentity(identity)
        | Failure::UnsupportedStorageAccess(identity) => {
            lowering_identity_context(failure.as_str(), "storage_access", identity)
        }
        Failure::MissingStorageIdentityRecord(identity) => {
            lowering_identity_context(failure.as_str(), "storage_identity", identity)
        }
        Failure::MissingRepresentation(role) => vec![
            text_field("cause", failure.as_str()),
            text_field("representation_role", role),
        ],
        Failure::InvalidFrameDescriptor(problem) => vec![
            text_field("cause", failure.as_str()),
            text_field(
                "frame_descriptor_problem",
                frame_descriptor_failure(problem),
            ),
        ],
        Failure::MemoryArgumentOrdinalUnrepresentable {
            expression,
            ordinal,
        } => {
            let mut context = lowering_identity_context(failure.as_str(), "expression", expression);

            context.push(count_u64_field("memory_argument_ordinal", ordinal));

            context
        }
        Failure::MatchArmOrdinalUnrepresentable {
            expression,
            ordinal,
        } => {
            let mut context = lowering_identity_context(failure.as_str(), "expression", expression);

            context.push(text_field("match_arm_ordinal", ordinal.to_string()));

            context
        }
        Failure::MissingCallableResultType | Failure::SemanticValueUnavailable => {
            vec![text_field("cause", failure.as_str())]
        }
    }
}

const fn frame_descriptor_failure(
    failure: bray_diagnostics::DiagnosticFrameDescriptorFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticFrameDescriptorFailure as Failure;

    match failure {
        Failure::MissingState => "missing_state",
        Failure::NonContiguousState => "non_contiguous_state",
        Failure::DuplicateStateOrEntry => "duplicate_state_or_entry",
        Failure::IdentityCapacityExceeded => "identity_capacity_exceeded",
    }
}

fn lowering_identity_context(
    cause: &'static str,
    name: &'static str,
    identity: bray_diagnostics::DiagnosticLoweringIdentity,
) -> Vec<DiagnosticEmissionFieldJson> {
    vec![
        text_field("cause", cause),
        count_u64_field("bound_unit", u64::from(identity.unit())),
        count_u64_field(name, u64::from(identity.ordinal())),
    ]
}

fn lowering_root_context(
    cause: &'static str,
    root: bray_diagnostics::DiagnosticLoweringRoot,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticLoweringRoot as Root;

    let mut context = vec![text_field("cause", cause)];

    match root {
        Root::CallableBody { execution, body } => {
            context.push(text_field("root_kind", "callable_body"));
            context.push(text_field("execution", execution));
            push_lowering_identity(&mut context, "callable_body", body);
        }
        Root::AnonymousCallable {
            callable_region,
            callable_ordinal,
            execution,
            body,
        } => {
            context.push(text_field("root_kind", "anonymous_callable"));

            context.push(count_u64_field(
                "callable_region",
                u64::from(callable_region),
            ));

            context.push(count_u64_field(
                "anonymous_callable",
                u64::from(callable_ordinal),
            ));

            context.push(text_field("execution", execution));
            push_lowering_identity(&mut context, "callable_body", body);
        }
        Root::Expression(identity) => {
            context.push(text_field("root_kind", "expression"));
            push_lowering_identity(&mut context, "expression", identity);
        }
        Root::ExpressionSequence(identity) => {
            context.push(text_field("root_kind", "expression_sequence"));
            push_lowering_identity(&mut context, "block", identity);
        }
    }

    context
}

fn push_lowering_identity(
    context: &mut Vec<DiagnosticEmissionFieldJson>,
    name: &'static str,
    identity: bray_diagnostics::DiagnosticLoweringIdentity,
) {
    context.push(count_u64_field("bound_unit", u64::from(identity.unit())));
    context.push(count_u64_field(name, u64::from(identity.ordinal())));
}

const fn source_construct_kind(
    kind: bray_diagnostics::DiagnosticSourceConstructKind,
) -> &'static str {
    match kind {
        bray_diagnostics::DiagnosticSourceConstructKind::Expression => "expression",
        bray_diagnostics::DiagnosticSourceConstructKind::Pattern => "pattern",
        bray_diagnostics::DiagnosticSourceConstructKind::Block => "block",
        bray_diagnostics::DiagnosticSourceConstructKind::CallableBody => "callable_body",
    }
}

fn mir_unit_failure_context(
    failure: bray_diagnostics::DiagnosticMirUnitBuildFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticMirUnitBuildFailureContext as Context;

    let mut context = vec![text_field("cause", failure.as_str())];

    match failure.context() {
        Context::None => {}
        Context::UnitMismatch { expected, actual } => {
            context.push(count_u64_field("expected_unit", u64::from(expected)));
            context.push(count_u64_field("actual_unit", u64::from(actual)));
        }
        Context::Block(identity) => push_mir_identity(&mut context, "block", identity),
        Context::Operation(identity) => push_mir_identity(&mut context, "operation", identity),
        Context::Storage(identity) => push_mir_identity(&mut context, "storage", identity),
        Context::Value(identity) => push_mir_identity(&mut context, "value", identity),
        Context::CleanupTarget { phase, target } => {
            context.push(text_field("cleanup_phase", phase));
            push_mir_identity(&mut context, "target_block", target);
        }
        Context::RuntimeRoleMismatch { expected, actual } => {
            context.push(text_field("expected_runtime_role", expected));
            context.push(text_field("actual_runtime_role", actual));
        }
    }

    context
}

fn push_mir_identity(
    context: &mut Vec<DiagnosticEmissionFieldJson>,
    name: &'static str,
    identity: bray_diagnostics::DiagnosticMirUnitLocalIdentity,
) {
    context.push(count_u64_field("mir_unit", u64::from(identity.unit())));
    context.push(count_u64_field(name, u64::from(identity.slot())));
}
