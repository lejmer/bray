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
        let projection = self.intrinsic_projection_symbol(symbol, cancellation)?;
        let transfer = self.intrinsic_transfer_symbol(symbol, cancellation)?;

        Ok((projection || transfer)
            && declaration.domains().iter().all(|domain| {
                domain.guards.is_empty()
                    && domain.properties.iter().all(|property| {
                        (projection
                            && matches!(
                                property.property,
                                ExecutionProperty::Pure | ExecutionProperty::Total
                            ))
                            || (transfer && property.property == ExecutionProperty::Total)
                    })
            }))
    }

    fn intrinsic_transfer_symbol(
        &self,
        symbol: bray_symbols::AnySymbolId,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<bool, crate::fact::FactQueryError> {
        let context = self.checker_context(cancellation)?;

        Ok(context.implementation_hook(symbol)?.is_some_and(|hook| {
            hook.is_available()
                && matches!(
                    hook.hook(),
                    bray_compiler_known::ImplementationHook::RawBufferPush
                        | bray_compiler_known::ImplementationHook::RawBufferRelocate
                )
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
                && (hook.hook().is_value_observation()
                    || matches!(
                        hook.hook(),
                        bray_compiler_known::ImplementationHook::NumericTruncate
                            | bray_compiler_known::ImplementationHook::NumericTruncationSupported
                            | bray_compiler_known::ImplementationHook::NumericTruncationDomain
                            | bray_compiler_known::ImplementationHook::NullableIsPresent
                            | bray_compiler_known::ImplementationHook::NullableIsAbsent
                            | bray_compiler_known::ImplementationHook::SequenceLength
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
                    ))
        }))
    }

    pub(super) fn intrinsic_projection_obligation(
        &self,
        callable: CallableInstanceData,
        obligation: ExecutionObligation,
        cancellation: &crate::fact::CancellationToken,
    ) -> Result<bool, crate::fact::FactQueryError> {
        // Selected domain identities do not change a compiler operation's fixed semantics.
        let obligation = match obligation {
            ExecutionObligation::Property(property, _) => {
                ExecutionObligation::Property(property, None)
            }
            obligation => obligation,
        };

        let context = self.checker_context(cancellation)?;

        let hook = context
            .implementation_hook(callable.definition().symbol())?
            .filter(|hook| hook.is_available())
            .map(|hook| hook.hook());

        // Numeric operations are total on their checked type and value domains.
        // Caller-side precondition failures remain part of the caller's execution graph.
        if hook.is_some_and(|hook| hook.is_numeric_policy()) {
            return Ok(matches!(
                obligation,
                ExecutionObligation::Observation
                    | ExecutionObligation::Termination
                    | ExecutionObligation::Property(
                        ExecutionProperty::Pure | ExecutionProperty::Total,
                        None
                    )
            ));
        }

        if hook.is_some_and(|hook| hook.is_range_iteration())
            && (matches!(
                obligation,
                ExecutionObligation::Observation
                    | ExecutionObligation::Termination
                    | ExecutionObligation::Property(ExecutionProperty::Total, None)
            ) || hook != Some(bray_compiler_known::ImplementationHook::RangeNext)
                && obligation == ExecutionObligation::Property(ExecutionProperty::Pure, None)
                || hook == Some(bray_compiler_known::ImplementationHook::RangeNext)
                    && obligation == ExecutionObligation::FiniteIteration)
        {
            return Ok(true);
        }

        let value_observation = hook.is_some_and(|hook| hook.is_value_observation());

        if value_observation
            && matches!(
                obligation,
                ExecutionObligation::Observation
                    | ExecutionObligation::Termination
                    | ExecutionObligation::Property(
                        ExecutionProperty::Pure | ExecutionProperty::Total,
                        None
                    )
            )
        {
            return Ok(true);
        }

        let obligation = if obligation == ExecutionObligation::Termination {
            ExecutionObligation::Property(ExecutionProperty::Total, None)
        } else {
            obligation
        };

        if !matches!(
            obligation,
            ExecutionObligation::Property(ExecutionProperty::Pure | ExecutionProperty::Total, None)
        ) {
            return Ok(false);
        }

        if self.intrinsic_projection_symbol(callable.definition().symbol(), cancellation)? {
            return Ok(true);
        }

        if matches!(
            obligation,
            ExecutionObligation::Property(ExecutionProperty::Total, None)
        ) && self.intrinsic_transfer_symbol(callable.definition().symbol(), cancellation)?
        {
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
                r#"module app;

using core.atomic;

 func operation(pos storage: &core.atomic.Atomic<u32>)
                 executes(total) {{ _ = {operation}; }}"#
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
            ("pure", "core.atomic.notify_all<u32>(storage)"),
        ] {
            let compilation = compilation(&format!(
                r#"module app;

using core.atomic;

 func operation(pos storage: &core.atomic.Atomic<u32>)
                 requires(blocking_execution()) executes({property}) {{ _ = {operation}; }}"#
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
    fn atomic_notification_is_total() {
        let notification = compilation(
            r#"
            module app;

            using core.atomic;

            func notify(pos storage: &core.atomic.Atomic<u32>) executes(total) {
                core.atomic.notify_one<u32>(storage);
                core.atomic.notify_all<u32>(storage);
            }
        "#,
        );

        assert!(
            !notification.check_diagnostics().has_errors(),
            "{:?}",
            notification.check_diagnostics()
        );
    }

    #[test]
    fn callable_address_projections_are_pure_and_total() {
        let compilation = compilation(
            r#"
            trusted module app;

            using core.memory;
            using core.memory.function;

            callable Callback = @abi(c) func();
            @abi(c)
            func entry() {}

            trusted func address() -> DataPointer<u8>
                executes(pure, total)
                uses(layout_reinterpret)
            {
                let pointer: FunctionPointer<Callback> = trusted core.memory.function_address<Callback>(entry);

                return trusted core.memory.function.to_data<Callback>(pointer);
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

            using core.memory;
            using core.memory.function;

            callable Callback = @abi(c) func();

            trusted func caller(pos pointer: DataPointer<u8>) -> Callback
                uses(layout_reinterpret)
            {
                let callable_pointer: FunctionPointer<Callback> = trusted core.memory.function.from_data<Callback>(pointer);

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
