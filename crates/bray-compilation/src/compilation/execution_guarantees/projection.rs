use bray_checker::{CheckerRequestContext, ExecutionObligation, ExecutionProperty};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_symbols::{CallableInstanceData, TraitCallableFulfillmentSymbolId};

use crate::compilation::Compilation;

impl Compilation {
    pub(super) fn intrinsic_projection_declaration(
        &self,
        symbol: bray_symbols::AnySymbolId,
        declaration: &bray_checker::ExecutionDeclaration,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<bool, crate::fact::FactQueryError> {
        Ok(self.intrinsic_projection_symbol(symbol, cancellation)?
            && declaration.domains().iter().all(|domain| {
                domain.guards.is_empty()
                    && domain.postconditions.is_empty()
                    && domain.properties.iter().all(|property| {
                        matches!(
                            property.property,
                            ExecutionProperty::Pure | ExecutionProperty::Total
                        )
                    })
            }))
    }

    pub(super) fn intrinsic_projection_symbol(
        &self,
        symbol: bray_symbols::AnySymbolId,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<bool, crate::fact::FactQueryError> {
        let context = self.checker_context(cancellation)?;
        let hook = context.implementation_hook(symbol)?;

        Ok(hook.is_some_and(|hook| {
            hook.is_available()
                && matches!(
                    hook.hook(),
                    bray_compiler_known::ImplementationHook::UninitPointer
                        | bray_compiler_known::ImplementationHook::UninitPointerMut
                        | bray_compiler_known::ImplementationHook::BorrowFrom
                        | bray_compiler_known::ImplementationHook::BorrowMutFrom
                )
        }))
    }

    pub(super) fn synthetic_heap_projection_obligation(
        &self,
        callable: CallableInstanceData,
        obligation: ExecutionObligation,
    ) -> bool {
        if !matches!(
            obligation,
            ExecutionObligation::Property(ExecutionProperty::Pure | ExecutionProperty::Total, None)
        ) {
            return false;
        }

        // These catalog identities select compiler-emitted heap projections. Source storage
        // implementations retain ordinary callable proof dependencies.
        ["HeapStorageBorrow", "HeapStorageBorrowMut"]
            .into_iter()
            .any(|name| {
                CompilerKnownDeclarationKey::try_new(name)
                    .and_then(|key| {
                        self.available_compiler_known_symbols()
                            .declaration_symbol::<TraitCallableFulfillmentSymbolId>(&key)
                    })
                    .is_some_and(|symbol| callable.definition().symbol() == symbol.into())
            })
    }
}
