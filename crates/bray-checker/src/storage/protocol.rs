use bray_bound_tree::StorageProtocolCall;
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::TypeId;

use crate::{CheckerQueryError, CheckerRequestContext};

pub(crate) fn selected_storage_protocol_call<C: CheckerRequestContext + ?Sized>(
    context: &C,
    storage: TypeId,
    target: TypeId,
    member: &CompilerKnownDeclarationKey,
) -> Result<DiagnosticResult<Option<StorageProtocolCall>>, CheckerQueryError<C::UpstreamError>> {
    let selected = context.storage_protocol_callable(storage, target, member)?;

    let (selected, diagnostics) = selected.into_parts();

    let call = selected
        .filter(|_| !diagnostics.has_errors())
        .and_then(|(callable, signature)| {
            let [parameter] = signature.parameters() else {
                return None;
            };

            if signature.receiver().is_some() {
                return None;
            }

            Some(StorageProtocolCall::new(
                callable,
                signature.callable_type(),
                parameter.ty(),
                signature.result(),
            ))
        });

    Ok(DiagnosticResult::new(call, diagnostics))
}
