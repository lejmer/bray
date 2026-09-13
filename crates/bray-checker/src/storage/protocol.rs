use bray_bound_tree::StorageProtocolCall;
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{TypeData, TypeId};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

pub(crate) fn selected_storage_protocol_call<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: TypeId,
    target: TypeId,
    member: &CompilerKnownDeclarationKey,
) -> Result<DiagnosticResult<Option<StorageProtocolCall>>, CheckerQueryError<C::UpstreamError>> {
    let selected = request
        .context()
        .storage_protocol_callable(storage, target, member)?;

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

impl<C: CheckerRequestContext + ?Sized> super::plan::Planner<'_, C> {
    pub(in crate::storage) fn plan_owned_borrows(
        &mut self,
        owner: bray_symbols::TypeId,
    ) -> Result<bool, super::plan::PlanError<C::UpstreamError>> {
        let owner = self
            .request
            .semantic_values()
            .unborrowed_type(owner)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let data = self
            .request
            .semantic_values()
            .type_data(owner)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let TypeData::OwnedIndirection { storage, target } = data.as_ref() else {
            return Ok(false);
        };

        let mut complete = true;

        for (kind, member) in [
            (bray_symbols::BorrowKind::Shared, "StorageBorrow"),
            (bray_symbols::BorrowKind::Mutable, "StorageBorrowMut"),
        ] {
            let member = bray_compiler_known::CompilerKnownDeclarationKey::try_new(member)
                .ok_or(CheckerInfrastructureError::InvalidStoragePlan)?;

            let selected =
                super::selected_storage_protocol_call(self.request, *storage, *target, &member)?;

            let (call, diagnostics) = selected.into_parts();

            self.diagnostics.add_range(diagnostics);

            match call {
                Some(call) => self
                    .builder_mut()?
                    .set_owned_borrow(owner, kind, call)
                    .map_err(CheckerInfrastructureError::StoragePlan)?,
                None => complete = false,
            }
        }

        Ok(complete)
    }
}
