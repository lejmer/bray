use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundReferenceTarget, CheckedLiteralValues,
    CheckedMemoryOperation, CheckedMemoryOperations, CheckedSemanticSelections, SelectedArgument,
    SemanticSelection,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticCallbackStateProblem, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticResult,
    SeverityKind,
};
use bray_symbols::{
    CallableAbi, CallableInstanceData, CallableSignatureQuery, CallableTrust,
    DeclarationDirectivesQuery, DirectiveKind, GenericArgument, SymbolQueryRequest, TypeData,
    TypeExpressionTemplate,
};

use super::classification::classify_operation;
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::unit::semantic_inputs_match;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};

pub(crate) fn check_memory_operations<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    literals: &CheckedLiteralValues,
) -> CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<DeclarationDirectivesQuery>
        + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if !semantic_inputs_match(
        request,
        [
            (selections.unit(), selections.kind()),
            (literals.unit(), literals.kind()),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    }

    let mut operations = Vec::new();
    let mut read_kinds = BTreeMap::new();
    let mut diagnostics = DiagnosticBag::new();

    for entry in selections.entries() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let SemanticSelection::Call(call) = entry.selection() else {
            continue;
        };

        let BoundCallableTarget::Declaration(instance) = call.target() else {
            continue;
        };

        let resolution = match request.implementation_hook(instance.definition().symbol()) {
            Ok(resolution) => resolution,
            Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(crate::CheckerQueryError::Infrastructure(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            Err(crate::CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        let Some(resolution) = resolution else {
            continue;
        };

        if !resolution.is_available() {
            let Some(operation) =
                crate::memory_diagnostics::diagnostic_memory_operation(resolution.hook())
            else {
                continue;
            };

            let span = match expression_span(request, entry.expression()) {
                Ok(span) => span,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            crate::memory_diagnostics::add_target_memory_operation_unavailable(
                span,
                request.selected_target().identity().as_str(),
                operation,
                &mut diagnostics,
            );

            continue;
        }

        let generic_arguments = match generic_arguments(request, instance) {
            Ok(arguments) => arguments,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        let type_arguments = generic_arguments
            .iter()
            .filter_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(*ty),
                GenericArgument::Constant(_) => None,
            })
            .collect::<Vec<_>>();

        let arguments = match selected_arguments(call.arguments()) {
            Ok(arguments) => arguments,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        let target_control = match crate::target_control::check_contract(
            request,
            resolution.hook(),
            &type_arguments,
            arguments.as_slice(),
            literals,
            selections,
        ) {
            Ok(check) => check,
            Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(crate::CheckerQueryError::Infrastructure(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            Err(crate::CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        let kind = match target_control {
            crate::target_control::TargetControlCheck::Valid(kind) => kind,
            crate::target_control::TargetControlCheck::NotApplicable => {
                match classify_operation(
                    request,
                    resolution.hook(),
                    &generic_arguments,
                    entry.expression(),
                    &mut read_kinds,
                    &mut diagnostics,
                ) {
                    Ok(Some(kind)) => kind,
                    Ok(None) => continue,
                    Err(outcome) => return outcome,
                }
            }
            crate::target_control::TargetControlCheck::Invalid => {
                let Some(operation) =
                    crate::memory_diagnostics::diagnostic_memory_operation(resolution.hook())
                else {
                    return CheckerOutcome::InfrastructureFailure(
                        CheckerInfrastructureError::InvalidSemanticSelectionInput,
                    );
                };

                let span = match expression_span(request, entry.expression()) {
                    Ok(span) => span,
                    Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                };

                diagnostics.add(
                    Diagnostic::new(
                        diagnostic_id(diagnostics.len()),
                        DiagnosticKind::CheckingInvalidTargetControlContract,
                        SeverityKind::Error,
                    )
                    .with_primary_span(span)
                    .with_label(DiagnosticLabel::primary(
                        DiagnosticLabelKind::UnsupportedTargetRequirement,
                        span,
                    ))
                    .with_arg(DiagnosticArg::target_triple(
                        request.selected_target().identity().as_str(),
                    ))
                    .with_arg(DiagnosticArg::memory_operation(operation)),
                );

                continue;
            }
        };

        if resolution.hook() == bray_compiler_known::ImplementationHook::CallbackState {
            let problem = match callback_state_problem(request, arguments.as_slice()) {
                Ok(problem) => problem,
                Err(outcome) => return outcome,
            };

            if let Some(problem) = problem {
                let span = match expression_span(request, entry.expression()) {
                    Ok(span) => span,
                    Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                };

                diagnostics.add(
                    Diagnostic::new(
                        diagnostic_id(diagnostics.len()),
                        DiagnosticKind::CheckingInvalidCallbackStateContext,
                        SeverityKind::Error,
                    )
                    .with_primary_span(span)
                    .with_label(DiagnosticLabel::primary(
                        DiagnosticLabelKind::InvalidForeignBoundary,
                        span,
                    ))
                    .with_arg(DiagnosticArg::callback_state_problem(problem))
                    .with_note(DiagnosticNote::new(
                        DiagnosticNoteKind::CallbackStateRequirements,
                    )),
                );

                continue;
            }
        }

        operations.push(CheckedMemoryOperation::new(
            entry.expression(),
            kind,
            arguments,
        ));
    }

    let operations = match CheckedMemoryOperations::try_new(
        request.unit().unit(),
        request.unit().key().kind(),
        operations,
        false,
    ) {
        Ok(operations) => operations,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            );
        }
    };

    CheckerOutcome::Complete(DiagnosticResult::new(operations, diagnostics))
}

fn callback_state_problem<C>(
    request: CheckerUnitView<'_, C>,
    arguments: &[bray_bound_tree::BoundExpressionId],
) -> Result<
    Option<DiagnosticCallbackStateProblem>,
    CheckerOutcome<CheckedMemoryOperations, C::UpstreamError>,
>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<DeclarationDirectivesQuery>
        + ?Sized,
{
    let [context] = arguments else {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    let Some(callable) = request.containing_callable() else {
        return Ok(Some(DiagnosticCallbackStateProblem::OutsideCallable));
    };

    let signature = match request
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
    {
        Ok(signature) => signature,
        Err(crate::CheckerQueryError::Cancelled) => return Err(CheckerOutcome::Cancelled),
        Err(crate::CheckerQueryError::Infrastructure(error)) => {
            return Err(CheckerOutcome::InfrastructureFailure(error));
        }
        Err(crate::CheckerQueryError::Upstream(error)) => {
            return Err(CheckerOutcome::UpstreamFailure(error));
        }
    };

    let (abi, trust) = match signature.value().callable_type() {
        TypeExpressionTemplate::Callable(callable) => (callable.abi(), callable.trust()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = request.semantic_values().type_data(*ty).map_err(|error| {
                CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::SemanticValueStore(error),
                )
            })?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ));
            };

            (callable.abi(), callable.trust())
        }
        _ => {
            return Err(CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        }
    };

    if abi == CallableAbi::Bray {
        return Ok(Some(DiagnosticCallbackStateProblem::LanguageAbi));
    }

    if trust != CallableTrust::Trusted {
        return Ok(Some(DiagnosticCallbackStateProblem::CallableNotTrusted));
    }

    if signature.value().receiver().is_some() {
        return Ok(Some(DiagnosticCallbackStateProblem::ReceiverPresent));
    }

    let directives = match request.resolve_symbol_query(SymbolQueryRequest::<
        DeclarationDirectivesQuery,
    >::new(callable.into_any()))
    {
        Ok(directives) => directives,
        Err(crate::CheckerQueryError::Cancelled) => return Err(CheckerOutcome::Cancelled),
        Err(crate::CheckerQueryError::Infrastructure(error)) => {
            return Err(CheckerOutcome::InfrastructureFailure(error));
        }
        Err(crate::CheckerQueryError::Upstream(error)) => {
            return Err(CheckerOutcome::UpstreamFailure(error));
        }
    };

    if !directives
        .value()
        .directives()
        .iter()
        .any(|directive| directive.kind() == DirectiveKind::Symbol)
    {
        return Ok(Some(DiagnosticCallbackStateProblem::MissingSymbolDirective));
    }

    let Some((parameters, receiver)) = request.symbols().callable_parameters_and_receiver(callable)
    else {
        return Err(CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    if receiver.is_some() {
        return Ok(Some(DiagnosticCallbackStateProblem::ReceiverPresent));
    }

    if parameters.is_empty() {
        return Ok(Some(
            DiagnosticCallbackStateProblem::MissingContextParameter,
        ));
    };

    let Some(BoundExpression::Name(context)) = request.view().expression(*context) else {
        return Ok(Some(DiagnosticCallbackStateProblem::ContextArgumentNotName));
    };

    let BoundReferenceTarget::Surface(target) = context.target() else {
        return Ok(Some(
            DiagnosticCallbackStateProblem::ContextArgumentNotParameter,
        ));
    };

    let Some(ordinal) = parameters
        .iter()
        .position(|parameter| bray_symbols::AnySymbolId::from(*parameter) == target)
    else {
        return Ok(Some(
            DiagnosticCallbackStateProblem::ContextArgumentNotParameter,
        ));
    };

    if ordinal != 0 {
        let actual_ordinal = u64::try_from(ordinal).map_err(|_| {
            CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            )
        })?;

        return Ok(Some(
            DiagnosticCallbackStateProblem::ContextParameterNotFirst { actual_ordinal },
        ));
    }

    Ok(None)
}

fn selected_arguments(
    arguments: &[SelectedArgument],
) -> Result<Vec<bray_bound_tree::BoundExpressionId>, CheckerInfrastructureError> {
    let mut selected = arguments
        .iter()
        .map(|argument| match argument {
            SelectedArgument::Explicit {
                expression,
                ordinal,
                ..
            } => Ok((*ordinal, *expression)),
            SelectedArgument::Default { .. } => {
                Err(CheckerInfrastructureError::InvalidSemanticSelectionInput)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    selected.sort_unstable_by_key(|(ordinal, _)| *ordinal);

    Ok(selected
        .into_iter()
        .map(|(_, expression)| expression)
        .collect())
}

fn generic_arguments<C>(
    request: CheckerUnitView<'_, C>,
    instance: CallableInstanceData,
) -> Result<Vec<GenericArgument>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let substitution = request
        .semantic_values()
        .generic_substitution_data(instance.substitution())
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(substitution
        .bindings()
        .iter()
        .map(|binding| binding.argument())
        .collect())
}
