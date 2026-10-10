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
                    bray_compiler_known::ImplementationHook::SequenceLength
                        | bray_compiler_known::ImplementationHook::ByteBufferRead
                        | bray_compiler_known::ImplementationHook::SequenceIsEmpty
                        | bray_compiler_known::ImplementationHook::AddressOf
                        | bray_compiler_known::ImplementationHook::AddressOfMut
                        | bray_compiler_known::ImplementationHook::PointerFromCallable
                        | bray_compiler_known::ImplementationHook::UninitPointer
                        | bray_compiler_known::ImplementationHook::UninitPointerMut
                        | bray_compiler_known::ImplementationHook::BorrowFrom
                        | bray_compiler_known::ImplementationHook::BorrowMutFrom
                        | bray_compiler_known::ImplementationHook::RawBufferInitializedSlice
                        | bray_compiler_known::ImplementationHook::RawBufferInitializedSliceMut
                )
        }))
    }

    pub(super) fn intrinsic_projection_obligation(
        &self,
        callable: CallableInstanceData,
        obligation: ExecutionObligation,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<bool, crate::fact::FactQueryError> {
        if !matches!(
            obligation,
            ExecutionObligation::Property(ExecutionProperty::Pure | ExecutionProperty::Total, None)
        ) {
            return Ok(false);
        }

        if self.intrinsic_projection_symbol(callable.definition().symbol(), cancellation)? {
            return Ok(true);
        }

        // These catalog identities select compiler-emitted heap projections. Source storage
        // implementations retain ordinary callable proof dependencies.
        Ok(["HeapStorageBorrow", "HeapStorageBorrowMut"]
            .into_iter()
            .any(|name| {
                CompilerKnownDeclarationKey::try_new(name)
                    .and_then(|key| {
                        self.available_compiler_known_symbols()
                            .declaration_symbol::<TraitCallableFulfillmentSymbolId>(&key)
                    })
                    .is_some_and(|symbol| callable.definition().symbol() == symbol.into())
            }))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;
    use bray_diagnostics::DiagnosticKind;

    #[test]
    fn non_waiting_atomic_operations_complete_without_cleanup_reporting() {
        for operation in [
            "core.atomic.initialize<u32>(0)",
            "core.atomic.load<u32, 1>(storage)",
            "core.atomic.store<u32, 2>(storage, 1)",
            "core.atomic.exchange<u32, 0>(storage, 1)",
            "core.atomic.compare_exchange<u32, 0, 0>(storage, expected = 0, desired = 1)",
            "core.atomic.compare_exchange_weak<u32, 0, 0>(storage, expected = 0, desired = 1)",
            "core.atomic.fetch_add<u32, 2>(storage, 1)",
            "core.atomic.fetch_sub<u32, 0>(storage, 1)",
            "core.atomic.fetch_and<u32, 0>(storage, 1)",
            "core.atomic.fetch_or<u32, 0>(storage, 1)",
            "core.atomic.fetch_xor<u32, 0>(storage, 1)",
        ] {
            let compilation = compilation(&format!(
                "module app; func operation(pos storage: &core.atomic.Atomic<u32>) \
                 executes(total) {{ let _ = {operation}; }}"
            ));

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{operation}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn atomic_termination_evidence_preserves_wait_and_effect_constraints() {
        for (property, operation) in [
            ("total", "core.atomic.wait<u32, 1>(storage, 0)"),
            ("pure", "core.atomic.load<u32, 1>(storage)"),
            ("pure", "core.atomic.fetch_add<u32, 2>(storage, 1)"),
        ] {
            let compilation = compilation(&format!(
                "module app; func operation(pos storage: &core.atomic.Atomic<u32>) \
                 executes({property}) {{ let _ = {operation}; }}"
            ));

            assert!(
                compilation.check_diagnostics().iter().any(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::CheckingExecutionGuaranteeNotProven
                }),
                "{property}, {operation}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn callable_address_projections_are_pure_and_total() {
        let compilation = compilation(
            r#"
            trusted module app;
            callable Callback = @abi(c) func();
            @abi(c)
            func entry() {}

            trusted func address() -> RawPointer<u8>
                executes(pure, total)
                uses(layout_reinterpret)
            {
                let pointer: RawPointer<Callback> = trusted core.memory.pointer_from_callable<Callback>(entry);

                return trusted core.memory.reinterpret<u8, Callback>(pointer);
            }
            "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn pointer_projection_certificates_do_not_create_callable_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            callable Callback = @abi(c) func();

            trusted func caller(pos pointer: RawPointer<u8>) -> Callback
                uses(layout_reinterpret)
            {
                let callable_pointer: RawPointer<Callback> = trusted core.memory.reinterpret<Callback, u8>(pointer);

                return trusted core.memory.callable_from_pointer<Callback>(callable_pointer);
            }
            "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
}
