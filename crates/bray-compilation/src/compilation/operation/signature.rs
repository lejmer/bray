use bray_binder::BindingQueryContext;
use bray_checker::CheckerQueryError;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableSignature, ImplementationInstanceId, ReceiverParameterSignature, SelfTypeContext,
    TypeId,
};

use super::super::binder::CompilationBindingContext;
use super::super::checker::CompilationCheckerContext;
use crate::fact::FactQueryError;

pub(super) fn member_callable_signature(
    signature: CallableSignature,
    receiver_type: TypeId,
) -> CallableSignature {
    let receiver = signature.receiver().map(|receiver| {
        ReceiverParameterSignature::new(receiver.parameter(), receiver_type, receiver.mode())
    });

    CallableSignature::new(
        signature.callable_type(),
        receiver,
        signature.parameters().iter().copied(),
        signature.result(),
    )
}

pub(super) fn normalize_callable_type_equalities(
    binding_context: &CompilationBindingContext<'_>,
    signature: CallableSignature,
    constraints: &[(
        bray_symbols::GenericOwnerId,
        bray_symbols::CheckedConstraint,
    )],
) -> Result<CallableSignature, FactQueryError> {
    let values = binding_context.semantic_values();

    signature.try_map_types(|ty| {
        super::constraint::normalize_type_equalities(values, ty, constraints)
    })
}

pub(super) fn normalize_callable_type_valued_members(
    binding_context: &CompilationBindingContext<'_>,
    signature: CallableSignature,
    witness: ImplementationInstanceId,
    diagnostics: &mut DiagnosticBag,
) -> Result<CallableSignature, FactQueryError> {
    let checker =
        CompilationCheckerContext::new(*binding_context).with_implementation_witnesses([witness]);

    bray_checker::normalize_callable_signature_type_valued_members(&checker, signature, diagnostics)
        .map_err(checker_query_error)
}

pub(super) fn substitute_callable_self(
    binding_context: &CompilationBindingContext<'_>,
    signature: CallableSignature,
    context: SelfTypeContext,
    replacement: TypeId,
) -> Result<CallableSignature, FactQueryError> {
    let values = binding_context.semantic_values();

    signature.try_map_types(|ty| {
        values
            .substitute_contextual_self(ty, context, replacement)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    })
}

const fn checker_query_error(error: CheckerQueryError) -> FactQueryError {
    match error {
        CheckerQueryError::Cancelled => FactQueryError::Cancelled,
        CheckerQueryError::Infrastructure(error) => FactQueryError::CheckerInfrastructure(error),
    }
}
